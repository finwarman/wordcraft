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
| Toolbar command | Flat at rest, raised on hover, recessed while pressed |
| Toggle | Recessed with a dark outline while selected |
| Tool category | Connected active tab; narrow windows expose remaining tabs through More |
| Workspace / ruler | Muted page surround, pale ruler with distinct margin zones |
| Input / document | White input, readable dark ink; stored document styling remains separate |
| Focus | Existing keyboard routing and explicit focus outline |
| Disabled / pending | Existing enabled predicates; update action coalesces while pending and recovers after an error |
| Help | 220ms hover delay; Classic hover and press feedback has no fade |
| Fonts | Installed Tahoma, Verdana or Arial, otherwise licensed bundled UI fonts; existing script and IME fallbacks retained |

The native host loads at most two installed UI faces once on a worker, each capped at 2MiB. Font installation occurs on a profile/language/fallback change. Pink and Classic share that font profile. No proprietary font is bundled. The document renderer retains its own font selection.

Discord controls, commands, links and settings are removed. Mailings and Developer start hidden; Tabs… shows visibility checkboxes and the full ribbon editor. Saved explicit layouts retain their choices. File, Edit and View hold export, editing mode and comments instead of duplicate prominent toolbar controls. The menu bar dispatches existing commands. The ribbon remains available so the skin does not remove features. View or Interface Theme changes the skin; File opens documents, print settings, recent files and Options. Keyboard shortcuts, native file dialogs and save guards use their existing paths.

## Fork identity and updates

Fin's preferences, recent files, custom fonts, control keys and personal settings live in the isolated `WordCraft Fin` profile on macOS/Windows, or `wordcraft-fin` on Unix. No automatic import from official WordCraft occurs. Shared operating-system fonts and spelling dictionaries remain readable.

Help / App updates opens `dev.f1nb0.launcher` on macOS, falling back to `https://f1nb0.dev/apps/`. Process I/O runs on a worker with a bounded wait. Other hosts use the download page. There is no new background updater, timer or document mutation.

## Qualification

Use generated documents with headless `ui_shot` for Classic, Pink, dialogs and options. Check narrow, normal and wide windows at 100%, 150% and 200% interface size, including overflow, arrow space, toggle state and keyboard operation. The status strip retains page/word count and zoom in its compact layout. Check the shared-source digest and Mail's native hidden UI checks. Document conversion and the existing editing suite remain required. These checks do not establish live VoiceOver coverage or a whole-application startup, memory or leak budget.

Native template thumbnails run one worker at a time, cache at most eight results (including failures), cap scale at 3x, and release five seconds after leaving Home. The first frame does not wait for thumbnail rasterization. WASM retains synchronous on-demand rendering.

`scripts/qualify_native.py` measures first usable frames, RSS and physical footprint using synthetic hidden windows with preferences disabled. It does not flush OS caches or prove leak freedom. Keep startup and renderer memory limits in ticket2168 until native qualification is complete.

## 2168 graphics and accessibility pass

On macOS, Classic/Pink use a square application frame with named minimize, maximize and close controls. Close uses the existing unsaved-document guard. Home has the same caption and a separate Back to document row. Light/Dark/System keep native decorations. Caption commands use light ink; disabled commands use the lighter caption-end colour and remain disabled to keyboard/accessibility actions. The user icon opens Options. Compact captions reduce search to an icon rather than overlapping the window controls.

The macOS surface uses egui-wgpu LOW_LATENCY with vsync. In repeated 1440x920/2x synthetic Metal probes, limiting the drawable queue saved 21,463,040 bytes when a third drawable was used. This is a resource-allocation result, not a whole-process reduction or input-latency claim. See [wgpu surface behavior](https://wgpu.rs/doc/wgpu/api/surface/type.SurfaceConfiguration.html). Page textures have a 64 MiB cache budget with farthest offscreen eviction; visible pages retain full resolution and can exceed that budget. No document/export quality reduction is used.

`graphics_probe` is a non-activating synthetic example that records owned texture sizes and the default Metal device's allocation, plus optional UI callback intervals. It reads no user preferences or documents. Hidden-window callbacks can run without presentation, so their cadence is not display FPS. Metal allocation and process physical footprint measure different things; [Apple's device-memory definition](https://developer.apple.com/documentation/metal/mtldevice/recommendedmaxworkingsetsize) does not include every driver/process allocation.

Native qualification now reports installed UI-face count/bytes/readiness and aggregate texture metadata through explicit ui.inspect calls. The native font worker remains enabled with WORDCRAFT_NO_PREFS; only preferences, the personal dictionary and custom added fonts are disabled. Earlier notes calling these bundled-font-only native runs were incorrect. The separate graphics example uses Services::default and bundled UI fallbacks.

Keyboard/AccessKit regression checks cover named caption/status controls, AutoSave state, User settings, Back placement/keyboard activation and Close's unsaved prompt. The native macOS tree is inspected after installation. Live VoiceOver speech and a long idle/scrolling soak remain separate 2168 gates.

Page rasters now cover the visible viewport plus bounded prefetch/padding, rather than allocating whole high-zoom pages. Integer pixel origins retain device resolution. A crop is reused until scrolling leaves its padding. Paper, hit testing, selection, accessibility and export still use whole-page geometry. Object-drag previews show the cached portion of large objects without stretching a cropped edge. Extremely large viewports retain a texture-limit fallback.

The synthetic native 500% scene reduced aggregate textures from209,163,456 to31,069,376bytes, about85%. Its zoom frame fell from109-122ms to30-33ms including20ms control polling. This is a hidden-window result, not input-to-display latency. Physical footprint fell from about1.0-1.2GB to528-574MB in these short samples. Driver allocation remains substantial. Startup samples include a first observed1.5s run and a178ms warm run; they do not establish a startup improvement. The crop comparison allows at most two colour levels of CPU edge rounding. AccessKit checks preserve full text and the caret at500% zoom. Live VoiceOver and longer real-document scrolling remain open.

## 2181 icons and controls

The displayed app name is WordCraft; the Fin installation filename, bundle identifier, profile and update identity remain separate. Original Classic/Pink document-and-pencil artwork uses the shared palette. A bounded pair of native images is decoded only on demand. The Dock redraws only when the theme changes and restores from the saved theme at launch. Finder and the closed-app icon remain Classic: custom Finder metadata failed strict signature verification in a disposable qualification bundle. Signed resources are never changed at runtime.

Caption marks use font-independent geometry inside the bevel; maximize changes to Restore while maximized. Ribbon scroll controls sit on opaque framed rails, with accessible names and focus rings.

## Optional cursor sparkles

File / Options / General offers Cursor sparkles, off by default and saved per profile. The trail shares ClassicKit palette roles and metrics with Mail. At most32 small particles live for450ms, with idle decay repaint requests at most every33ms. Pointer-driven frames retain the native UI cadence. Dragging/clicking, missing pointer, inactive/minimized windows, animation effects Off and system Reduce Motion clear the decoration. After particles expire it requests no repaint. It uses a fixed array and painter geometry, no image textures, cursor hooks, worker or full-window backing surface. Document editing/export and pointer events are unchanged. ui.cursorSparkles reads/sets the saved preference and reports its effective enabled state.

Caption gradients retain the original dark/light palette endpoints. The fade starts at15% and reaches40% blending at70%, then continues to the light right edge. This broadens the light field while keeping white central title text at least4.5:1 against the middle stop in both palettes. Shared metrics control both toolkit adapters.
