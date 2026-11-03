use super::*;
use wordcraft_doc::{Document, para_block};

const TARGET_WIDTH: f32 = 300.0;
const RAW_WIDTHS: [f32; 3] = [60.0, 80.0, 60.0];
const STALE_GRID: [f32; 3] = [30.0, 180.0, 90.0];
const TITLE_WIDTH: f32 = 20.0;
const PARTIAL_WIDTH: f32 = RAW_WIDTHS[0] + RAW_WIDTHS[1];
const EPSILON: f32 = 0.01;

fn title_width_table() -> Table {
    let mut table = Table::new(4, RAW_WIDTHS.len(), TARGET_WIDTH);
    table.props.style = None;
    table.props.width = Some(TARGET_WIDTH);
    table.props.cell_margins = Some([0.0; 4]);
    table.props.borders = Some(Borders::default());
    table.grid = STALE_GRID.to_vec();
    for row in &mut table.rows {
        for (cell, width) in row.cells.iter_mut().zip(RAW_WIDTHS) {
            cell.props.width = Some(width);
            cell.blocks = vec![para_block(Paragraph::with_text("A", Default::default()))];
        }
    }
    table.rows[0].props.header = true;
    table.rows[0].cells.truncate(1);
    table.rows[0].cells[0].props.span = RAW_WIDTHS.len() as u32;
    table.rows[0].cells[0].props.width = Some(TITLE_WIDTH);
    table.rows[1].props.header = true;
    table.rows[1].cells.truncate(2);
    table.rows[1].cells[0].props.span = 2;
    table.rows[1].cells[0].props.width = Some(PARTIAL_WIDTH);
    table.rows[1].cells[1].props.width = Some(RAW_WIDTHS[2]);
    table
}

#[test]
fn title_rows_keep_raw_widths() {
    let table = title_width_table();
    let mut grid = table.grid.clone();
    apply_cell_widths(&table, &mut grid);
    assert_eq!(grid, RAW_WIDTHS);
}

#[test]
fn title_rows_scale_once() {
    let table = title_width_table();
    let mut doc = Document::new();
    doc.last_section.page_w = 400.0;
    doc.last_section.margin_left = 50.0;
    doc.last_section.margin_right = 50.0;
    doc.body = vec![std::sync::Arc::new(Block::Table(table))];
    let laid = crate::layout(&doc, &mut crate::LayoutCache::new(), &crate::LayoutOptions::default());
    let cells: Vec<_> = laid
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Placed::Cell { table, row: 2, rect, .. } if table.0 == [0] => Some(rect.w),
            _ => None,
        })
        .collect();
    assert_eq!(cells.len(), RAW_WIDTHS.len());
    let scale = TARGET_WIDTH / RAW_WIDTHS.iter().sum::<f32>();
    for (got, raw) in cells.iter().zip(RAW_WIDTHS) {
        assert!((got - raw * scale).abs() < EPSILON, "{cells:?}");
    }
}

#[derive(Clone, Copy, Debug)]
enum WidthFault {
    Fixed,
    MissingTarget,
    OtherTarget,
    PercentTable,
    NonheaderTitle,
    OversizedTitle,
    InvalidTitle,
    PartialConflict,
    RowConflict,
    BoundaryDrift,
    MissingCell,
    PercentCell,
    InvalidCell,
    LeadingGap,
    TrailingGap,
    MissingWitness,
    TitleAfterBody,
}

#[test]
fn title_rows_reject_conflicts() {
    for fault in [
        WidthFault::Fixed,
        WidthFault::MissingTarget,
        WidthFault::OtherTarget,
        WidthFault::PercentTable,
        WidthFault::NonheaderTitle,
        WidthFault::OversizedTitle,
        WidthFault::InvalidTitle,
        WidthFault::PartialConflict,
        WidthFault::RowConflict,
        WidthFault::BoundaryDrift,
        WidthFault::MissingCell,
        WidthFault::PercentCell,
        WidthFault::InvalidCell,
        WidthFault::LeadingGap,
        WidthFault::TrailingGap,
        WidthFault::MissingWitness,
        WidthFault::TitleAfterBody,
    ] {
        let mut table = title_width_table();
        match fault {
            WidthFault::Fixed => table.props.fixed = true,
            WidthFault::MissingTarget => table.props.width = None,
            WidthFault::OtherTarget => table.props.width = Some(TARGET_WIDTH + 1.0),
            WidthFault::PercentTable => table.props.width_pct = Some(100.0),
            WidthFault::NonheaderTitle => table.rows[0].props.header = false,
            WidthFault::OversizedTitle => table.rows[0].cells[0].props.width = Some(RAW_WIDTHS.iter().sum::<f32>() + 1.0),
            WidthFault::InvalidTitle => table.rows[0].cells[0].props.width = Some(f32::NAN),
            WidthFault::PartialConflict => table.rows[1].cells[0].props.width = Some(PARTIAL_WIDTH - 1.0),
            WidthFault::RowConflict => {
                table.rows[3].cells[0].props.width = Some(RAW_WIDTHS[0] + 1.0);
                table.rows[3].cells[1].props.width = Some(RAW_WIDTHS[1] - 1.0);
            }
            WidthFault::BoundaryDrift => {
                for (cell, width) in table.rows[3].cells.iter_mut().zip(RAW_WIDTHS) {
                    cell.props.width = Some(width + EPSILON * 0.6);
                }
            }
            WidthFault::MissingCell => table.rows[3].cells[1].props.width = None,
            WidthFault::PercentCell => table.rows[3].cells[1].props.width_pct = Some(40.0),
            WidthFault::InvalidCell => table.rows[3].cells[1].props.width = Some(f32::INFINITY),
            WidthFault::LeadingGap => table.rows[3].props.grid_before = 1,
            WidthFault::TrailingGap => table.rows[3].props.grid_after = 1,
            WidthFault::MissingWitness => table.rows.truncate(3),
            WidthFault::TitleAfterBody => {
                table.rows.swap(0, 3);
                table.rows[0].props.header = true;
            }
        }
        let mut grid = table.grid.clone();
        apply_cell_widths(&table, &mut grid);
        assert_eq!(grid, STALE_GRID, "{fault:?}");
    }
}
