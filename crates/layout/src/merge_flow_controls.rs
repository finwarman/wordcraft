use super::*;
use wordcraft_doc::Table;
use wordcraft_doc::props::{Border, Borders, HeightRule, LineSpacing, ParaProps, VAlign, VMerge};

const FLOW_EPS: f32 = 0.01;

fn merge_overflow_doc(align: VAlign, spacing: f32) -> Document {
    let mut d = Document::new();
    d.last_section.page_h = 360.0;
    d.last_section.margin_top = 36.0;
    d.last_section.margin_bottom = 36.0;
    let para = |text: &str| {
        let mut p = Paragraph::with_text(text, CharProps { size: Some(9.0), font: Some("Arial".into()), ..Default::default() });
        p.props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(spacing)),
            space_before: Some(0.0),
            space_after: Some(0.0),
            widow_control: Some(false),
            ..Default::default()
        };
        wordcraft_doc::para_block(p)
    };
    let mut t = Table::new(3, 3, 300.0);
    t.props.style = None;
    t.props.cell_margins = Some([0.0; 4]);
    t.props.borders = Some(Borders::all(Border::single(0.5)));
    for (r, row) in t.rows.iter_mut().enumerate() {
        row.props.cant_split = true;
        row.props.height = Some(19.0);
        row.props.height_rule = HeightRule::AtLeast;
        row.cells[0].props.vmerge = if r == 0 { VMerge::Restart } else { VMerge::Continue };
        row.cells[2].props.vmerge = row.cells[0].props.vmerge;
        row.cells[1].blocks = vec![para(&format!("Checkpoint {r}"))];
    }
    t.rows[0].cells[0].props.valign = Some(align);
    t.rows[0].cells[0].blocks = vec![para("Label")];
    t.rows[0].cells[2].blocks = vec![para(&(0..12).map(|i| format!("Comment {i:02}")).collect::<Vec<_>>().join("\n"))];
    let mut before = Paragraph::with_text("Before", Default::default());
    before.props.line_spacing = Some(LineSpacing::Exactly(220.0));
    before.props.space_after = Some(0.0);
    d.body = vec![wordcraft_doc::para_block(before), Arc::new(Block::Table(t))];
    d
}

fn lay(doc: &Document) -> DocLayout {
    layout(doc, &mut LayoutCache::new(), &LayoutOptions::default())
}

#[test]
fn merged_lines_fit_once() {
    let doc = merge_overflow_doc(VAlign::Center, 10.0);
    let pages = lay(&doc);
    assert_eq!(pages.pages.len(), 2);
    let mut seen = Vec::new();
    for page in &pages.pages {
        let frame = page
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Cell { table, row: 0, cell: 2, rect, covered: false, .. } if table.0 == [1] => Some(rect),
                _ => None,
            })
            .unwrap();
        assert!(frame.y >= page.body.y - FLOW_EPS);
        assert!(frame.bottom() <= page.body.bottom() + FLOW_EPS);
        let mut count = 0;
        for item in &page.items {
            let Placed::Lines { path, para, l0, l1, y, .. } = item else { continue };
            if path.0 != [1, 0, 2, 0] {
                continue;
            }
            assert!(*y >= frame.y - FLOW_EPS);
            assert!(crate::item_bottom(*y, para, *l0, *l1).unwrap() <= frame.bottom() + FLOW_EPS);
            seen.extend(*l0..*l1);
            count += l1 - l0;
        }
        assert!(count > 0, "every physical fragment contains comment lines");
    }
    assert_eq!(seen, (0..12).collect::<Vec<_>>(), "every comment line appears once in source order");
}

#[test]
fn merge_label_is_not_duplicated() {
    let doc = merge_overflow_doc(VAlign::Center, 10.0);
    let pages = lay(&doc);
    let mut seen = Vec::new();
    for (page, placed) in pages.pages.iter().enumerate() {
        for item in &placed.items {
            if let Placed::Lines { path, y, .. } = item
                && path.0 == [1, 0, 0, 0]
            {
                assert!(*y >= placed.body.y && *y < placed.body.bottom());
                seen.push(page);
            }
        }
    }
    assert_eq!(seen, [0]);
}

#[test]
fn merge_fills_fit_each_fragment() {
    let mut doc = merge_overflow_doc(VAlign::Center, 10.0);
    let Block::Table(table) = Arc::make_mut(&mut doc.body[1]) else { panic!("table fixture") };
    let color = Rgb(30, 80, 160);
    table.rows[0].cells[0].props.shading = Some(color);
    let pages = lay(&doc);
    assert_eq!(pages.pages.len(), 2);
    for page in &pages.pages {
        let frame = page
            .items
            .iter()
            .find_map(|item| match item {
                Placed::Cell { table, row: 0, cell: 0, rect, covered: false, .. } if table.0 == [1] => Some(rect),
                _ => None,
            })
            .unwrap();
        let fills: Vec<_> = page
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                Placed::Fill { color: own, rect, .. } if *own == color => Some((index, rect)),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].1, frame);
        assert!(frame.y >= page.body.y - FLOW_EPS && frame.bottom() <= page.body.bottom() + FLOW_EPS);
        if let Some(ink) = page.items.iter().position(|item| matches!(item, Placed::Lines { path, .. } if path.0 == [1, 0, 0, 0])) {
            assert!(fills[0].0 < ink, "fill is painted before label ink");
        }
    }
}
