//! Recover a point-width parent and its sole fixed nested header together.
use super::*;
use std::borrow::Cow;
use std::sync::Arc;

const WIDTH_EPSILON: f32 = 0.01;
const MIN_HEADER_ROWS: usize = 2;

pub(super) fn prepare<'a>(t: &'a Table, style: Option<&TableStyleProps>) -> Cow<'a, Table> {
    recover(t, style).map_or_else(|| Cow::Borrowed(t), Cow::Owned)
}

fn recover(t: &Table, style: Option<&TableStyleProps>) -> Option<Table> {
    if t.props.fixed || t.props.width_pct.is_some() || t.props.float.is_some() || t.props.bidi_visual {
        return None;
    }
    let target = t.props.width.filter(|w| w.is_finite() && *w > 0.0)?;
    let old_total = grid_total(t)?;
    // Leave the accepted undersized-title policy and already consistent grids alone.
    if (old_total - target).abs() <= WIDTH_EPSILON {
        return None;
    }
    let first = t.rows.first()?;
    let cell = first.cells.first().filter(|_| first.cells.len() == 1)?;
    if cell.span() != t.grid.len() || (cell.props.width? - target).abs() > WIDTH_EPSILON {
        return None;
    }
    let parent_grid = point_grid(t, target).or_else(|| super::rounded_width::point_grid(t, target))?;
    let (block_index, nested) = sole_header(t)?;
    if !nested.props.fixed
        || nested.props.width.is_some()
        || nested.props.width_pct.is_some()
        || nested.props.float.is_some()
        || nested.props.bidi_visual
        || nested.rows.len() < MIN_HEADER_ROWS
        || nested.rows.iter().any(|row| row.cells.len() != nested.grid.len() || row.cells.iter().any(|c| c.span() != 1))
        || nested.rows.iter().flat_map(|row| &row.cells).flat_map(|c| &c.blocks).any(|b| matches!(&**b, Block::Table(_)))
    {
        return None;
    }
    let nested_target: f32 = nested.rows.first()?.cells.iter().filter_map(|c| c.props.width).sum();
    let nested_grid = point_grid(nested, nested_target)?;
    let margins = cell.props.resolved_margins(row_margins(first, default_margins(t, style)));
    if [margins[1], margins[3]].iter().any(|w| !w.is_finite() || *w < 0.0) {
        return None;
    }
    let inset = margins[1] + margins[3];
    let old_need = grid_total(nested)? + inset;
    let new_need = nested_grid.iter().sum::<f32>() + inset;
    if !old_need.is_finite() || !new_need.is_finite() || old_need <= target + WIDTH_EPSILON || new_need > target + WIDTH_EPSILON {
        return None;
    }

    // Only grids change. Paragraph Arcs, cell/block indices and field ownership stay shared.
    let mut out = t.clone();
    out.grid = parent_grid;
    let block = out.rows.first_mut()?.cells.first_mut()?.blocks.get_mut(block_index)?;
    let Block::Table(header) = Arc::make_mut(block) else { return None };
    header.grid = nested_grid;
    Some(out)
}

fn grid_total(t: &Table) -> Option<f32> {
    if !(2..=wordcraft_doc::table::MAX_COLS).contains(&t.grid.len()) || t.cols() != t.grid.len() || t.grid.iter().any(|w| !w.is_finite() || *w <= 0.0)
    {
        return None;
    }
    let total: f32 = t.grid.iter().sum();
    (total.is_finite() && total > 0.0).then_some(total)
}

// Every declared span and cumulative boundary must agree. Missing widths never vote.
fn point_grid(t: &Table, target: f32) -> Option<Vec<f32>> {
    grid_total(t)?;
    if !target.is_finite() || target <= 0.0 || t.rows.is_empty() || t.rows.len() > wordcraft_doc::table::MAX_ROWS {
        return None;
    }
    let mut edges = vec![None; t.grid.len() + 1];
    edges[0] = Some(0.0);
    edges[t.grid.len()] = Some(target);
    for row in &t.rows {
        if row.first_grid() != 0 || row.props.grid_after != 0 {
            return None;
        }
        let (mut column, mut x) = (0usize, 0.0f32);
        for cell in &row.cells {
            if cell.props.width_pct.is_some() || cell.props.vmerge != VMerge::None {
                return None;
            }
            let width = cell.props.width.filter(|w| w.is_finite() && *w > 0.0)?;
            let start = column;
            column = column.checked_add(cell.span())?;
            x += width;
            let edge = edges.get_mut(column)?;
            if !x.is_finite() || edge.is_some_and(|previous: f32| (previous - x).abs() > WIDTH_EPSILON) {
                return None;
            }
            if edge.is_none() {
                *edge = Some(x);
            }
            // A previously known start also pins this cell's width, not just its end.
            if let Some(left) = edges.get(start).copied().flatten()
                && (x - left - width).abs() > WIDTH_EPSILON
            {
                return None;
            }
        }
        if column != t.grid.len() {
            return None;
        }
    }
    let widths: Option<Vec<f32>> = edges.windows(2).map(|pair| Some(pair[1]? - pair[0]?)).collect();
    let widths = widths.filter(|ws| ws.iter().all(|w| w.is_finite() && *w > 0.0))?;
    for row in &t.rows {
        let mut column = 0usize;
        for cell in &row.cells {
            let end = column.checked_add(cell.span())?;
            let actual: f32 = widths.get(column..end)?.iter().sum();
            if (actual - cell.props.width?).abs() > WIDTH_EPSILON {
                return None;
            }
            column = end;
        }
    }
    Some(widths)
}

fn sole_header(t: &Table) -> Option<(usize, &Table)> {
    let mut found = None;
    for (ri, row) in t.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            for (bi, block) in cell.blocks.iter().enumerate() {
                if let Block::Table(nested) = &**block {
                    if ri != 0 || ci != 0 || found.is_some() {
                        return None;
                    }
                    found = Some((bi, nested));
                }
            }
        }
    }
    found
}
