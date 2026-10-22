//! Recover complete point preferences whose stored twips were rounded independently.
use super::*;

const TWIPS_PER_POINT: f32 = 20.0;
const TWIP_NOISE: f32 = 0.01;
const HALF_TWIP: f32 = 0.5;

pub(super) fn point_grid(t: &Table, target: f32) -> Option<Vec<f32>> {
    let target = twips(target)?;
    let count = t.grid.len();
    if !(2..=wordcraft_doc::table::MAX_COLS).contains(&count)
        || t.cols() != count
        || t.rows.is_empty()
        || t.rows.len() > wordcraft_doc::table::MAX_ROWS
    {
        return None;
    }

    let mut witnesses = Vec::new();
    let mut previous_merges = Vec::new();
    let mut rounded_total = false;
    for row in &t.rows {
        if row.first_grid() != 0 || row.props.grid_after != 0 {
            return None;
        }
        let mut column = 0usize;
        let mut total = 0.0;
        let mut edges = vec![(0, 0.0)];
        let mut complete = true;
        let mut current_merges = Vec::new();
        for cell in &row.cells {
            let end = column.checked_add(cell.span())?;
            if end > count || cell.props.width_pct.is_some() {
                return None;
            }
            let span = (column, end);
            if cell.props.vmerge == VMerge::Continue && (!previous_merges.contains(&span) || !empty_continuation(cell)) {
                return None;
            }
            if cell.props.vmerge != VMerge::None {
                current_merges.push(span);
            }
            match cell.props.width {
                Some(width) if width > 0.0 => {
                    total += twips(width)?;
                    edges.push((end, total));
                }
                None | Some(0.0) if empty_continuation(cell) => complete = false,
                _ => return None,
            }
            column = end;
        }
        if column != count {
            return None;
        }
        previous_merges = current_merges;
        if !complete {
            continue;
        }
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        rounded_total |= (total - target).abs() > TWIP_NOISE;
        for (_, edge) in &mut edges {
            *edge *= target / total;
            if !edge.is_finite() {
                return None;
            }
        }
        witnesses.push(edges);
    }
    if !rounded_total {
        return None;
    }

    // Dense rows pin more boundaries. Sorting makes duplicate/reordered rows immaterial.
    witnesses.sort_by(|a, b| {
        b.len().cmp(&a.len()).then_with(|| {
            a.iter()
                .zip(b)
                .map(|((ai, ax), (bi, bx))| ai.cmp(bi).then_with(|| ax.total_cmp(bx)))
                .find(|order| !order.is_eq())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let mut edges = vec![None; count + 1];
    edges[0] = Some(0.0);
    edges[count] = Some(target);
    for row in witnesses {
        for (column, x) in row {
            let edge = edges.get_mut(column)?;
            if edge.is_none() {
                *edge = Some(x);
            }
        }
    }

    // A candidate must satisfy every declared span, not only its seed row.
    for row in &t.rows {
        let mut column = 0usize;
        for cell in &row.cells {
            let end = column.checked_add(cell.span())?;
            if let Some(width) = cell.props.width.filter(|w| *w > 0.0) {
                let expected = twips(width)?;
                let actual = (*edges.get(end)?)? - (*edges.get(column)?)?;
                if (actual - expected).abs() > HALF_TWIP + TWIP_NOISE {
                    return None;
                }
            }
            column = end;
        }
    }
    edges
        .windows(2)
        .map(|pair| {
            let width = (pair[1]? - pair[0]?) / TWIPS_PER_POINT;
            (width.is_finite() && width > 0.0).then_some(width)
        })
        .collect()
}

fn twips(points: f32) -> Option<f32> {
    let value = points * TWIPS_PER_POINT;
    if !value.is_finite() || value <= 0.0 || (value - value.round()).abs() > TWIP_NOISE {
        return None;
    }
    Some(value.round())
}

fn empty_continuation(cell: &wordcraft_doc::Cell) -> bool {
    cell.props.vmerge == VMerge::Continue && cell.blocks.iter().all(|block| matches!(&**block, Block::Para(p) if p.text.is_empty()))
}
