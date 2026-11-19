//! Original pixel-aligned bevel and caption adapters for the shared ClassicKit roles.
use super::{Tokens, shared};
use egui::{Color32, Painter, Rect, pos2};

pub(crate) enum Edge {
    Raised,
    Sunken,
}
pub(crate) fn bevel(painter: &Painter, rect: Rect, edge: Edge, t: &Tokens) {
    if !t.classic || rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    let (top, bottom, inner_top, inner_bottom) = match edge {
        Edge::Raised => (t.bevel_light, t.bevel_dark, t.ribbon, t.bevel_shadow),
        Edge::Sunken => (t.bevel_shadow, t.bevel_light, t.bevel_dark, t.ribbon),
    };
    edges(painter, rect, top, bottom);
    edges(painter, rect.shrink(shared::BORDER_WIDTH), inner_top, inner_bottom);
}
fn edges(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let w = shared::BORDER_WIDTH;
    painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + w)), 0.0, top);
    painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.min.x + w, rect.max.y)), 0.0, top);
    painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, rect.max.y - w), rect.max), 0.0, bottom);
    painter.rect_filled(Rect::from_min_max(pos2(rect.max.x - w, rect.min.y), rect.max), 0.0, bottom);
}
pub(crate) fn caption(painter: &Painter, rect: Rect, t: &Tokens) {
    if !t.classic {
        return;
    }
    // Keep white caption text on the dark field; fade only beneath face-filled right controls.
    let split = rect.left() + rect.width() * 0.75;
    painter.rect_filled(Rect::from_min_max(rect.min, pos2(split, rect.bottom())), 0.0, t.title_bar);
    let mut mesh = egui::Mesh::default();
    for (position, color) in [
        (pos2(split, rect.top()), t.title_bar),
        (rect.right_top(), t.title_end),
        (rect.right_bottom(), t.title_end),
        (pos2(split, rect.bottom()), t.title_bar),
    ] {
        mesh.colored_vertex(position, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}
