//! Original pixel-grid document and pencil emblem. Palette changes share the UI source.
use super::{Appearance, shared};

const SIZE: u32 = 512;
const GRID: u32 = 32;

/// A bounded, font-independent application icon for the native host and packaging tool.
pub fn application_icon(theme: Appearance) -> egui::IconData {
    let p = if theme == Appearance::Pink { shared::PINK } else { shared::CLASSIC };
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let (x, y) = (px * GRID / SIZE, py * GRID / SIZE);
            let mut color = None;
            // Raised window with a small original document inset.
            if (3..29).contains(&x) && (4..28).contains(&y) {
                color = Some(p.dark);
                if x < 28 && y < 27 {
                    color = Some(p.light);
                }
                if (4..28).contains(&x) && (5..27).contains(&y) {
                    color = Some(p.face);
                }
                if (5..27).contains(&x) && (6..11).contains(&y) {
                    color = Some(p.navy);
                }
                if (7..10).contains(&x) && (7..10).contains(&y) {
                    color = Some(p.paper);
                }
                if (12..24).contains(&x) && y == 8 {
                    color = Some(p.title_ink);
                }
                if (7..25).contains(&x) && (13..25).contains(&y) {
                    color = Some(p.shadow);
                }
                if (8..24).contains(&x) && (14..24).contains(&y) {
                    color = Some(p.input);
                }
                if (10..21).contains(&x) && [16, 19, 22].contains(&y) {
                    color = Some(p.navy);
                }
            }
            // Diagonal pencil sits over the lower-right corner, without a brand lettermark.
            let diagonal = x + y;
            if (18..30).contains(&x) && (15..29).contains(&y) && (43..=47).contains(&diagonal) {
                color = Some(if diagonal == 43 || diagonal == 47 { p.dark } else { p.blue });
                if x >= 28 {
                    color = Some(p.navy);
                }
                if x <= 19 {
                    color = Some(p.paper);
                }
            }
            match color {
                Some(c) => rgba.extend_from_slice(&[((c >> 16) & 255) as u8, ((c >> 8) & 255) as u8, (c & 255) as u8, 255]),
                None => rgba.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
    egui::IconData { rgba, width: SIZE, height: SIZE }
}
