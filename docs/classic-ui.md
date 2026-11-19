# WordCraft Fin interface

Classic and Pink are application skins over the existing Rust/egui command UI. They never rewrite document colours, fonts, layout or exports. Light, Dark and System remain available. A fresh Fin profile starts in Classic.

## Shared theme source

Square Mail owns the reusable ClassicKit package at `ui/themes.json` and `ui/generate.py`. It contains application-independent palette roles and logical metrics. The checked-in Rust adapter is `crates/ui-egui/src/theme_shared.rs`; the Swift adapter is `macos/ClassicKit/SharedClassicTokens.swift` in Mail. Both include the same source SHA256. Standalone builds use compiled constants and need no other checkout, parser, theme server or runtime file watcher.

From a Mail checkout:

```sh
python3 ui/generate.py --wordcraft /absolute/path/to/wordcraft
python3 ui/generate.py --wordcraft /absolute/path/to/wordcraft --check
```

Commit the source and both regenerated adapters together. Editing Pink in Mail's source and running this command propagates it to both builds. Generated adapters are snapshots, so updating an installed application still needs a new build through f1nb0 Apps.

## Design contract

The reference language is late-1990s desktop tooling: square surfaces, explicit raised and recessed edges, dense labelled controls, a dark caption field, small menus, a ruler and status strip. Pink uses the same hierarchy with pale rose faces, dark plum ink and selection, and a pink caption end. This is an original implementation using existing original WordCraft icons. Proprietary reference screenshots and fonts are not distributed.

| Role | Treatment |
| --- | --- |
| Caption | Dark field beneath white text, gradient only at the right end |
| Toolbar / menu / dialog | Shared face, square corners, no blurred shadow |
| Command | Raised bevel, recessed while pressed or selected |
| Input / document | White input, readable dark ink; stored document styling remains separate |
| Focus | Existing keyboard routing and explicit focus outline |
| Disabled / pending | Existing enabled predicates; update action coalesces while pending and recovers after an error |
| Help | 220ms hover delay; Classic hover and press feedback has no fade |
| Fonts | Installed Tahoma, Verdana or Arial, otherwise licensed bundled UI fonts; existing script and IME fallbacks retained |

The native host loads at most two installed UI faces once on a worker, each capped at 2MiB. Font installation occurs on a profile/language/fallback change. Pink and Classic share that font profile. No proprietary font is bundled. The document renderer retains its own font selection.

The menu bar dispatches existing commands. The ribbon remains available so the skin does not remove features. View or Interface Theme changes the skin; File opens documents, print settings, recent files and Options. Keyboard shortcuts, native file dialogs and save guards use their existing paths.

## Fork identity and updates

Fin's preferences, recent files, custom fonts, control keys and personal settings live in the isolated `WordCraft Fin` profile on macOS/Windows, or `wordcraft-fin` on Unix. No automatic import from official WordCraft occurs. Shared operating-system fonts and spelling dictionaries remain readable.

Help / App updates opens `dev.f1nb0.launcher` on macOS, falling back to `https://f1nb0.dev/apps/`. Process I/O runs on a worker with a bounded wait. Other hosts use the download page. There is no new background updater, timer or document mutation.

## Qualification

Use generated documents with headless `ui_shot` for Classic, Pink, dialogs and options. Check the shared-source digest and Mail's native hidden UI checks. Document conversion and the existing editing suite remain required. These checks do not establish live VoiceOver coverage or a whole-application startup, memory or leak budget.
