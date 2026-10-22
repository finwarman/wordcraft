use super::*;
use std::sync::Arc;
use wordcraft_doc::props::{BorderStyle, CharProps, LineSpacing, ParaProps};
use wordcraft_doc::{Document, para_block};

const RULE_WIDTH: f32 = 1.0;
const HALF_RULE: f32 = RULE_WIDTH / 2.0;
const LINE_HEIGHT: f32 = 12.0;
const EPS: f32 = 0.001;
const PROBE: &str = "MMMM";

fn paragraph(text: &str) -> Paragraph {
    let mut p = Paragraph::with_text(text, CharProps { size: Some(10.0), font: Some("Arial".into()), ..Default::default() });
    p.props = ParaProps {
        space_before: Some(0.0),
        space_after: Some(0.0),
        line_spacing: Some(LineSpacing::Exactly(LINE_HEIGHT)),
        widow_control: Some(false),
        ..Default::default()
    };
    p
}

fn lay(doc: &Document) -> crate::DocLayout {
    crate::layout(doc, &mut crate::LayoutCache::new(), &crate::LayoutOptions::default())
}

fn probe_width() -> f32 {
    let mut doc = Document::new();
    doc.body = vec![para_block(paragraph(PROBE))];
    let laid = lay(&doc);
    let para = laid.pages[0]
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Lines { para, .. } => Some(para),
            _ => None,
        })
        .unwrap();
    assert_eq!(para.lines.len(), 1);
    para.clusters.iter().map(|c| c.adv).sum()
}

fn control_table(text: &str) -> Table {
    // Derive the threshold from real shaping. Half a rule fits before clearance,
    // while the complete pair of inward half-rules forces a second line.
    let width = probe_width() + HALF_RULE;
    let mut table = Table::new(1, 1, width);
    table.props.style = None;
    table.props.fixed = true;
    table.props.width = Some(width);
    table.props.cell_margins = Some([0.0; 4]);
    table.props.borders = Some(Borders::all(Border::single(RULE_WIDTH)));
    table.rows[0].cells[0].blocks = vec![para_block(paragraph(text))];
    table
}

fn document(table: Table) -> Document {
    let mut doc = Document::new();
    doc.body = vec![Arc::new(Block::Table(table))];
    doc
}

fn first_cell(laid: &crate::DocLayout) -> Rect {
    laid.pages[0]
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Cell { rect, table, .. } if table.0 == [0] => Some(*rect),
            _ => None,
        })
        .unwrap()
}

fn first_lines(laid: &crate::DocLayout) -> (f32, usize) {
    laid.pages[0]
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Lines { x, para, .. } => Some((*x, para.lines.len())),
            _ => None,
        })
        .unwrap()
}

#[test]
fn zero_margins_clear_side_rules() {
    let table = control_table(PROBE);
    let source = table.clone();
    let doc = document(table);
    let laid = lay(&doc);
    let rect = first_cell(&laid);
    let (x, lines) = first_lines(&laid);
    assert!((rect.w - source.grid[0]).abs() < EPS, "side clearance must not change the grid");
    assert!((x - rect.x - HALF_RULE).abs() < EPS);
    assert_eq!(lines, 2, "the inward rule halves reduce usable text width");
    assert!((rect.h - (2.0 * LINE_HEIGHT + 2.0 * RULE_WIDTH)).abs() < EPS);
    assert_eq!(doc.body[0].as_table().unwrap().props.cell_margins, Some([0.0; 4]), "layout must not mutate source margins");
}

#[test]
fn positive_margins_stay_source() {
    for margin in [HALF_RULE / 2.0, RULE_WIDTH * 2.0] {
        let mut table = control_table(PROBE);
        let width = probe_width() + margin * 2.0;
        table.grid[0] = width;
        table.props.width = Some(width);
        table.props.cell_margins = Some([0.0, margin, 0.0, margin]);
        let laid = lay(&document(table));
        let (x, lines) = first_lines(&laid);
        assert!((x - first_cell(&laid).x - margin).abs() < EPS);
        assert_eq!(lines, 1, "positive margins must not gain another side-rule inset");
    }
}

#[test]
fn each_zero_side_clears_its_rule() {
    for margins in [[0.0, 0.0, 0.0, RULE_WIDTH], [0.0, RULE_WIDTH, 0.0, 0.0]] {
        let mut table = control_table(PROBE);
        table.props.cell_margins = Some(margins);
        let laid = lay(&document(table));
        let expected_left = if margins[1] == 0.0 { HALF_RULE } else { margins[1] };
        assert!((first_lines(&laid).0 - first_cell(&laid).x - expected_left).abs() < EPS);
        assert_eq!(first_lines(&laid).1, 2);
    }
}

#[test]
fn invisible_rules_keep_width() {
    for border in [None, Some(Border::single(0.0)), Some(Border { style: BorderStyle::None, ..Border::single(RULE_WIDTH) })] {
        let mut table = control_table(PROBE);
        table.props.borders = Some(Borders { left: border, right: border, ..Default::default() });
        let laid = lay(&document(table));
        let (x, lines) = first_lines(&laid);
        assert!((x - first_cell(&laid).x).abs() < EPS);
        assert_eq!(lines, 1, "absent, zero-width and hidden rules add no clearance");
    }
}

#[derive(Clone, Copy)]
enum PriorPath {
    AutoFit,
    Rtl,
}

#[test]
fn autofit_and_rtl_keep_prior_path() {
    for path in [PriorPath::AutoFit, PriorPath::Rtl] {
        let mut table = control_table(PROBE);
        match path {
            PriorPath::AutoFit => table.props.fixed = false,
            PriorPath::Rtl => table.props.bidi_visual = true,
        }
        let laid = lay(&document(table));
        let (x, lines) = first_lines(&laid);
        assert!((x - first_cell(&laid).x).abs() < EPS);
        assert_eq!(lines, 1);
    }
}

#[test]
fn turned_text_keeps_prior_frame() {
    for turn in [TextDirection::Up, TextDirection::Down] {
        let mut table = control_table(PROBE);
        table.rows[0].props.height = Some(LINE_HEIGHT * 4.0);
        table.rows[0].props.height_rule = HeightRule::AtLeast;
        table.rows[0].cells[0].props.text_direction = turn;
        let laid = lay(&document(table));
        let cell = first_cell(&laid);
        let x = laid.pages[0]
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Lines { x, turn: direction, .. } if *direction == turn => Some(*x),
                _ => None,
            })
            .unwrap();
        let expected = if turn == TextDirection::Up { cell.x } else { cell.right() };
        assert!((x - expected).abs() < EPS, "turned origins keep the original zero-margin frame");
    }
}

#[test]
fn exact_row_keeps_physical_clip() {
    let mut table = control_table(PROBE);
    table.rows[0].props.height = Some(LINE_HEIGHT);
    table.rows[0].props.height_rule = HeightRule::Exact;
    let doc = document(table);
    let laid = lay(&doc);
    assert_eq!(first_lines(&laid).1, 2);
    let draws = crate::display::page_display(&doc, &laid.pages[0], &Default::default());
    assert!(draws.iter().any(|draw| matches!(draw, crate::display::Draw::Clip { rect, .. } if (rect.h - LINE_HEIGHT).abs() < EPS)));
    assert!(!draws.iter().any(|draw| matches!(draw, crate::display::Draw::Glyphs { .. })), "exact-row glyphs stay inside the physical clip");
}

fn tabbed_table() -> Table {
    let mut doc = Document::new();
    doc.body = vec![para_block(paragraph(":"))];
    let layout = lay(&doc);
    let colon_width: f32 = layout.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Lines { para, .. } => Some(para.clusters.iter().map(|c| c.adv).sum()),
            _ => None,
        })
        .unwrap();
    let width = probe_width() + colon_width + HALF_RULE;
    let mut table = control_table(PROBE);
    table.grid[0] = width;
    table.props.width = Some(width);
    let mut p = paragraph(&format!("{PROBE}\t:"));
    p.props.tabs = Some(vec![wordcraft_doc::props::TabStop {
        pos: width - colon_width / 2.0,
        align: wordcraft_doc::props::TabAlign::Center,
        leader: Default::default(),
    }]);
    table.rows[0].cells[0].blocks = vec![para_block(p)];
    table
}

#[test]
fn tabbed_exact_keeps_frame() {
    let mut table = tabbed_table();
    table.rows[0].props.height = Some(LINE_HEIGHT + 2.0 * RULE_WIDTH);
    table.rows[0].props.height_rule = HeightRule::Exact;
    let doc = document(table);
    let laid = lay(&doc);
    assert_eq!(first_lines(&laid).1, 1, "side clearance must not wrap a tabbed label into a clipped line");
    assert!((first_lines(&laid).0 - first_cell(&laid).x).abs() < EPS);
    assert!(!laid.pages[0].items.iter().any(|item| matches!(item, Placed::Cell { clip: true, .. })));
    let draws = crate::display::page_display(&doc, &laid.pages[0], &Default::default());
    let glyphs: usize = draws
        .iter()
        .filter_map(|draw| match draw {
            crate::display::Draw::Glyphs { glyphs, .. } => Some(glyphs.len()),
            _ => None,
        })
        .sum();
    assert_eq!(glyphs, PROBE.len() + 1, "the label and its delimiter must remain painted");
}

#[test]
fn tabbed_auto_clears_rules() {
    let laid = lay(&document(tabbed_table()));
    assert_eq!(first_lines(&laid).1, 2);
    assert!((first_lines(&laid).0 - first_cell(&laid).x - HALF_RULE).abs() < EPS);
}

#[test]
fn continued_row_keeps_all_ink() {
    const WORDS: usize = 12;
    let text = std::iter::repeat_n(PROBE, WORDS).collect::<Vec<_>>().join(" ");
    let mut doc = document(control_table(&text));
    doc.last_section.page_h = LINE_HEIGHT * 8.0;
    doc.last_section.margin_top = LINE_HEIGHT;
    doc.last_section.margin_bottom = LINE_HEIGHT;
    let laid = lay(&doc);
    assert!(laid.pages.len() > 1);
    let mut next_line = 0;
    for page in &laid.pages {
        let cell = page
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Cell { rect, table, .. } if table.0 == [0] => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!(cell.bottom() <= doc.last_section.page_h - doc.last_section.margin_bottom + EPS);
        for item in &page.items {
            if let Placed::Lines { x, y, para, l0, l1, .. } = item {
                assert_eq!(*l0, next_line, "continued ranges neither repeat nor skip a line");
                assert_eq!(para.text_len, text.len());
                assert_eq!(para.lines.len(), WORDS * 2);
                assert!((x - cell.x - HALF_RULE).abs() < EPS);
                assert!(*y >= cell.y);
                assert!(crate::item_bottom(*y, para, *l0, *l1).unwrap() <= cell.bottom() + EPS);
                next_line = *l1;
            }
        }
    }
    assert_eq!(next_line, WORDS * 2);
}
