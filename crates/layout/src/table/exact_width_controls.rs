//! Synthetic point-width pairs. No report content or report dimensions.
use super::*;
use std::sync::Arc;
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Document, Path, para_block};

const EPSILON: f32 = 0.01;
const PARENT_WIDTHS: [f32; 2] = [40.0, 80.0];
const PARENT_GRID: [f32; 2] = [70.0, 62.0];
const HEADER_WIDTHS: [f32; 2] = [30.0, 80.0];
const HEADER_GRID: [f32; 2] = [80.0, 60.0];
const HEADER_BLOCK: usize = 1;
const MARK_NAME: &str = "generated-width-mark";
const ROUNDED_TARGET: f32 = 96.05;

fn plain_table(widths: &[f32], rows: usize) -> Table {
    let mut table = Table::new(rows, widths.len(), widths.iter().sum());
    table.props.style = None;
    table.props.cell_margins = Some([0.0; 4]);
    table.props.borders = Some(Borders::default());
    for row in &mut table.rows {
        for (cell, width) in row.cells.iter_mut().zip(widths) {
            cell.props.width = Some(*width);
        }
    }
    table
}

fn header_cell(columns: usize, target: f32, header: Table) -> wordcraft_doc::Cell {
    let mut cell = wordcraft_doc::Cell::empty();
    cell.props.span = columns as u32;
    cell.props.width = Some(target);
    cell.blocks = vec![para_block(Paragraph::new()), Arc::new(Block::Table(header)), para_block(Paragraph::new())];
    cell
}

fn paired_table() -> Table {
    let mut header = plain_table(&HEADER_WIDTHS, 2);
    header.props.fixed = true;
    header.props.width = None;
    header.grid = HEADER_GRID.to_vec();
    let mut table = plain_table(&PARENT_WIDTHS, 3);
    let target = PARENT_WIDTHS.iter().sum();
    table.props.width = Some(target);
    table.props.cell_margins = Some([0.0, 2.0, 0.0, 2.0]);
    table.grid = PARENT_GRID.to_vec();
    table.rows[0].cells = vec![header_cell(PARENT_WIDTHS.len(), target, header)];
    table
}

fn span_row(parts: &[(u32, f32)]) -> wordcraft_doc::Row {
    wordcraft_doc::Row {
        cells: parts
            .iter()
            .map(|&(span, width)| {
                let mut cell = wordcraft_doc::Cell::empty();
                cell.props.span = span;
                cell.props.width = Some(width);
                cell
            })
            .collect(),
        ..Default::default()
    }
}

fn spanning_table() -> Table {
    let widths = [20.0, 30.0, 40.0, 100.0, 10.0, 10.0];
    let mut table = plain_table(&widths, 1);
    let mut header = plain_table(&[60.0, 130.0], 2);
    header.props.fixed = true;
    header.props.width = None;
    header.grid = vec![130.0, 100.0];
    table.props.width = Some(widths.iter().sum());
    table.props.cell_margins = Some([0.0, 2.0, 0.0, 2.0]);
    table.grid = vec![30.0, 40.0, 25.0, 100.0, 10.0, 15.0];
    table.rows[0].cells = vec![header_cell(widths.len(), widths.iter().sum(), header)];
    table.rows.push(span_row(&[(3, 90.0), (1, 100.0), (1, 10.0), (1, 10.0)]));
    table.rows.push(span_row(&[(1, 20.0), (1, 30.0), (4, 160.0)]));
    table.rows.push(span_row(&[(2, 50.0), (1, 40.0), (1, 100.0), (1, 10.0), (1, 10.0)]));
    table
}

fn nested_mut(table: &mut Table) -> &mut Table {
    let block = &mut table.rows[0].cells[0].blocks[HEADER_BLOCK];
    let Block::Table(header) = Arc::make_mut(block) else { panic!("synthetic header missing") };
    header
}

fn document(table: Table) -> Document {
    let mut doc = Document::new();
    doc.last_section.page_w = 600.0;
    doc.last_section.page_h = 800.0;
    doc.last_section.margin_left = 20.0;
    doc.last_section.margin_right = 20.0;
    doc.last_section.margin_top = 20.0;
    doc.last_section.margin_bottom = 20.0;
    doc.body = vec![Arc::new(Block::Table(table))];
    doc
}

fn laid(table: Table) -> crate::DocLayout {
    crate::layout(&document(table), &mut crate::LayoutCache::new(), &crate::LayoutOptions::default())
}

fn cell_widths(layout: &crate::DocLayout, own: &[u32], source_row: usize) -> Vec<f32> {
    layout
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Placed::Cell { table, row, rect, covered: false, .. } if table.0 == own && *row == source_row => Some(rect.w),
            _ => None,
        })
        .collect()
}

fn assert_widths(got: &[f32], expected: &[f32]) {
    assert_eq!(got.len(), expected.len(), "{got:?}");
    for (a, b) in got.iter().zip(expected) {
        assert!((a - b).abs() < EPSILON, "{got:?} != {expected:?}");
    }
}

#[test]
fn exact_pair_recovers_columns() {
    let layout = laid(paired_table());
    assert_widths(&cell_widths(&layout, &[0], 1), &PARENT_WIDTHS);
    assert_widths(&cell_widths(&layout, &[0], 0), &[PARENT_WIDTHS.iter().sum()]);
}

#[test]
fn exact_pair_recovers_spans() {
    let layout = laid(spanning_table());
    assert_widths(&cell_widths(&layout, &[0], 1), &[90.0, 100.0, 10.0, 10.0]);
    assert_widths(&cell_widths(&layout, &[0], 2), &[20.0, 30.0, 160.0]);
    assert_widths(&cell_widths(&layout, &[0], 3), &[50.0, 40.0, 100.0, 10.0, 10.0]);
}

#[test]
fn exact_pair_order_is_stable() {
    let mut table = spanning_table();
    table.rows.swap(1, 3);
    table.rows.push(table.rows[2].clone());
    let layout = laid(table);
    assert_widths(&cell_widths(&layout, &[0], 1), &[50.0, 40.0, 100.0, 10.0, 10.0]);
    assert_widths(&cell_widths(&layout, &[0], 2), &[20.0, 30.0, 160.0]);
    assert_widths(&cell_widths(&layout, &[0], 3), &[90.0, 100.0, 10.0, 10.0]);
    assert_widths(&cell_widths(&layout, &[0], 4), &[20.0, 30.0, 160.0]);
}

#[test]
fn exact_pair_uses_nested_grid() {
    let layout = laid(paired_table());
    let own = [0, 0, 0, HEADER_BLOCK as u32];
    assert_widths(&cell_widths(&layout, &own, 0), &HEADER_WIDTHS);
    assert_widths(&cell_widths(&layout, &own, 1), &HEADER_WIDTHS);
    // Correct nested layout alone is insufficient: the parent measurement must also shrink.
    assert_widths(&cell_widths(&layout, &[0], 0), &[PARENT_WIDTHS.iter().sum()]);
}

#[test]
fn explicit_header_keeps_grid() {
    let mut table = paired_table();
    nested_mut(&mut table).props.width = Some(HEADER_GRID.iter().sum());
    let layout = laid(table);
    assert_widths(&cell_widths(&layout, &[0, 0, 0, HEADER_BLOCK as u32], 0), &HEADER_GRID);
    assert_widths(&cell_widths(&layout, &[0], 0), &[HEADER_GRID.iter().sum::<f32>() + 4.0]);
}

#[derive(Clone, Copy, Debug)]
enum PairFault {
    FixedParent,
    PercentParent,
    MissingTarget,
    InvalidTarget,
    InvalidGrid,
    MatchingParentTotal,
    WrongTitle,
    BodyConflict,
    RoundedBody,
    MissingWidth,
    InvalidWidth,
    PercentCell,
    VerticalMerge,
    MergeContinuation,
    SingleColumn,
    LeadingGap,
    TrailingGap,
    MissingBoundary,
    NestedNotFixed,
    NestedConflict,
    NestedMissingWidth,
    NestedInvalidWidth,
    NestedSingleRow,
    NestedStillWide,
    NoStaleFloor,
    SecondNested,
    NestedElsewhere,
    NestedGrandchild,
    ParentRtl,
    NestedRtl,
}

type CellGeometry = (Vec<u32>, usize, usize, Rect, bool);

fn geometry(layout: &crate::DocLayout) -> Vec<CellGeometry> {
    layout
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .filter_map(|item| match item {
            Placed::Cell { table, row, cell, rect, covered, .. } => Some((table.0.clone(), *row, *cell, *rect, *covered)),
            _ => None,
        })
        .collect()
}

#[test]
fn exact_pair_rejects_faults() {
    for fault in [
        PairFault::FixedParent,
        PairFault::PercentParent,
        PairFault::MissingTarget,
        PairFault::InvalidTarget,
        PairFault::InvalidGrid,
        PairFault::MatchingParentTotal,
        PairFault::WrongTitle,
        PairFault::BodyConflict,
        PairFault::RoundedBody,
        PairFault::MissingWidth,
        PairFault::InvalidWidth,
        PairFault::PercentCell,
        PairFault::VerticalMerge,
        PairFault::MergeContinuation,
        PairFault::SingleColumn,
        PairFault::LeadingGap,
        PairFault::TrailingGap,
        PairFault::MissingBoundary,
        PairFault::NestedNotFixed,
        PairFault::NestedConflict,
        PairFault::NestedMissingWidth,
        PairFault::NestedInvalidWidth,
        PairFault::NestedSingleRow,
        PairFault::NestedStillWide,
        PairFault::NoStaleFloor,
        PairFault::SecondNested,
        PairFault::NestedElsewhere,
        PairFault::NestedGrandchild,
        PairFault::ParentRtl,
        PairFault::NestedRtl,
    ] {
        let mut table = paired_table();
        match fault {
            PairFault::FixedParent => table.props.fixed = true,
            PairFault::PercentParent => table.props.width_pct = Some(100.0),
            PairFault::MissingTarget => table.props.width = None,
            PairFault::InvalidTarget => table.props.width = Some(f32::NAN),
            PairFault::InvalidGrid => table.grid[0] = f32::NAN,
            PairFault::MatchingParentTotal => table.grid = PARENT_WIDTHS.to_vec(),
            PairFault::WrongTitle => table.rows[0].cells[0].props.width = Some(10.0),
            PairFault::BodyConflict => {
                table.rows[2].cells[0].props.width = Some(41.0);
                table.rows[2].cells[1].props.width = Some(79.0);
            }
            PairFault::RoundedBody => {
                table.rows[2].cells[0].props.width = Some(40.025);
                table.rows[2].cells[1].props.width = Some(79.975);
            }
            PairFault::MissingWidth => table.rows[2].cells[0].props.width = None,
            PairFault::InvalidWidth => table.rows[2].cells[0].props.width = Some(f32::NAN),
            PairFault::PercentCell => table.rows[2].cells[0].props.width_pct = Some(25.0),
            PairFault::VerticalMerge => table.rows[2].cells[0].props.vmerge = VMerge::Restart,
            PairFault::MergeContinuation => {
                table.rows[1].cells[0].props.vmerge = VMerge::Restart;
                table.rows[2].cells[0].props.vmerge = VMerge::Continue;
                table.rows[2].cells[0].props.width = None;
            }
            PairFault::SingleColumn => {
                table.grid = vec![PARENT_GRID.iter().sum()];
                table.rows[0].cells[0].props.span = 1;
                for row in &mut table.rows[1..] {
                    row.cells = span_row(&[(1, PARENT_WIDTHS.iter().sum())]).cells;
                }
            }
            PairFault::LeadingGap => table.rows[2].props.grid_before = 1,
            PairFault::TrailingGap => table.rows[2].props.grid_after = 1,
            PairFault::MissingBoundary => {
                for row in &mut table.rows[1..] {
                    row.cells = span_row(&[(2, PARENT_WIDTHS.iter().sum())]).cells;
                }
            }
            PairFault::NestedNotFixed => nested_mut(&mut table).props.fixed = false,
            PairFault::NestedConflict => {
                let header = nested_mut(&mut table);
                header.rows[1].cells[0].props.width = Some(31.0);
                header.rows[1].cells[1].props.width = Some(79.0);
            }
            PairFault::NestedMissingWidth => nested_mut(&mut table).rows[1].cells[0].props.width = None,
            PairFault::NestedInvalidWidth => nested_mut(&mut table).rows[1].cells[0].props.width = Some(f32::NAN),
            PairFault::NestedSingleRow => nested_mut(&mut table).rows.truncate(1),
            PairFault::NestedStillWide => {
                for row in &mut nested_mut(&mut table).rows {
                    row.cells[0].props.width = Some(50.0);
                }
            }
            PairFault::NoStaleFloor => nested_mut(&mut table).grid = vec![40.0, 70.0],
            PairFault::SecondNested => {
                let extra = nested_mut(&mut table).clone();
                table.rows[0].cells[0].blocks.push(Arc::new(Block::Table(extra)));
            }
            PairFault::NestedElsewhere => {
                let extra = nested_mut(&mut table).clone();
                table.rows[2].cells[0].blocks.push(Arc::new(Block::Table(extra)));
            }
            PairFault::NestedGrandchild => {
                let extra = plain_table(&[10.0, 10.0], 1);
                nested_mut(&mut table).rows[0].cells[0].blocks.push(Arc::new(Block::Table(extra)));
            }
            PairFault::ParentRtl => table.props.bidi_visual = true,
            PairFault::NestedRtl => nested_mut(&mut table).props.bidi_visual = true,
        }
        // An explicit raw-grid target disables preparation but preserves the old nested width.
        let mut reference = table.clone();
        let header = nested_mut(&mut reference);
        header.props.width = Some(header.grid.iter().sum());
        assert_eq!(geometry(&laid(table)), geometry(&laid(reference)), "{fault:?}");
    }
}

#[test]
fn ordinary_table_keeps_grid() {
    let mut table = paired_table();
    table.rows[0].cells[0].blocks.remove(HEADER_BLOCK);
    let target = PARENT_WIDTHS.iter().sum::<f32>();
    let scale = target / PARENT_GRID.iter().sum::<f32>();
    let layout = laid(table);
    assert_widths(&cell_widths(&layout, &[0], 1), &PARENT_GRID.map(|w| w * scale));
}

#[test]
fn accepted_title_policy_stays() {
    let mut table = paired_table();
    table.grid = vec![30.0, 90.0];
    table.rows[0].props.header = true;
    table.rows[0].cells[0].props.width = Some(10.0);
    let mut grid = table.grid.clone();
    apply_cell_widths(&table, &mut grid);
    assert_eq!(grid, PARENT_WIDTHS);
    let mut reference = table.clone();
    let header = nested_mut(&mut reference);
    header.props.width = Some(header.grid.iter().sum());
    assert_eq!(geometry(&laid(table)), geometry(&laid(reference)));
}

fn marker_para() -> Paragraph {
    let mut para = Paragraph::new();
    para.insert_object(0, InlineObject::BookmarkStart { name: MARK_NAME.into() }, &Default::default()).unwrap();
    para.insert_object(
        para.text.len(),
        InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() },
        &Default::default(),
    )
    .unwrap();
    para.insert_object(para.text.len(), InlineObject::BookmarkEnd { name: MARK_NAME.into() }, &Default::default()).unwrap();
    para
}

fn para_at(layout: &crate::DocLayout, own: &[u32]) -> Arc<ParaLayout> {
    layout
        .pages
        .iter()
        .flat_map(|page| &page.items)
        .find_map(|item| match item {
            Placed::Lines { path, story: StoryRef::Body, para, .. } if path.0 == own => Some(para.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn source_paths_and_cache_stay() {
    let mut table = paired_table();
    let marker = para_block(marker_para());
    table.rows[1].cells[0].blocks = vec![marker.clone()];
    nested_mut(&mut table).rows[0].cells[0].blocks = vec![marker.clone()];
    let doc = document(table);
    let before = doc.clone();
    let mut cache = crate::LayoutCache::new();
    let opts = crate::LayoutOptions::default();
    let first = crate::layout(&doc, &mut cache, &opts);
    let warm = crate::layout(&doc, &mut cache, &opts);
    assert_eq!(doc, before);
    assert!(Arc::ptr_eq(&doc.body[0], &before.body[0]));
    let parent_path = [0, 1, 0, 0];
    let nested_path = [0, 0, 0, HEADER_BLOCK as u32, 0, 0, 0];
    let parent = para_at(&first, &parent_path);
    let nested = para_at(&first, &nested_path);
    assert!(Arc::ptr_eq(&parent, &para_at(&warm, &parent_path)));
    assert!(Arc::ptr_eq(&nested, &para_at(&warm, &nested_path)));
    assert_eq!(parent.text_len, marker.as_para().unwrap().text.len());
    assert_eq!(nested.text_len, parent.text_len);

    // Reusing the source paragraph with new cell widths must get a distinct width cache key.
    let mut changed = doc.clone();
    let Block::Table(table) = Arc::make_mut(&mut changed.body[0]) else { panic!("synthetic table missing") };
    for row in &mut table.rows[1..] {
        row.cells[0].props.width = Some(50.0);
        row.cells[1].props.width = Some(70.0);
    }
    let moved = crate::layout(&changed, &mut cache, &opts);
    assert_widths(&cell_widths(&moved, &[0], 1), &[50.0, 70.0]);
    assert!(!Arc::ptr_eq(&parent, &para_at(&moved, &parent_path)));
    assert!(Arc::ptr_eq(&nested, &para_at(&moved, &nested_path)));
    assert_eq!(doc, before);
    assert_eq!(doc.table(StoryRef::Body, &Path::top(0)).unwrap().grid, PARENT_GRID);
}

fn rounded_pair() -> Table {
    let mut header = plain_table(&[25.0, 65.0], 2);
    header.props.fixed = true;
    header.props.width = None;
    header.grid = vec![50.0, 60.0];
    let mut table = plain_table(&[32.0; 3], 3);
    table.props.width = Some(ROUNDED_TARGET);
    table.props.cell_margins = Some([0.0, 2.0, 0.0, 2.0]);
    table.grid = vec![38.0, 40.0, 30.0];
    table.rows[0].cells = vec![header_cell(3, ROUNDED_TARGET, header)];
    table.rows[2] = span_row(&[(2, 64.05), (1, 32.0)]);
    table
}

#[test]
fn rounded_pair_keeps_twip_spans() {
    let table = rounded_pair();
    let expected = [ROUNDED_TARGET / 3.0; 3];
    assert_widths(&cell_widths(&laid(table.clone()), &[0], 1), &expected);
    let original = super::rounded_width::point_grid(&table, ROUNDED_TARGET).unwrap();
    let mut reordered = table.clone();
    reordered.rows.swap(1, 2);
    reordered.rows.push(reordered.rows[1].clone());
    assert_eq!(super::rounded_width::point_grid(&reordered, ROUNDED_TARGET).unwrap(), original);
}

#[test]
fn rounded_pair_rejects_faults() {
    let mut table = rounded_pair();
    table.rows[1].cells[0].props.width = Some(32.10);
    table.rows[1].cells[2].props.width = Some(31.90);
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
    let mut table = rounded_pair();
    table.rows[1].cells[0].props.width = Some(32.025);
    table.rows[1].cells[2].props.width = Some(31.975);
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
    let mut table = rounded_pair();
    table.rows.remove(1);
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
}

#[test]
fn rounded_pair_checks_merges() {
    let mut table = rounded_pair();
    table.rows[1].cells[0].props.vmerge = VMerge::Restart;
    table.rows[2] = plain_table(&[32.0; 3], 1).rows.remove(0);
    table.rows[2].cells[0].props.vmerge = VMerge::Continue;
    table.rows[2].cells[0].props.width = None;
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_some());
    table.rows[1].cells[0].props.vmerge = VMerge::None;
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
    table.rows[1].cells[0].props.vmerge = VMerge::Restart;
    table.rows[2].cells[0].blocks = vec![para_block(Paragraph::with_text("synthetic", Default::default()))];
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
    table.rows[2].cells[0].props.width = Some(32.0);
    assert!(super::rounded_width::point_grid(&table, ROUNDED_TARGET).is_none());
}

#[test]
fn rounded_pair_fills_tied_edges() {
    let target = 128.05;
    let mut table = plain_table(&[32.0; 4], 3);
    table.rows[0] = span_row(&[(4, target)]);
    table.rows[1] = span_row(&[(1, 32.0), (1, 32.0), (2, 64.0)]);
    table.rows[2] = span_row(&[(2, 64.0), (1, 32.0), (1, 32.0)]);
    let original = super::rounded_width::point_grid(&table, target).unwrap();
    assert_widths(&original, &[target / 4.0; 4]);
    table.rows.swap(1, 2);
    table.rows.push(table.rows[1].clone());
    assert_eq!(super::rounded_width::point_grid(&table, target).unwrap(), original);
}

#[test]
fn exact_pair_rejects_span_drift() {
    let mut table = paired_table();
    table.rows[2].cells[0].props.width = Some(40.007_813);
    table.rows[2].cells[1].props.width = Some(79.984375);
    let mut reference = table.clone();
    let header = nested_mut(&mut reference);
    header.props.width = Some(header.grid.iter().sum());
    assert_eq!(geometry(&laid(table)), geometry(&laid(reference)));
}

#[test]
fn exact_pair_nested_min_agrees() {
    let inner = paired_table();
    let target = PARENT_WIDTHS.iter().sum();
    let mut outer = plain_table(&[target], 1);
    outer.props.width = Some(target);
    outer.rows[0].cells[0].blocks = vec![Arc::new(Block::Table(inner))];
    let layout = laid(outer);
    assert_widths(&cell_widths(&layout, &[0], 0), &[target]);
    assert_widths(&cell_widths(&layout, &[0, 0, 0, 0], 1), &PARENT_WIDTHS);
}
