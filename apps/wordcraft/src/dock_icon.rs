//! Theme-driven Dock artwork. Never modify signed bundle files or Finder metadata.
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSApplication, NSImage, NSImageView};
use objc2_foundation::{NSBundle, NSData, NSPoint, NSRect};
use wordcraft_ui_egui::theme::Appearance;

const CLASSIC: &[u8] = include_bytes!("../../../assets/app-icon/fin/classic.png");
const PINK: &[u8] = include_bytes!("../../../assets/app-icon/fin/pink.png");
const BUNDLE_ID: &str = "dev.f1nb0.wordcraft";

#[derive(Default)]
pub(super) struct DockIcon {
    active: Option<Appearance>,
    view: Option<Retained<NSImageView>>,
    images: [Option<Retained<NSImage>>; 2],
    enabled: bool,
}

impl DockIcon {
    pub(super) fn new() -> Self {
        let enabled = std::env::var_os("WORDCRAFT_NO_PREFS").is_none()
            && NSBundle::mainBundle().bundleIdentifier().is_some_and(|id| id.to_string() == BUNDLE_ID);
        Self { enabled, ..Self::default() }
    }

    pub(super) fn update(&mut self, theme: Appearance) {
        // Both retro variants share fixed-size cached artwork. Other skins use Classic.
        let theme = if theme == Appearance::Pink { Appearance::Pink } else { Appearance::Classic };
        if !self.enabled || self.active == Some(theme) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        let index = usize::from(theme == Appearance::Pink);
        if self.images.get(index).is_some_and(Option::is_none) {
            let data = NSData::with_bytes(if index == 1 { PINK } else { CLASSIC });
            if let Some(slot) = self.images.get_mut(index) {
                *slot = NSImage::initWithData(NSImage::alloc(), &data);
            }
        }
        let Some(image) = self.images.get(index).and_then(Option::as_ref) else {
            self.active = Some(theme);
            log::warn!("Dock icon could not be decoded");
            return;
        };
        let dock = NSApplication::sharedApplication(mtm).dockTile();
        let view = self.view.get_or_insert_with(|| NSImageView::initWithFrame(NSImageView::alloc(mtm), NSRect::new(NSPoint::ZERO, dock.size())));
        view.setImage(Some(image));
        dock.setContentView(Some(view));
        dock.display();
        self.active = Some(theme);
    }
}
