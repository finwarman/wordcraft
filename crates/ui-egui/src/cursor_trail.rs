//! Decorative, fixed-budget pointer particles. No textures, input capture or idle repaint loop.
use super::{Tokens, shared};
use egui::{Context, Pos2, Stroke, Vec2};

const CAPACITY: usize = shared::SPARKLE_MAX_PARTICLES as usize;
const LIFETIME: f64 = shared::SPARKLE_LIFETIME_MS as f64 / 1000.0;
const FRAME: std::time::Duration = std::time::Duration::from_millis(shared::SPARKLE_FRAME_MS as u64);

#[derive(Clone, Copy)]
struct Particle {
    position: Pos2,
    born: f64,
}

#[derive(Clone, Copy)]
pub(crate) enum TrailMode {
    Enabled,
    Suppressed,
}

pub(crate) struct Trail {
    particles: [Option<Particle>; CAPACITY],
    next: usize,
    last: Option<Pos2>,
}
impl Default for Trail {
    fn default() -> Self {
        Self { particles: [None; CAPACITY], next: 0, last: None }
    }
}
impl Trail {
    fn clear(&mut self) {
        self.particles.fill(None);
        self.last = None;
    }
    fn advance(&mut self, position: Pos2, now: f64) {
        if !position.is_finite() || !now.is_finite() {
            self.clear();
            return;
        }
        for particle in &mut self.particles {
            if particle.is_some_and(|p| now - p.born >= LIFETIME || now < p.born) {
                *particle = None;
            }
        }
        let last = self.last.get_or_insert(position);
        if last.distance(position) < shared::SPARKLE_SPACING {
            return;
        }
        *last = position;
        if let Some(slot) = self.particles.get_mut(self.next) {
            *slot = Some(Particle { position, born: now });
        }
        self.next = (self.next + 1) % CAPACITY;
    }
    pub(crate) fn show(&mut self, ctx: &Context, mode: TrailMode) {
        let (point, now, active) = ctx.input(|i| {
            (i.pointer.hover_pos(), i.time, i.viewport().focused != Some(false) && i.viewport().minimized != Some(true) && !i.pointer.any_down())
        });
        let Some(position) = point.filter(|_| active && matches!(mode, TrailMode::Enabled)) else {
            self.clear();
            return;
        };
        self.advance(position, now);
        let tokens = Tokens::get(ctx);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("cursor_sparkles")));
        let mut live = false;
        for (index, p) in self.particles.iter().enumerate().filter_map(|(i, p)| p.map(|p| (i, p))) {
            live = true;
            let age = ((now - p.born) / LIFETIME).clamp(0.0, 1.0) as f32;
            let size = shared::SPARKLE_RADIUS * (1.0 - age);
            let center = p.position + egui::vec2(if index % 2 == 0 { -5.0 } else { 5.0 }, 8.0 + age * 10.0);
            let stroke = Stroke::new(1.0, tokens.accent.gamma_multiply(1.0 - age));
            for vector in [Vec2::X, Vec2::Y] {
                painter.line_segment([center - vector * size, center + vector * size], stroke);
            }
        }
        if live {
            ctx.request_repaint_after(FRAME);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trail_budget_and_idle_release() {
        let mut trail = Trail::default();
        for i in 0..10_000 {
            trail.advance(egui::pos2(i as f32 * 10.0, 20.0), i as f64 / 1000.0);
        }
        assert_eq!(trail.particles.iter().flatten().count(), CAPACITY);
        let point = trail.last.unwrap();
        trail.advance(point, 11.0);
        assert_eq!(trail.particles.iter().flatten().count(), 0);
        trail.advance(egui::pos2(100_020.0, 20.0), 11.01);
        trail.clear();
        assert_eq!(trail.particles.iter().flatten().count(), 0);
        assert!(trail.last.is_none());
    }
    #[test]
    fn trail_stops_repainting() {
        let ctx = Context::default();
        let mut trail = Trail::default();
        let mut previous = None;
        let mut frame = |x: f32, now: f64, mode| {
            let events = if previous == Some(x) { Vec::new() } else { vec![egui::Event::PointerMoved(egui::pos2(x, 30.0))] };
            previous = Some(x);
            let input = egui::RawInput { time: Some(now), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| trail.show(ui.ctx(), mode));
            let delay = out.viewport_output.get(&egui::ViewportId::ROOT).unwrap().repaint_delay;
            out.textures_delta.clear();
            delay
        };
        for i in 0..4 {
            frame(20.0, i as f64, TrailMode::Suppressed);
        }
        frame(20.0, 5.0, TrailMode::Enabled);
        let active = frame(40.0, 5.1, TrailMode::Enabled);
        assert!(active <= FRAME);
        frame(40.0, 6.0, TrailMode::Enabled);
        let idle = frame(40.0, 7.0, TrailMode::Enabled);
        assert!(idle > std::time::Duration::from_secs(1), "idle trail requested {idle:?}");
    }
}
