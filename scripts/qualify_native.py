#!/usr/bin/env python3
"""Measure a non-activating synthetic native app. Never read/write user preferences."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import statistics
import subprocess
import time

KEY_DIR = Path.home() / 'Library/Application Support/WordCraft Fin'
START_LIMIT = 15
POLL = 0.02


def rss(pid):
    result = subprocess.check_output(['ps', '-o', 'rss=', '-p', str(pid)], text=True)
    return int(result.strip()) * 1024


def run(binary, cycles, idle, scene):
    with socket.socket() as port_socket:
        port_socket.bind(('127.0.0.1', 0))
        port = port_socket.getsockname()[1]
    key_path = KEY_DIR / f'control-key.local-{port}'
    env = dict(os.environ, WORDCRAFT_NO_PREFS='1')
    started = time.perf_counter()
    process = subprocess.Popen([str(binary), '--background-ui', '--sample' if scene == 'sample' else '--start-screen', '--control', str(port)], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    connection = None
    stream = None
    request_id = 0

    def request(method, params):
        nonlocal request_id
        request_id += 1
        stream.write((json.dumps({'id': request_id, 'key': key, 'method': method, 'params': params}) + '\n').encode())
        stream.flush()
        reply = json.loads(stream.readline())
        if not reply.get('ok'):
            raise RuntimeError(str(reply.get('error', 'Control request failed')))
        return reply['result']

    def command(name, params=None):
        return request('engine.execute', {'command': name, 'params': params or {}})

    try:
        while time.perf_counter() - started < START_LIMIT:
            if process.poll() is not None:
                raise RuntimeError('Probe exited before readiness')
            if key_path.exists():
                key = key_path.read_text().strip()
                try:
                    connection = socket.create_connection(('127.0.0.1', port), timeout=1)
                    connection.settimeout(START_LIMIT)
                    stream = connection.makefile('rwb')
                    break
                except OSError:
                    pass
            time.sleep(POLL)
        if stream is None:
            raise RuntimeError('Native startup deadline exceeded')
        while time.perf_counter() - started < START_LIMIT:
            state = request('ui.inspect', {})
            if state['perf']['frameMs'] > 0 and (scene == 'start' or state.get('canvasRect')):
                break
            time.sleep(POLL)
        else:
            raise RuntimeError('First usable native UI frame deadline exceeded')
        startup_ms = (time.perf_counter() - started) * 1000
        while state.get('uiFonts', {}).get('pending', False):
            if time.perf_counter() - started >= START_LIMIT:
                raise RuntimeError('Installed UI fonts deadline exceeded')
            time.sleep(POLL)
            state = request('ui.inspect', {})
        fonts_ready_ms = (time.perf_counter() - started) * 1000
        time.sleep(1)
        memory = [{'cycle': 0, 'rssBytes': rss(process.pid)}]
        timings = []
        for cycle in range(cycles):
            before = time.perf_counter()
            command('ui.theme', {'value': 'pink' if cycle % 2 else 'classic'})
            command('ui.dialog', {'name': 'font'})
            command('ui.closeDialog')
            timings.append((time.perf_counter() - before) * 1000)
            if (cycle + 1) % 50 == 0:
                memory.append({'cycle': cycle + 1, 'rssBytes': rss(process.pid)})
        time.sleep(idle)
        memory.append({'cycle': cycles, 'idleSeconds': idle, 'rssBytes': rss(process.pid)})
        result = {'startupFirstFrameMs': startup_ms, 'fontsReadyMs': fonts_ready_ms, 'uiFonts': state.get('uiFonts'), 'memory': memory, 'roundTripSamples': len(timings)}
        if timings:
            result['themeDialogRoundTripP95Ms'] = sorted(timings)[int(len(timings) * .95) - 1]
        settled = request('ui.inspect', {})
        result['settledFrame'] = settled['perf']
        result['textures'] = settled.get('textures')
        # OS process memory includes GPU mappings and shared pages; RSS is not a leak verdict.
        vmmap = subprocess.run(['vmmap', '-summary', str(process.pid)], capture_output=True, text=True, timeout=15)
        result['vmmapSummary'] = vmmap.stdout
        result['footprintSummary'] = [line.strip() for line in vmmap.stdout.splitlines() if 'Physical footprint' in line]
        command('file.close')
        process.wait(timeout=10)
        return result
    finally:
        if stream:
            stream.close()
        if connection:
            connection.close()
        if process.poll() is None:
            # Only this owned, synthetic, preference-free probe can be terminated on failure.
            process.terminate()
            process.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--scene', choices=['sample','start'], default='sample')
    parser.add_argument('--cycles', type=int, default=200)
    parser.add_argument('--idle', type=int, default=5)
    args = parser.parse_args()
    if not 1 <= args.runs <= 10 or not 0 <= args.cycles <= 1000 or not 0 <= args.idle <= 30:
        parser.error('Probe bounds exceeded')
    results = [run(args.binary.resolve(), args.cycles if index == 0 else 0, args.idle, args.scene) for index in range(args.runs)]
    report = {'scene': args.scene, 'binarySha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'runs': results,
              'startupMedianMs': statistics.median(value['startupFirstFrameMs'] for value in results),
              'limits': 'Native hidden window, synthetic document/start screen, no preferences, loopback readiness includes 20ms polling. OS caches are not flushed. RSS and vmmap snapshots do not prove leak freedom. No mail/provider access.'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: value for key, value in report.items() if key != 'runs'}))


if __name__ == '__main__':
    main()
