//! Flow plain merged-cell text through physical fragments of unsplit rows.
use super::*;
use crate::PageBuilder;
use wordcraft_doc::para::InlineObject;

const FLOW_EPS: f32 = 0.01;

struct CellFlow {
    first: usize,
    last: usize,
    column: usize,
    pending: Option<RowLayout>,
    frame: Placed,
    fill: Option<Placed>,
    align: VAlign,
    origin: Option<(f32, f32)>,
    height: f32,
    bottom: f32,
    done: bool,
}

pub(crate) struct MergeFlow {
    cells: Vec<CellFlow>,
    by_row: Vec<Vec<usize>>,
    by_key: std::collections::HashMap<(usize, usize), usize>,
    active: Vec<usize>,
}

impl MergeFlow {
    pub(crate) fn disabled() -> Self {
        Self { cells: Vec::new(), by_row: Vec::new(), by_key: Default::default(), active: Vec::new() }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub(crate) fn new(t: &Table, laid: &TableLayout, capacity: f32) -> Self {
        if !capacity.is_finite()
            || capacity <= 0.0
            || laid.rows.iter().flat_map(|r| &r.items).any(|it| matches!(it, Placed::Lines { para, .. } if !para.notes.is_empty()))
        {
            return Self::disabled();
        }
        let mut cells = Vec::new();
        for (ri, row) in laid.rows.iter().enumerate() {
            for cell in &row.cells {
                let Some(edge) = cell.merge.as_ref().filter(|e| e.key.0 == ri) else { continue };
                let last = (ri + 1..laid.rows.len())
                    .take_while(|&i| laid.rows.get(i).is_some_and(|r| r.cells.iter().any(|c| c.merge.as_ref().is_some_and(|e| e.key == edge.key))))
                    .last()
                    .unwrap_or(ri);
                if t.rows.get(ri..=last).is_none_or(|rows| {
                    rows.iter().any(|r| {
                        r.cells.iter().any(|cell| {
                            cell.props.hide_mark
                                || cell.blocks.iter().any(|b| {
                                    b.as_para().is_none_or(|p| {
                                        p.objects.iter().any(|o| !matches!(o, InlineObject::BookmarkStart { .. } | InlineObject::BookmarkEnd { .. }))
                                            || p.plain_text().contains(['\u{000c}', '\u{000e}'])
                                    })
                                })
                        })
                    })
                }) {
                    continue;
                }
                if last == ri
                    || t.rows
                        .get(ri..=last)
                        .is_none_or(|rows| rows.iter().any(|r| !r.props.cant_split || r.props.header || r.props.height_rule == HeightRule::Exact))
                {
                    continue;
                }
                if laid.rows.get(ri..=last).is_none_or(|rows| rows.iter().any(|row| row.cells.iter().any(|cell| !plain_part(row, cell)))) {
                    continue;
                }
                let Some(owned) = row.items.get(cell.items.clone()) else { continue };
                if !cell.anchored_rules.is_empty()
                    || cell.valign == VAlign::Bottom
                    || owned.iter().any(|it| match it {
                        Placed::Lines { para, turn: TextDirection::Horizontal, .. } => {
                            para.rp.borders.is_some() || para.rp.shading.is_some() || !para.notes.is_empty()
                        }
                        Placed::Cell { clip: false, .. } | Placed::Rule { .. } | Placed::Fill { .. } => false,
                        _ => true,
                    })
                {
                    continue;
                }
                let Some(frame) = owned.iter().find(|it| matches!(it, Placed::Cell { .. })).cloned() else { continue };
                if owned.iter().filter(|it| matches!(it, Placed::Cell { .. })).count() != 1 {
                    continue;
                }
                let Placed::Cell { rect, cell: source_cell, .. } = &frame else { continue };
                let mut fills = owned.iter().filter(|it| matches!(it, Placed::Fill { .. }));
                let fill = fills.next().cloned();
                if fills.next().is_some() || fill.as_ref().is_some_and(|it| matches!(it, Placed::Fill { rect: area, .. } if area != rect)) {
                    continue;
                }
                let span = t.rows.get(ri).and_then(|r| r.cells.get(*source_cell)).map(|c| c.span());
                if ![rect.x, rect.w, rect.h].iter().all(|v| v.is_finite()) || rect.w <= 0.0 {
                    continue;
                }
                if laid.rows.get(ri..=last).is_none_or(|rows| {
                    rows.iter().enumerate().any(|(offset, row)| {
                        let Some(part) = row.cells.iter().find(|c| c.merge.as_ref().is_some_and(|e| e.key == edge.key)) else { return true };
                        row.items.get(part.items.clone()).is_none_or(|items| {
                            items.iter().any(|it| match it {
                                Placed::Cell { rect: r, cell, .. } => {
                                    !r.x.is_finite()
                                        || !r.w.is_finite()
                                        || (r.x - rect.x).abs() > FLOW_EPS
                                        || (r.w - rect.w).abs() > FLOW_EPS
                                        || t.rows.get(ri + offset).and_then(|r| r.cells.get(*cell)).map(|c| c.span()) != span
                                }
                                _ => false,
                            })
                        })
                    })
                }) {
                    continue;
                }
                let offset = cell.center.as_ref().map_or(0.0, |c| c.offset);
                let Some(end) = cell.end.map(|e| e - offset) else { continue };
                let height = end + row.bottom;
                if height <= row.height + FLOW_EPS && cell.center.is_none() {
                    continue;
                }
                if !height.is_finite()
                    || height <= 0.0
                    || height > capacity
                    || laid.rows.get(ri..=last).is_none_or(|rows| rows.iter().any(|r| r.height > capacity))
                {
                    continue;
                }
                let mut items: Vec<_> = owned.iter().filter(|it| matches!(it, Placed::Lines { .. })).cloned().collect();
                for it in &mut items {
                    it.translate(0.0, -offset);
                }
                let mut area = frame.clone();
                if let Placed::Cell { rect, .. } = &mut area {
                    rect.h = height;
                }
                items.push(area);
                let mut part = cell.clone();
                part.items = 0..items.len();
                part.bottom_rule = None;
                part.center = None;
                part.merge = None;
                part.valign = VAlign::Top;
                part.end = Some(end);
                let pending = RowLayout { height, min_height: 0.0, items, foreground: Vec::new(), bottom: row.bottom, cells: vec![part] };
                cells.push(CellFlow {
                    first: ri,
                    last,
                    column: edge.key.1,
                    pending: Some(pending),
                    frame,
                    fill,
                    align: cell.valign,
                    origin: None,
                    height: 0.0,
                    bottom: row.bottom,
                    done: false,
                });
            }
        }
        let by_key: std::collections::HashMap<_, _> = cells.iter().enumerate().map(|(i, c)| ((c.first, c.column), i)).collect();
        let by_row = laid
            .rows
            .iter()
            .map(|row| row.cells.iter().filter_map(|c| c.merge.as_ref().and_then(|e| by_key.get(&e.key).copied())).collect())
            .collect();
        Self { cells, by_row, by_key, active: Vec::new() }
    }

    pub(crate) fn row<'a>(&self, row: &'a RowLayout, ri: usize) -> Option<std::borrow::Cow<'a, RowLayout>> {
        if self.by_row.get(ri).is_none_or(Vec::is_empty) {
            return Some(std::borrow::Cow::Borrowed(row));
        }
        let height = self
            .by_row
            .get(ri)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.cells.get(i))
            .filter(|c| c.last == ri)
            .filter_map(|c| c.pending.as_ref().map(|p| p.height + (row.bottom - p.bottom).max(0.0) - c.height))
            .fold(row.height, f32::max);
        let mut out = row.clone();
        if height > row.height + FLOW_EPS {
            // Excluded merges retain restart geometry that cannot grow from this row.
            if row.cells.iter().any(|c| c.merge.as_ref().is_some_and(|e| !self.by_key.contains_key(&e.key))) {
                return None;
            }
            grow_row(&mut out, height);
        }
        let mut remove = vec![false; out.items.len()];
        for part in &out.cells {
            if !part.merge.as_ref().is_some_and(|e| self.by_key.contains_key(&e.key)) {
                continue;
            }
            for i in part.items.clone() {
                if let (Some(slot), Some(it)) = (remove.get_mut(i), out.items.get(i)) {
                    *slot = matches!(it, Placed::Lines { .. } | Placed::Cell { .. } | Placed::Fill { .. });
                }
            }
        }
        let mut map = Vec::with_capacity(out.items.len());
        let mut items = Vec::with_capacity(out.items.len());
        for (i, it) in out.items.into_iter().enumerate() {
            let start = items.len();
            if !remove.get(i).copied().unwrap_or(false) {
                items.push(it);
            }
            map.push(start..items.len());
        }
        out.foreground = remap_foreground(&out.foreground, &map);
        // These rows are unsplittable; the detached cells no longer own row-local text.
        for part in &mut out.cells {
            let start = map.get(part.items.start).map_or(items.len(), |r| r.start);
            let end = part.items.end.checked_sub(1).and_then(|i| map.get(i)).map_or(start, |r| r.end);
            let detached = part.merge.as_ref().is_some_and(|e| self.by_key.contains_key(&e.key));
            part.items = start..end;
            part.bottom_rule = part.bottom_rule.and_then(|i| map.get(i)).filter(|r| !r.is_empty()).map(|r| r.start);
            part.anchored_rules = part
                .anchored_rules
                .iter()
                .filter_map(|rule| {
                    let range = map.get(rule.item)?;
                    (!range.is_empty()).then_some(AnchoredRule { item: range.start, top: rule.top })
                })
                .collect();
            if detached {
                part.end = None;
                part.center = None;
            } else if let Some(center) = &mut part.center {
                let start = map.get(center.content.start).map_or(items.len(), |r| r.start);
                let end = center.content.end.checked_sub(1).and_then(|i| map.get(i)).map_or(start, |r| r.end);
                center.content = start..end;
            }
        }
        out.items = items;
        Some(std::borrow::Cow::Owned(out))
    }

    pub(crate) fn placed(&mut self, ri: usize, pb: &PageBuilder, x: f32, y: f32, height: f32, bottom: f32) -> bool {
        let Some(indices) = self.by_row.get(ri) else { return true };
        if !indices.is_empty()
            && (![x, y, height, bottom].iter().all(|v| v.is_finite()) || y < pb.top - FLOW_EPS || y + height > pb.bottom + FLOW_EPS)
        {
            return false;
        }
        for &i in indices {
            let Some(c) = self.cells.get_mut(i) else { return false };
            if !self.active.contains(&i) {
                self.active.push(i);
            }
            c.origin.get_or_insert((x, y));
            c.height += height;
            c.bottom = bottom;
        }
        true
    }

    pub(crate) fn end_row(&mut self, pb: &mut PageBuilder, ri: usize, anchor: Option<usize>) -> bool {
        for &i in self.by_row.get(ri).into_iter().flatten() {
            let Some(c) = self.cells.get_mut(i) else { return false };
            if c.last == ri {
                if !c.emit(pb, MergeEnd::Last, anchor) {
                    return false;
                }
                c.done = true;
            }
        }
        self.active.retain(|&i| self.cells.get(i).is_some_and(|c| !c.done));
        true
    }

    pub(crate) fn end_page(&mut self, pb: &mut PageBuilder, anchor: Option<usize>) -> bool {
        for &i in &self.active {
            if !self.cells.get_mut(i).is_some_and(|c| c.emit(pb, MergeEnd::Page, anchor)) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Copy, PartialEq)]
enum MergeEnd {
    Page,
    Last,
}

impl CellFlow {
    fn emit(&mut self, pb: &mut PageBuilder, end: MergeEnd, anchor: Option<usize>) -> bool {
        let Some((x, y)) = self.origin.take() else { return true };
        let height = std::mem::take(&mut self.height);
        let mut ink = Vec::new();
        if let Some(mut pending) = self.pending.take() {
            pending.height += (self.bottom - pending.bottom).max(0.0);
            pending.bottom = self.bottom;
            if end == MergeEnd::Last && pending.height > height + FLOW_EPS {
                return false;
            }
            if pending.height <= height + FLOW_EPS {
                let shift = if self.align == VAlign::Center { ((height - pending.height) / 2.0).max(0.0) } else { 0.0 };
                for it in &mut pending.items {
                    it.translate(0.0, shift);
                }
                ink = pending.items;
            } else if let Some((first, rest)) = split_row(&pending, height) {
                ink = first.items;
                self.pending = Some(rest);
            } else {
                self.pending = Some(pending);
            }
        }
        if ink.iter().any(|it| match it {
            Placed::Lines { para, l0, l1, y, .. } => {
                !y.is_finite()
                    || *y < -FLOW_EPS
                    || crate::item_bottom(*y, para, *l0, *l1).is_none_or(|b| !b.is_finite() || b > height - self.bottom + FLOW_EPS)
            }
            _ => false,
        }) {
            return false;
        }
        let behind = pb.behind.last().copied().unwrap_or(0);
        let Some(pg) = pb.pages.last_mut() else { return false };
        if let Some(mut fill) = self.fill.clone() {
            let start = pg.items.len().saturating_sub(behind);
            let Some(anchor) = anchor.filter(|anchor| *anchor <= start) else { return false };
            let Some(ranges) = pb.foreground.last_mut() else { return false };
            if let Placed::Fill { rect, .. } = &mut fill {
                rect.h = height;
            }
            fill.translate(x, y);
            pg.items.push(fill);
            crate::extend_table_paint(ranges, &[Foreground { range: 0..1, layer: PaintLayer::CellFill }], start, anchor);
        }
        for mut it in ink.into_iter().filter(|it| matches!(it, Placed::Lines { .. })) {
            it.translate(x, y);
            pg.items.push(it);
        }
        let mut frame = self.frame.clone();
        if let Placed::Cell { rect, .. } = &mut frame {
            rect.h = height;
        }
        frame.translate(x, y);
        pg.items.push(frame);
        true
    }
}

fn grow_row(row: &mut RowLayout, height: f32) {
    let old = row.height;
    for cell in &mut row.cells {
        if cell.valign == VAlign::Bottom && cell.merge.is_none() {
            let shift = height - old;
            for it in row.items.get_mut(cell.items.clone()).into_iter().flatten().filter(|it| matches!(it, Placed::Lines { .. })) {
                it.translate(0.0, shift);
            }
            cell.end = cell.end.map(|e| e + shift);
        } else if let Some(center) = &mut cell.center
            && center.kind == CenterKind::Fragment
        {
            let offset = ((height - center.height) / 2.0).max(0.0);
            let shift = offset - center.offset;
            for it in row.items.get_mut(center.content.clone()).into_iter().flatten() {
                it.translate(0.0, shift);
            }
            cell.end = cell.end.map(|e| e + shift);
            center.offset = offset;
        } else if cell.center.is_none() && cell.valign == VAlign::Center && cell.merge.is_none() {
            // Zero-offset plain centered content originally determined the row height.
            let shift = (height - old) / 2.0;
            for it in row.items.get_mut(cell.items.clone()).into_iter().flatten().filter(|it| matches!(it, Placed::Lines { .. })) {
                it.translate(0.0, shift);
            }
            cell.end = cell.end.map(|e| e + shift);
        }
        for (i, it) in row.items.iter_mut().enumerate().skip(cell.items.start).take(cell.items.len()) {
            match it {
                Placed::Cell { rect, .. } | Placed::Fill { rect, .. } if (rect.bottom() - old).abs() < FLOW_EPS => rect.h += height - old,
                Placed::Rule { y0, y1, .. } if cell.bottom_rule == Some(i) => {
                    *y0 += height - old;
                    *y1 += height - old;
                }
                Placed::Rule { y0, y1, .. } if (*y0 - *y1).abs() > FLOW_EPS => {
                    if (*y0 - old).abs() < FLOW_EPS {
                        *y0 = height;
                    }
                    if (*y1 - old).abs() < FLOW_EPS {
                        *y1 = height;
                    }
                }
                _ => {}
            }
        }
    }
    row.height = height;
}

fn plain_part(row: &RowLayout, cell: &CellPart) -> bool {
    let Some(items) = row.items.get(cell.items.clone()) else { return false };
    cell.anchored_rules.is_empty()
        && (cell.valign != VAlign::Bottom || cell.merge.is_none())
        && items.iter().filter(|it| matches!(it, Placed::Cell { .. })).count() == 1
        && items.iter().all(|it| match it {
            Placed::Lines { para, turn: TextDirection::Horizontal, .. } => {
                para.rp.borders.is_none()
                    && para.rp.shading.is_none()
                    && para.rp.frame.is_none()
                    && !para.rp.keep_lines
                    && !para.rp.keep_next
                    && !para.rp.page_break_before
                    && !para.has_page_fields
                    && para.notes.is_empty()
                    && para.drop_cap.is_none()
            }
            Placed::Cell { clip: false, .. } | Placed::Rule { .. } | Placed::Fill { .. } => true,
            _ => false,
        })
}
