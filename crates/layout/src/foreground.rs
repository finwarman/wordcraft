use super::Placed;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) enum PaintLayer {
    Foreground,
    CellFill,
    Before(usize),
}

impl PaintLayer {
    fn offset(self, offset: usize) -> Self {
        match self {
            Self::Before(anchor) => Self::Before(anchor.saturating_add(offset)),
            other => other,
        }
    }
}

/// A foreground group's indices exclude the prepended behind-text items.
#[derive(Clone)]
pub(super) struct Foreground {
    pub(super) range: std::ops::Range<usize>,
    pub(super) z: u32,
    pub(super) layer: PaintLayer,
    /// Earlier paragraph-local ranks, including behind-text drawings.
    pub(super) floor: u32,
}

/// Keep each object's frame and text together until its story is complete.
pub(super) fn append_foreground(items: &mut Vec<Placed>, ranges: &mut Vec<Foreground>, behind: usize, front: Vec<(u32, u32, Vec<Placed>)>) {
    for (z, floor, group) in front {
        let start = items.len().saturating_sub(behind);
        ranges.push(Foreground { range: start..start.saturating_add(group.len()), z, floor, layer: PaintLayer::Foreground });
        items.extend(group);
    }
}

/// Carry table-cell groups into their owning story without changing item order.
pub(super) fn extend_foreground(out: &mut Vec<Foreground>, groups: &[Foreground], offset: usize) {
    out.extend(groups.iter().map(|g| Foreground {
        range: g.range.start.saturating_add(offset)..g.range.end.saturating_add(offset),
        z: g.z,
        layer: g.layer.offset(offset),
        floor: g.floor,
    }));
}

/// Keep each table's shading below its own borders, after preceding content.
pub(super) fn extend_table_paint(out: &mut Vec<Foreground>, groups: &[Foreground], offset: usize, anchor: usize) {
    let start = out.len();
    extend_foreground(out, groups, offset);
    for group in out.iter_mut().skip(start) {
        if matches!(group.layer, PaintLayer::CellFill) {
            group.layer = PaintLayer::Before(anchor);
        }
    }
}

pub(super) fn remap_foreground(groups: &[Foreground], map: &[std::ops::Range<usize>]) -> Vec<Foreground> {
    groups
        .iter()
        .filter_map(|g| {
            let mut kept = map.get(g.range.clone())?.iter().filter(|r| !r.is_empty());
            let first = kept.next()?;
            let layer = match g.layer {
                PaintLayer::Before(anchor) => PaintLayer::Before(map.get(anchor..)?.iter().find(|r| !r.is_empty())?.start),
                other => other,
            };
            Some(Foreground { range: first.start..kept.next_back().unwrap_or(first).end, z: g.z, floor: g.floor, layer })
        })
        .collect()
}

/// Foreground floats cover later inline content. Defer sorting until placement and vertical
/// alignment finish, keeping their indices valid for pagination and column balancing.
pub(super) fn order_foreground(items: &mut Vec<Placed>, ranges: &[Foreground], behind: usize) {
    if ranges.is_empty() {
        return;
    }
    let mut marked = vec![None; items.len()];
    for (i, group) in ranges.iter().enumerate() {
        let start = group.range.start.saturating_add(behind).min(marked.len());
        let end = group.range.end.saturating_add(behind).min(marked.len());
        if let Some(slice) = marked.get_mut(start..end) {
            slice.fill(Some(i));
        }
    }
    let mut before: HashMap<usize, Vec<Placed>> = HashMap::new();
    for (item, group) in items.iter().zip(&marked) {
        let Some(group) = group.and_then(|i| ranges.get(i)) else { continue };
        let anchor = match group.layer {
            PaintLayer::Before(anchor) => anchor,
            PaintLayer::CellFill => group.range.start,
            PaintLayer::Foreground => continue,
        };
        before.entry(anchor.saturating_add(behind).min(items.len())).or_default().push(item.clone());
    }
    let mut z_top = 0;
    let mut front: Vec<_> = ranges
        .iter()
        .map(|g| {
            z_top = z_top.max(g.z).max(g.floor);
            (if g.z == 0 { z_top } else { g.z }, Vec::new())
        })
        .collect();
    let len = items.len();
    for (i, (item, group)) in std::mem::take(items).into_iter().zip(marked).enumerate() {
        if let Some(group) = before.remove(&i) {
            items.extend(group);
        }
        match group.and_then(|index| ranges.get(index).map(|g| (index, g.layer))) {
            Some((index, PaintLayer::Foreground)) => {
                if let Some((_, group)) = front.get_mut(index) {
                    group.push(item);
                }
            }
            Some(_) => {}
            None => items.push(item),
        }
    }
    if let Some(group) = before.remove(&len) {
        items.extend(group);
    }
    front.sort_by_key(|(z, _)| *z);
    for (_, group) in front {
        items.extend(group);
    }
}
