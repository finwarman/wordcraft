//! Prefer an installed classic UI family. Never ship proprietary font files.
use std::sync::mpsc;
use wordcraft_fonts::FontDb;

const FAMILIES: [&str; 3] = ["Tahoma", "Verdana", "Arial"];
const MAX_FACE_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn hook(ctx: &egui::Context) -> wordcraft_ui_egui::theme::UiFontHook {
    let ctx = ctx.clone();
    Box::new(move || {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let db = FontDb::global();
            let mut faces = Vec::new();
            for family in FAMILIES {
                if !db.has_family(family) {
                    continue;
                }
                let regular = db.face(family, "Regular");
                let bold = db.face(family, "Bold");
                if regular.data().len() > MAX_FACE_BYTES || bold.data().len() > MAX_FACE_BYTES {
                    continue;
                }
                faces.extend([regular, bold]);
                break;
            }
            let _ = tx.send(faces);
            ctx.request_repaint();
        });
        rx
    })
}
