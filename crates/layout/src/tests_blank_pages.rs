use super::*;
use wordcraft_doc::props::{LineSpacing, ParaProps};
use wordcraft_doc::{Pos, Table, para_block};

const MARGIN: f32 = 24.0;
const LINE_HEIGHT: f32 = 18.0;
const GRID: [f32; 4] = [12.0, 130.0, 90.0, 60.0];
const EPSILON: f32 = 0.01;

fn paragraph(text: &str) -> Paragraph {
    let mut p = Paragraph::with_text(text, CharProps { font: Some("Source Sans 3".into()), size: Some(10.0), ..Default::default() });
    p.props = ParaProps {
        line_spacing: Some(LineSpacing::Exactly(LINE_HEIGHT)),
        space_before: Some(0.0),
        space_after: Some(0.0),
        widow_control: Some(false),
        ..Default::default()
    };
    p
}

fn lay(doc: &Document) -> DocLayout {
    layout(doc, &mut LayoutCache::new(), &LayoutOptions::default())
}

fn short_grid_doc() -> Document {
    let mut table = Table::new(10, GRID.len(), GRID.iter().sum());
    table.props.style = None;
    table.props.fixed = true;
    table.props.cell_margins = Some([0.0; 4]);
    table.grid = GRID[..GRID.len() - 1].to_vec();
    for (i, row) in table.rows.iter_mut().enumerate() {
        row.cells.truncate(2);
        row.props.height = Some(LINE_HEIGHT);
        if i == 0 {
            row.props.grid_after = 1;
            row.props.w_after = Some(GRID[3]);
            row.cells[0].props.span = 2;
            row.cells[0].props.width = Some(GRID[0] + GRID[1]);
            row.cells[1].props.width = Some(GRID[2]);
        } else {
            row.props.grid_before = 1;
            row.props.w_before = Some(GRID[0]);
            row.cells[0].props.width = Some(GRID[1]);
            row.cells[1].props.span = 2;
            row.cells[1].props.width = Some(GRID[2] + GRID[3]);
        }
        row.cells[0].blocks = vec![para_block(paragraph("MMMM MMMM MMMM"))];
        row.cells[1].blocks = vec![para_block(paragraph("Value"))];
    }
    let mut doc = Document::new();
    doc.last_section.page_w = 400.0;
    doc.last_section.page_h = 260.0;
    doc.last_section.margin_left = MARGIN;
    doc.last_section.margin_right = MARGIN;
    doc.last_section.margin_top = MARGIN;
    doc.last_section.margin_bottom = MARGIN;
    doc.body = vec![Arc::new(Block::Table(table)), para_block(paragraph(""))];
    doc
}

#[test]
fn short_grid_keeps_point_edges() {
    let doc = short_grid_doc();
    let before = doc.clone();
    let laid = lay(&doc);
    let width = laid.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Cell { row: 1, cell: 0, rect, .. } => Some(rect.w),
            _ => None,
        })
        .unwrap();
    assert!((width - GRID[1]).abs() < EPSILON, "width={width}");
    assert_eq!(laid.pages.len(), 1, "preserve room for the paragraph after the table");
    assert_eq!(laid.caret(&Pos::body(1, 0)).unwrap().page, 0);
    assert_eq!(doc, before);
}

#[test]
fn final_standalone_break() {
    let mut doc = Document::new();
    let table = Table::new(1, 1, 200.0);
    doc.body = vec![Arc::new(Block::Table(table)), para_block(paragraph("\u{000C}"))];
    assert_eq!(lay(&doc).pages.len(), 1);
}

#[test]
fn explicit_empty_page_survives() {
    for text in ["Body\u{000C}", "Body\n\u{000C}\n", "Body\n\u{000C}\n\u{000C}\nAfter"] {
        let doc = Document::from_text(text);
        let pages = if text.ends_with("After") { 3 } else { 2 };
        assert_eq!(lay(&doc).pages.len(), pages, "{text:?}");
    }
}

#[test]
fn blank_document_keeps_break() {
    let doc = Document::from_text("\u{000C}");
    assert_eq!(lay(&doc).pages.len(), 2, "preserve the existing blank-document path");
}

#[derive(Clone, Copy, Debug)]
enum GridFault {
    Conflict,
    MissingWidth,
    Percentage,
    DeclaredPrefix,
    NonFinite,
}

#[test]
fn short_grid_fallback() {
    for fault in [GridFault::Conflict, GridFault::MissingWidth, GridFault::Percentage, GridFault::DeclaredPrefix, GridFault::NonFinite] {
        let mut doc = short_grid_doc();
        let Block::Table(table) = Arc::make_mut(&mut doc.body[0]) else { panic!("table") };
        match fault {
            GridFault::Conflict => table.rows[2].cells[0].props.width = Some(GRID[1] + 1.0),
            GridFault::MissingWidth => table.rows[2].cells[0].props.width = None,
            GridFault::Percentage => table.rows[2].cells[0].props.width_pct = Some(50.0),
            GridFault::DeclaredPrefix => table.grid[1] += 1.0,
            GridFault::NonFinite => table.rows[2].props.w_before = Some(f32::INFINITY),
        }
        let laid = lay(&doc);
        let width = laid.pages[0]
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Cell { row: 1, cell: 0, rect, .. } => Some(rect.w),
                _ => None,
            })
            .unwrap();
        let fallback = doc.last_section.text_width() / GRID.len() as f32;
        assert!((width - fallback).abs() < EPSILON, "{fault:?}: width={width}");
    }
}

#[test]
fn complete_grid_widths() {
    let mut doc = short_grid_doc();
    let Block::Table(table) = Arc::make_mut(&mut doc.body[0]) else { panic!("table") };
    table.grid = GRID.to_vec();
    let laid = lay(&doc);
    assert_eq!(laid.pages.len(), 1);
    let width = laid.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Placed::Cell { row: 1, cell: 0, rect, .. } => Some(rect.w),
            _ => None,
        })
        .unwrap();
    assert!((width - GRID[1]).abs() < EPSILON);
}

#[test]
fn final_break_with_markers() {
    let mut doc = Document::from_text("Body\n\u{000C}");
    let p = doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap();
    p.insert_object(0, InlineObject::BookmarkStart { name: "probe".into() }, &Default::default()).unwrap();
    p.insert_object(p.len(), InlineObject::BookmarkEnd { name: "probe".into() }, &Default::default()).unwrap();
    let before = doc.clone();
    assert_eq!(lay(&doc).pages.len(), 1);
    assert_eq!(doc, before);
}

#[test]
fn final_break_keeps_page_decor() {
    let mut doc = Document::from_text("Body\n\u{000C}");
    for kind in [wordcraft_doc::PartKind::Header, wordcraft_doc::PartKind::Footer] {
        let mut p = paragraph("Page ");
        for instr in ["PAGE", "NUMPAGES", "SECTIONPAGES"] {
            let field =
                InlineObject::Field { instr: instr.into(), result: "9".into(), locked: false, dirty: false, code: false, form: String::new() };
            p.insert_object(p.len(), field, &Default::default()).unwrap();
        }
        let id = doc.add_part(kind, vec![para_block(p)]);
        if kind == wordcraft_doc::PartKind::Header {
            doc.last_section.headers.default = Some(id);
        } else {
            doc.last_section.footers.default = Some(id);
        }
    }
    let laid = lay(&doc);
    assert_eq!(laid.pages.len(), 1);
    for items in [&laid.pages[0].header, &laid.pages[0].footer] {
        let shown: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Placed::Lines { para, .. } => Some(para.shown.iter().map(|s| s.1.as_str())),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(shown, ["1", "1", "1"]);
    }
}

#[test]
fn final_break_keeps_endnotes() {
    let mut doc = Document::from_text("Body\n\u{000C}");
    let id = doc.add_part(wordcraft_doc::PartKind::Endnote, vec![para_block(paragraph("Endnote"))]);
    let note = InlineObject::NoteRef { kind: wordcraft_doc::para::NoteKind::Endnote, id, custom: String::new() };
    doc.insert_object(&Pos::body(0, 4), note, &Default::default()).unwrap();
    let laid = lay(&doc);
    assert_eq!(laid.pages.len(), 2, "retain the page used by the document's endnotes");
    assert!(laid.pages[1].items.iter().any(|item| matches!(item, Placed::Lines { story: StoryRef::Part(part), .. } if *part == id)));
}
