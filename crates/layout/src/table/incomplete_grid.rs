use wordcraft_doc::Table;

const WIDTH_EPSILON: f32 = 0.01;

// Recover missing columns only when point widths agree across every row and with the
// declared grid. Keeping its prefix avoids replacing narrow grid units with equal columns.
pub(super) fn recover(table: &Table) -> Option<Vec<f32>> {
    let columns = table.cols();
    if !table.props.fixed || table.grid.is_empty() || table.grid.len() >= columns {
        return None;
    }
    let mut edges = vec![None; columns + 1];
    edges[0] = Some(0.0);
    for row in &table.rows {
        let mut column = row.first_grid();
        let mut x = if column == 0 { 0.0 } else { point_width(row.props.w_before)? };
        set_edge(&mut edges, column, x)?;
        for cell in &row.cells {
            if cell.props.width_pct.is_some() {
                return None;
            }
            x += point_width(cell.props.width)?;
            column = column.checked_add(cell.span())?;
            set_edge(&mut edges, column, x)?;
        }
        if row.props.grid_after > 0 {
            x += point_width(row.props.w_after)?;
            column = column.checked_add(row.props.grid_after as usize)?;
            set_edge(&mut edges, column, x)?;
        }
        if column != columns {
            return None;
        }
    }
    let widths: Option<Vec<f32>> = edges.windows(2).map(|pair| point_width(Some(pair[1]? - pair[0]?))).collect();
    let widths = widths?;
    for (declared, recovered) in table.grid.iter().zip(&widths) {
        if !declared.is_finite() || *declared <= 0.0 || (*declared - *recovered).abs() > WIDTH_EPSILON {
            return None;
        }
    }
    Some(widths)
}

fn point_width(width: Option<f32>) -> Option<f32> {
    width.filter(|width| width.is_finite() && *width > 0.0)
}

fn set_edge(edges: &mut [Option<f32>], column: usize, x: f32) -> Option<()> {
    let edge = edges.get_mut(column)?;
    if !x.is_finite() || edge.is_some_and(|previous| (previous - x).abs() > WIDTH_EPSILON) {
        return None;
    }
    *edge = Some(x);
    Some(())
}
