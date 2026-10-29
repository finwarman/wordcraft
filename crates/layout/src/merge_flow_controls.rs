use super::*;
use wordcraft_doc::props::{HeightRule, LineSpacing, VAlign, VMerge};

const FLOW_EPS: f32 = 0.01;
const SIBLING_HEIGHT: f32 = 81.0;
const LABEL_TOP_BAND: f32 = 0.5;

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

#[test]
fn merged_ink_stays_in_fragments() {
    let d = merge_overflow_doc(VAlign::Center, 10.0);
    with_merge_ctx(&d, |ctx, pb, _| {
        let t = d.body[1].as_table().unwrap();
        assert!(t.rows.iter().all(|r| r.props.cant_split && r.props.height_rule != HeightRule::Exact));
        let tl = table::layout_table(ctx, StoryRef::Body, t, &[1], pb.col_w(), 1);
        let count = tl.rows[0]
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Lines { path, para, .. } if path.0 == [1, 0, 2, 0] => Some(para.lines.len()),
                _ => None,
            })
            .unwrap();
        assert_eq!(count, 12, "source contains the twelve intended lines");
        eprintln!("merge-source comment_lines={count} row_heights={:?}", tl.rows.iter().map(|r| r.height).collect::<Vec<_>>());
    });
    let l = lay(&d);
    assert_eq!(l.pages.len(), 2);
    let mut seen = Vec::new();
    for (page, pg) in l.pages.iter().enumerate() {
        let boxes: Vec<_> = pg
            .items
            .iter()
            .filter_map(|it| match it {
                Placed::Cell { rect, table, cell: 1, .. } if table.0 == [1] => Some(*rect),
                _ => None,
            })
            .collect();
        let bottom = boxes.iter().map(|r| r.bottom()).fold(0.0, f32::max);
        let top = boxes.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        assert!(bottom <= pg.body.bottom() + 0.01);
        for cell in [0usize, 2] {
            let frame = pg
                .items
                .iter()
                .find_map(|it| match it {
                    Placed::Cell { rect, table, row: 0, cell: own, covered: false, .. } if table.0 == [1] && *own == cell => Some(*rect),
                    _ => None,
                })
                .unwrap();
            assert!(
                (frame.y - top).abs() < 0.01 && (frame.bottom() - bottom).abs() < 0.01,
                "page {page}: independent frame {frame:?} vs {top}..{bottom}"
            );
            let hit = l.cell_at(page, frame.x + frame.w / 2.0, frame.y + frame.h / 2.0).unwrap();
            assert_eq!((hit.2, hit.3), (0, cell));
        }
        let mut count = 0;
        for it in &pg.items {
            let Placed::Lines { path, para, l0, l1, y, .. } = it else { continue };
            if path.0 != [1, 0, 2, 0] {
                continue;
            }
            assert!(*y >= pg.body.y - 0.01, "page {page}: text above body");
            assert!(crate::item_bottom(*y, para, *l0, *l1).unwrap() <= bottom + 0.01, "page {page}: merged text crossed fragment bottom {bottom}");
            seen.extend(*l0..*l1);
            count += l1 - l0;
        }
        assert!(count > 0, "page {page}: no comment continuation");
    }
    assert_eq!(seen, (0..12).collect::<Vec<_>>(), "each comment line survives once in order");
    let labels: Vec<_> = l
        .pages
        .iter()
        .enumerate()
        .flat_map(|(page, pg)| {
            pg.items.iter().filter_map(move |it| match it {
                Placed::Lines { path, y, .. } if path.0 == [1, 0, 0, 0] => Some((page, *y)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].0, 0);
    assert!(labels[0].1 < l.pages[0].body.bottom());

    // Ordinary rows define the physical fragment independently of the merged hitbox.
    let pg = &l.pages[0];
    let rows: Vec<_> = pg
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Cell { rect, table, cell: 1, story: StoryRef::Body, .. } if table.0 == [1] => Some(*rect),
            _ => None,
        })
        .collect();
    assert!(!rows.is_empty());
    let top = rows.iter().map(|r| r.y).fold(f32::MAX, f32::min);
    let bottom = rows.iter().map(|r| r.bottom()).fold(0.0, f32::max);
    let (ink_top, ink_bottom) = pg
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Lines { path, para, l0, l1, y, .. } if path.0 == [1, 0, 0, 0] => Some((*y, crate::item_bottom(*y, para, *l0, *l1).unwrap())),
            _ => None,
        })
        .unwrap();
    let expected = top + ((bottom - top - (ink_bottom - ink_top) - LABEL_TOP_BAND) / 2.0).max(0.0) + LABEL_TOP_BAND;
    assert!((ink_top - expected).abs() < FLOW_EPS, "label {ink_top} vs independent center {expected}");
}

#[test]
fn grown_merge_centers_sibling() {
    const COMMENT_STEP: f32 = 11.0;
    const SHORT_ROW_HEIGHT: f32 = 27.0;
    const SIBLING_STEP: f32 = 10.125;

    let mut d = merge_overflow_doc(VAlign::Center, COMMENT_STEP);
    let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
    t.props.borders = Some(Borders::default());
    for row in t.rows.iter_mut().take(2) {
        row.props.height = Some(SHORT_ROW_HEIGHT);
    }
    let mut p = Paragraph::with_text(&["Sibling"; 8].join("\n"), CharProps { size: Some(9.0), ..Default::default() });
    p.props = ParaProps {
        line_spacing: Some(LineSpacing::Exactly(SIBLING_STEP)),
        space_before: Some(0.0),
        space_after: Some(0.0),
        widow_control: Some(false),
        ..Default::default()
    };
    t.rows[2].cells[1].props.valign = Some(VAlign::Center);
    t.rows[2].cells[1].blocks = vec![wordcraft_doc::para_block(p)];
    with_merge_ctx(&d, |ctx, pb, _| {
        let t = d.body[1].as_table().unwrap();
        let tl = table::layout_table(ctx, StoryRef::Body, t, &[1], pb.col_w(), 1);
        for row in tl.rows.iter().take(2) {
            assert!((row.height - SHORT_ROW_HEIGHT).abs() < FLOW_EPS, "original short row {}", row.height);
        }
        let comment_end = tl.rows[0].cells[2].end.unwrap();
        assert!((comment_end - 12.0 * COMMENT_STEP).abs() < FLOW_EPS, "original comment end {comment_end}");
        let row = &tl.rows[2];
        let cell = &row.cells[1];
        assert!((row.height - SIBLING_HEIGHT).abs() < FLOW_EPS, "original row {}", row.height);
        assert!(cell.center.is_none(), "fixture must exercise zero-offset centerNone");
        assert!(cell.top.abs() < FLOW_EPS, "borderless own inset {}", cell.top);
        assert_eq!(cell.valign, VAlign::Center);
        assert!(cell.merge.is_none());
        let y = row.items[cell.items.clone()]
            .iter()
            .find_map(|it| match it {
                Placed::Lines { y, .. } => Some(*y),
                _ => None,
            })
            .unwrap();
        assert!((y - cell.top).abs() < FLOW_EPS, "original text {y} vs own inset {}", cell.top);
        eprintln!("zero-offset source_row_height={} text_y={y} own_top={} center_none=true", row.height, cell.top);
    });
    let l = lay(&d);
    let pg = &l.pages[1];
    let frame = pg
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Cell { rect, table, row: 2, cell: 1, .. } if table.0 == [1] => Some(*rect),
            _ => None,
        })
        .unwrap();
    let (top, bottom) = pg
        .items
        .iter()
        .find_map(|it| match it {
            Placed::Lines { path, para, l0, l1, y, .. } if path.0 == [1, 2, 1, 0] => Some((*y, crate::item_bottom(*y, para, *l0, *l1).unwrap())),
            _ => None,
        })
        .unwrap();
    assert!(frame.h > 81.0, "the carry must grow the last row");
    let tail_height = 8.0 * COMMENT_STEP;
    assert!((frame.h - tail_height).abs() < FLOW_EPS, "actual grown row {} vs carried tail {tail_height}", frame.h);
    eprintln!("zero-offset grown_row_height={} sibling_height={} sibling_ink_top={top} sibling_ink_bottom={bottom}", frame.h, SIBLING_HEIGHT);
    assert!(((top + bottom) / 2.0 - (frame.y + frame.h / 2.0)).abs() < 0.01);
}

// Real shaping and page creation for controls that inspect state before final paint ordering.
fn with_merge_ctx<R>(d: &Document, f: impl FnOnce(&mut Ctx<'_>, &mut PageBuilder<'_>, f32) -> R) -> R {
    let mut cache = LayoutCache::new();
    let opts = LayoutOptions::default();
    let prior = PriorBodyWraps::new();
    let mut back = BackBodyWraps::new();
    let notes = note_info(d, opts.hides_deleted(StoryRef::Body), &HashMap::new());
    let mut ctx = Ctx {
        doc: d,
        cache: &mut cache,
        opts: &opts,
        counters: Default::default(),
        fields: FieldCtx { page: 1, pages: 1, section_pages: 1, section: 1, notes: Arc::new(notes), ..Default::default() },
        notes_hash: 0,
        numbers: HashMap::new(),
        eq_count: 0,
        boxes: Default::default(),
        chains: Default::default(),
        grid: para::Grid::of(d, &d.last_section),
        hide_deleted: opts.hides_deleted(StoryRef::Body),
        uses_totals: false,
        track_totals: true,
        pct_base: None,
        quiet_marks: Vec::new(),
        stacked_marks: Vec::new(),
        mirror: false,
        table_keys: Vec::new(),
        prior_wraps: &prior,
        back_wraps: &mut back,
        header_tables: HashMap::new(),
        story_floats: None,
        story_wraps: HashMap::new(),
        header_sections: Arc::new(d.sections()),
    };
    let sect = &d.last_section;
    let body_top = body_top_for(&mut ctx, sect, sect.headers.default);
    let first_top = if sect.title_page { body_top_for(&mut ctx, sect, sect.headers.first) } else { body_top };
    let foot_top = foot_top_for(&mut ctx, sect, sect.footers.default);
    let first_foot = if sect.title_page { foot_top_for(&mut ctx, sect, sect.footers.first) } else { foot_top };
    let mut pb = PageBuilder {
        pages: Vec::new(),
        sect,
        sect_idx: 0,
        col: 0,
        cols: Vec::new(),
        y: 0.0,
        top: 0.0,
        page_top: 0.0,
        bottom: 0.0,
        number: 0,
        web: false,
        notes: Vec::new(),
        carry: Vec::new(),
        orig_bottom: 0.0,
        prev: None,
        after_break_mark: false,
        para_end: None,
        next_starts_page: false,
        restart_at: None,
        excl: Vec::new(),
        replay_excl: Vec::new(),
        float_tables: Vec::new(),
        planned_tables: Vec::new(),
        placed_tables: Vec::new(),
        late_table: false,
        line_no: 0,
        page_items_start: 0,
        block_marks: Vec::new(),
        page_valign: wordcraft_doc::section::PageVAlign::Top,
        seps: NoteSeps::default(),
        behind: Vec::new(),
        foreground: Vec::new(),
        max_y: 0.0,
        content_bottom: 0.0,
        balance: None,
        col_break_page: None,
        sect_beneath: false,
        beneath: false,
        foot_top: first_foot,
    };
    pb.new_page(0, first_top);
    pb.foot_top = foot_top;
    f(&mut ctx, &mut pb, body_top)
}

// Candidate-only controls call the new private paginator. The parent keeps the shared controls above.
mod replay {
    use super::*;
    use wordcraft_doc::para::{Anchor, Float, NoteKind, Wrap};
    use wordcraft_doc::section::PageVAlign;

    #[derive(Clone, Copy)]
    enum TablePath {
        Original,
        Wrapper,
    }

    #[derive(Clone, Copy, Debug)]
    enum NoteRow {
        Early,
        Nested,
        Header,
    }

    fn guard_merge_doc() -> Document {
        let mut d = merge_overflow_doc(VAlign::Center, 10.0);
        let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
        // No border-band hash iteration in exact raw item-order comparisons.
        t.props.borders = None;
        d
    }

    fn prepare_merge(ctx: &mut Ctx, pb: &mut PageBuilder) -> table::TableLayout {
        let t = ctx.doc.body[1].as_table().unwrap();
        pb.prev = None;
        let tl = table::layout_table(ctx, StoryRef::Body, t, &[1], pb.col_w(), 1);
        below_floats(ctx, pb, pb.col_x() + tl.x, pb.col_x() + tl.x + tl.width, tl.rows[0].height);
        tl
    }

    fn flow_capacity(t: &Table, tl: &table::TableLayout, pb: &PageBuilder, top: f32) -> f32 {
        let headers: f32 = t.rows.iter().zip(&tl.rows).take_while(|(r, _)| r.props.header).map(|(_, r)| r.height).sum();
        pb.bottom.min(pb.page_bottom(top)) - top - headers
    }

    fn paint_state(pb: &PageBuilder) -> Vec<Vec<(std::ops::Range<usize>, PaintLayer)>> {
        pb.foreground.iter().map(|groups| groups.iter().map(|g| (g.range.clone(), g.layer)).collect()).collect()
    }

    // Compare complete values and item order. No coordinate rounding or filtering.
    fn flow_output(ctx: &Ctx, pb: &PageBuilder) -> String {
        let notes: Vec<_> = pb.notes.iter().chain(&pb.carry).map(|n| (n.id, n.h, n.cont, &n.items)).collect();
        format!(
            "{:?}",
            (
                &pb.pages,
                paint_state(pb),
                &pb.behind,
                (pb.y, pb.top, pb.bottom, pb.orig_bottom, pb.max_y, pb.content_bottom),
                (pb.number, pb.col, pb.line_no, &pb.prev, pb.after_break_mark),
                (pb.notes.len(), pb.carry.len(), notes),
                (&ctx.fields, &ctx.counters, ctx.eq_count, ctx.boxes.total(), ctx.uses_totals),
            )
        )
    }

    fn table_output(d: &Document, path: TablePath) -> String {
        with_merge_ctx(d, |ctx, pb, top| {
            place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
            match path {
                TablePath::Original => {
                    let tl = prepare_merge(ctx, pb);
                    assert!(place_table_rows(ctx, pb, d.body[1].as_table().unwrap(), &tl, 1, top, table::MergeFlow::disabled()));
                }
                TablePath::Wrapper => place_table(ctx, pb, d.body[1].as_table().unwrap(), 1, top),
            }
            for (block, para) in d.body.iter().enumerate().skip(2) {
                place_para(ctx, pb, para.as_para().unwrap(), block, top);
            }
            pb.flush_notes();
            flow_output(ctx, pb)
        })
    }

    fn late_fit_doc() -> Document {
        let mut d = guard_merge_doc();
        d.last_section.header = 0.0;
        d.last_section.title_page = true;
        let empty = d.add_part(wordcraft_doc::PartKind::Header, vec![wordcraft_doc::para_block(Paragraph::new())]);
        d.last_section.headers.first = Some(empty);
        let mut header = Paragraph::new();
        let float = Float { wrap: Wrap::TopAndBottom, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 72.0, y: 36.0, ..Default::default() };
        header.insert_object(0, rect_shape(468.0, 270.0, float), &Default::default()).unwrap();
        let id = d.add_part(wordcraft_doc::PartKind::Header, vec![wordcraft_doc::para_block(header)]);
        d.last_section.headers.default = Some(id);
        let Block::Para(before) = Arc::make_mut(&mut d.body[0]) else { panic!("unexpected control block") };
        let front =
            Float { wrap: Wrap::InFrontOfText, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 500.0, y: 45.0, z_order: 7, ..Default::default() };
        before.insert_object(0, rect_shape(8.0, 8.0, front), &Default::default()).unwrap();
        let mut after = Paragraph::with_text("After replay", Default::default());
        after.props =
            ParaProps { line_spacing: Some(LineSpacing::Exactly(10.0)), space_before: Some(0.0), space_after: Some(0.0), ..Default::default() };
        d.body.push(wordcraft_doc::para_block(after));
        d
    }

    #[test]
    fn late_fit_restores_original() {
        let d = late_fit_doc();
        with_merge_ctx(&d, |ctx, pb, top| {
            assert!((top - 36.0).abs() < FLOW_EPS, "fixture requires a float, not a taller header");
            place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
            assert!(pb.foreground[0].iter().any(|g| matches!(g.layer, PaintLayer::Foreground(7))));
            let tl = prepare_merge(ctx, pb);
            let t = d.body[1].as_table().unwrap();
            let flow = table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top));
            assert!(!flow.is_empty(), "fixture must enter streaming");
            let prefix = flow_output(ctx, pb);
            let snap = Snap::take(pb, ctx);
            assert!(!place_table_rows(ctx, pb, t, &tl, 1, top, flow), "actual second-page fit must reject");
            assert!(pb.pages.len() > 1, "failure must follow a real page advance");
            assert!(pb.pages[0].items.iter().any(|it| matches!(it, Placed::Lines { path, l0, l1, .. } if path.0 == [1, 0, 2, 0] && l1 > l0)));
            assert!(
                pb.pages
                    .last()
                    .unwrap()
                    .items
                    .iter()
                    .any(|it| matches!(it, Placed::Cell { rect, table, .. } if table.0 == [1] && rect.bottom() > pb.bottom + FLOW_EPS))
            );
            eprintln!("late-fit pages={} physical_floor={} final_y={}", pb.pages.len(), pb.bottom, pb.y);
            snap.restore(pb, ctx);
            assert_eq!(flow_output(ctx, pb), prefix, "rollback must retain the prefix and foreground exactly");
        });
        assert_eq!(
            table_output(&d, TablePath::Wrapper),
            table_output(&d, TablePath::Original),
            "wrapper fallback must preserve complete pages, paint ranges and following content"
        );
    }

    fn note_table_doc(kind: NoteRow) -> (Document, Vec<u32>) {
        let mut d = guard_merge_doc();
        let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
        let mut early = t.rows[0].clone();
        let plain = early.cells[1].blocks.clone();
        for cell in &mut early.cells {
            cell.props.vmerge = VMerge::None;
            cell.blocks = plain.clone();
        }
        let path = match kind {
            NoteRow::Nested => {
                let mut nested = Table::new(1, 1, 80.0);
                nested.props.style = None;
                nested.rows[0].cells[0].blocks = plain;
                early.cells[1].blocks = vec![Arc::new(Block::Table(nested))];
                vec![1, 0, 1, 0, 0, 0, 0]
            }
            NoteRow::Header => {
                early.props.header = true;
                vec![1, 0, 1, 0]
            }
            NoteRow::Early => vec![1, 0, 1, 0],
        };
        t.rows.insert(0, early);
        (d, path)
    }

    #[test]
    fn unrelated_notes_disable_flow() {
        for kind in [NoteRow::Early, NoteRow::Nested, NoteRow::Header] {
            let (mut d, path) = note_table_doc(kind);
            with_merge_ctx(&d, |ctx, pb, top| {
                let tl = prepare_merge(ctx, pb);
                let t = d.body[1].as_table().unwrap();
                assert!(!table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "{kind:?}: note-free group must be eligible");
            });
            let id =
                d.add_part(wordcraft_doc::PartKind::Footnote, vec![wordcraft_doc::para_block(Paragraph::with_text("Real note", Default::default()))]);
            d.insert_object(
                &wordcraft_doc::Pos::new(StoryRef::Body, Path(path.clone()), 0),
                InlineObject::NoteRef { kind: NoteKind::Footnote, id, custom: String::new() },
                &Default::default(),
            )
            .unwrap();
            with_merge_ctx(&d, |ctx, pb, top| {
                let tl = prepare_merge(ctx, pb);
                let t = d.body[1].as_table().unwrap();
                assert!(
                    tl.rows[0]
                        .items
                        .iter()
                        .any(|it| matches!(it, Placed::Lines { path: own, para, .. } if own.0 == path && para.notes.iter().any(|(_, n)| *n == id)))
                );
                assert!(tl.rows.iter().skip(1).flat_map(|r| &r.items).all(|it| !matches!(it, Placed::Lines { para, .. } if !para.notes.is_empty())));
                assert!(table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "{kind:?}: whole-table note guard");
                eprintln!("whole-table note {kind:?} source_path={path:?} id={id} flow_disabled=true");
            });
            assert_eq!(
                table_output(&d, TablePath::Wrapper),
                table_output(&d, TablePath::Original),
                "{kind:?}: guarded note table must keep original placement"
            );
        }
    }

    #[test]
    fn non_top_keeps_original_path() {
        for align in [PageVAlign::Center, PageVAlign::Bottom, PageVAlign::Justify] {
            let mut d = guard_merge_doc();
            d.last_section.valign = align;
            with_merge_ctx(&d, |ctx, pb, top| {
                place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
                let tl = prepare_merge(ctx, pb);
                let t = d.body[1].as_table().unwrap();
                assert_eq!(pb.page_valign, align);
                assert!(!table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "the valign gate must be the exclusion");
                eprintln!("non-top {align:?}: plain group eligible, wrapper gate excludes");
            });
            assert_eq!(
                table_output(&d, TablePath::Wrapper),
                table_output(&d, TablePath::Original),
                "{align:?}: retained page prefix must follow original alignment"
            );
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum BottomInsets {
        Zero,
        Padded,
    }

    fn bottom_sibling_doc(insets: BottomInsets) -> Document {
        let mut d = merge_overflow_doc(VAlign::Center, 11.0);
        let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
        t.props.borders = Some(Borders::default());
        for row in t.rows.iter_mut().take(2) {
            row.props.height = Some(27.0);
        }
        let mut p = Paragraph::with_text(&["Sibling"; 8].join("\n"), CharProps { size: Some(9.0), ..Default::default() });
        p.props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(SIBLING_HEIGHT / 8.0)),
            space_before: Some(0.0),
            space_after: Some(0.0),
            widow_control: Some(false),
            ..Default::default()
        };
        let cell = &mut t.rows[2].cells[1];
        cell.props.valign = Some(VAlign::Bottom);
        if matches!(insets, BottomInsets::Padded) {
            cell.props.margins = Some([2.0, 0.0, 3.0, 0.0]);
        }
        cell.blocks = vec![wordcraft_doc::para_block(p)];
        d
    }

    #[test]
    fn bottom_sibling_grows_inset() {
        for insets in [BottomInsets::Zero, BottomInsets::Padded] {
            let d = bottom_sibling_doc(insets);
            let (upper, lower) = match insets {
                BottomInsets::Zero => (0.0, 0.0),
                BottomInsets::Padded => (2.0, 3.0),
            };
            let (old_height, old_y) = with_merge_ctx(&d, |ctx, pb, top| {
                let tl = prepare_merge(ctx, pb);
                let row = &tl.rows[2];
                let cell = &row.cells[1];
                assert_eq!(cell.valign, VAlign::Bottom);
                assert!(cell.merge.is_none() && cell.center.is_none());
                assert!((row.height - SIBLING_HEIGHT - upper - lower).abs() < FLOW_EPS);
                assert!((cell.top - upper).abs() < FLOW_EPS);
                let (y, bottom) = row.items[cell.items.clone()]
                    .iter()
                    .find_map(|it| match it {
                        Placed::Lines { para, l0, l1, y, .. } => {
                            assert_eq!((*l0, *l1), (0, 8));
                            Some((*y, crate::item_bottom(*y, para, *l0, *l1).unwrap()))
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!((y - upper).abs() < FLOW_EPS);
                assert!((row.height - bottom - lower).abs() < FLOW_EPS);
                let t = d.body[1].as_table().unwrap();
                assert!(!table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "ordinary Bottom must permit the comment flow");
                eprintln!("bottom-source {insets:?} height={} ink={y}..{bottom} lower={lower}", row.height);
                (row.height, y)
            });
            let layout = lay(&d);
            assert_eq!(layout.pages.len(), 2);
            let pg = &layout.pages[1];
            let frame = pg
                .items
                .iter()
                .find_map(|it| match it {
                    Placed::Cell { rect, table, row: 2, cell: 1, covered: false, .. } if table.0 == [1] => Some(*rect),
                    _ => None,
                })
                .unwrap();
            let (y, bottom) = pg
                .items
                .iter()
                .find_map(|it| match it {
                    Placed::Lines { path, para, l0, l1, y, .. } if path.0 == [1, 2, 1, 0] => {
                        assert_eq!((*l0, *l1), (0, 8));
                        Some((*y, crate::item_bottom(*y, para, *l0, *l1).unwrap()))
                    }
                    _ => None,
                })
                .unwrap();
            let expected = 8.0 * 11.0 + lower;
            let delta = frame.h - old_height;
            assert!((frame.h - expected).abs() < FLOW_EPS && delta > 0.0, "{insets:?}: frame {frame:?}, old {old_height}");
            assert!((frame.y - pg.body.y).abs() < FLOW_EPS && frame.bottom() <= pg.body.bottom() + FLOW_EPS);
            assert!((y - frame.y - old_y - delta).abs() < FLOW_EPS, "Bottom ink must move by the full growth delta");
            assert!((frame.bottom() - bottom - lower).abs() < FLOW_EPS, "Bottom ink must retain its own lower inset");
            let hit = layout.cell_at(1, frame.x + frame.w / 2.0, frame.y + frame.h / 2.0).unwrap();
            assert_eq!(hit, (StoryRef::Body, Path::top(1), 2, 1));
            let mut seen = Vec::new();
            for pg in &layout.pages {
                let before = seen.len();
                for it in &pg.items {
                    if let Placed::Lines { path, l0, l1, .. } = it
                        && path.0 == [1, 0, 2, 0]
                    {
                        seen.extend(*l0..*l1);
                    }
                }
                assert!(seen.len() > before, "each physical page must hold comment lines");
            }
            assert_eq!(seen, (0..12).collect::<Vec<_>>());
            eprintln!("bottom-grown {insets:?} frame={frame:?} delta={delta} ink={y}..{bottom} lower={lower}");
        }
    }

    #[test]
    fn bottom_merge_keeps_old_route() {
        let mut d = bottom_sibling_doc(BottomInsets::Zero);
        let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
        t.rows[0].cells[2].props.valign = Some(VAlign::Bottom);
        with_merge_ctx(&d, |ctx, pb, top| {
            let tl = prepare_merge(ctx, pb);
            let cell = &tl.rows[0].cells[2];
            assert_eq!(cell.valign, VAlign::Bottom);
            assert!(cell.merge.is_some());
            let t = d.body[1].as_table().unwrap();
            assert!(table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "Bottom merged cells remain unsupported");
        });
        assert_eq!(table_output(&d, TablePath::Wrapper), table_output(&d, TablePath::Original));
    }

    #[test]
    fn same_page_merge_keeps_cells() {
        let mut d = Document::new();
        let mut before = Paragraph::with_text("Before", Default::default());
        before.props =
            ParaProps { line_spacing: Some(LineSpacing::Exactly(10.0)), space_before: Some(0.0), space_after: Some(0.0), ..Default::default() };
        let mut t = band_row_table(None);
        t.rows[0].cells[0].props.vmerge = VMerge::Restart;
        t.rows[1].cells[0].props.vmerge = VMerge::Continue;
        let mut p = Paragraph::with_text("First\nSecond\nThird", Default::default());
        p.props = ParaProps {
            line_spacing: Some(LineSpacing::Exactly(10.0)),
            space_before: Some(0.0),
            space_after: Some(0.0),
            widow_control: Some(false),
            ..Default::default()
        };
        t.rows[0].cells[0].blocks = vec![wordcraft_doc::para_block(p)];
        d.body = vec![wordcraft_doc::para_block(before), Arc::new(Block::Table(t))];
        with_merge_ctx(&d, |ctx, pb, top| {
            place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
            let tl = prepare_merge(ctx, pb);
            let height: f32 = tl.rows.iter().map(|r| r.height).sum();
            assert!(pb.y + height <= pb.bottom, "the whole table must fit this physical page");
            assert!(tl.rows[0].cells[0].end.unwrap() > tl.rows[0].height, "ink must cross the first logical row");
            assert!(tl.rows[0].cells[0].center.is_none());
            let t = d.body[1].as_table().unwrap();
            assert!(!table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "the wrapper fit gate must be the exclusion");
        });
        let layout = lay(&d);
        assert_eq!(layout.pages.len(), 1);
        let cells: Vec<_> = layout.pages[0]
            .items
            .iter()
            .filter_map(|it| match it {
                Placed::Cell { rect, row, covered, table, .. } if table.0 == [1] => Some((*row, rect.h, *covered)),
                _ => None,
            })
            .collect();
        assert_eq!(cells, [(0, 42.0, false), (1, 20.0, true), (2, 22.0, false), (3, 24.0, false)]);
        assert_eq!(
            table_output(&d, TablePath::Wrapper),
            table_output(&d, TablePath::Original),
            "same-page metadata, order and line positions stay original"
        );
    }

    #[test]
    fn other_objects_keep_old_route() {
        for object in [
            InlineObject::CommentStart { id: 77 },
            InlineObject::FieldEnd,
            InlineObject::Opaque { format: "synthetic".into(), xml: "<unknown/>".into(), text: String::new() },
        ] {
            let mut d = guard_merge_doc();
            let path = Path(vec![1, 1, 1, 0]);
            d.para_mut(StoryRef::Body, &path).unwrap().insert_object(0, object.clone(), &Default::default()).unwrap();
            with_merge_ctx(&d, |ctx, pb, top| {
                let tl = prepare_merge(ctx, pb);
                let t = d.body[1].as_table().unwrap();
                assert_eq!(d.para(StoryRef::Body, &path).unwrap().objects.as_slice(), std::slice::from_ref(&object));
                assert!(table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top)).is_empty(), "unadmitted object {object:?}");
            });
            assert_eq!(table_output(&d, TablePath::Wrapper), table_output(&d, TablePath::Original), "unadmitted object {object:?}");
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum ExcludedMerge {
        Short,
        Shaded,
    }

    fn overlap_merge_doc(kind: ExcludedMerge) -> Document {
        let mut d = Document::new();
        d.last_section.page_h = 172.0;
        d.last_section.margin_top = 36.0;
        d.last_section.margin_bottom = 36.0;
        let para = |text: &str, height| {
            let mut p = Paragraph::with_text(text, CharProps { size: Some(9.0), font: Some("Arial".into()), ..Default::default() });
            p.props = ParaProps {
                line_spacing: Some(LineSpacing::Exactly(height)),
                space_before: Some(0.0),
                space_after: Some(0.0),
                widow_control: Some(true),
                ..Default::default()
            };
            wordcraft_doc::para_block(p)
        };
        let mut t = Table::new(4, 2, 200.0);
        t.props.style = None;
        t.props.cell_margins = Some([0.0; 4]);
        t.props.borders = Some(Borders::default());
        for (ri, row) in t.rows.iter_mut().enumerate() {
            row.props.cant_split = true;
            row.props.height = Some(20.0);
            row.props.height_rule = HeightRule::AtLeast;
            row.cells[0].props.vmerge = if ri == 0 { VMerge::Restart } else { VMerge::Continue };
            row.cells[1].blocks = vec![para("Ordinary", 10.0)];
        }
        t.rows[0].cells[0].blocks = vec![para("Tall first\nTall second", 35.0)];
        t.rows[2].cells[1].props.vmerge = VMerge::Restart;
        t.rows[3].cells[1].props.vmerge = VMerge::Continue;
        t.rows[2].cells[1].blocks = vec![para("Short", 10.0)];
        if matches!(kind, ExcludedMerge::Shaded) {
            t.rows[2].cells[1].props.shading = Some(wordcraft_doc::Rgb(200, 210, 220));
        }
        d.body = vec![para("Before", 55.0), Arc::new(Block::Table(t)), para("After replay", 10.0)];
        d
    }

    #[test]
    fn excluded_merge_replays() {
        for kind in [ExcludedMerge::Short, ExcludedMerge::Shaded] {
            let d = overlap_merge_doc(kind);
            with_merge_ctx(&d, |ctx, pb, top| {
                place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
                let tl = prepare_merge(ctx, pb);
                let t = d.body[1].as_table().unwrap();
                assert!((flow_capacity(t, &tl, pb, top) - 100.0).abs() < FLOW_EPS);
                assert!((pb.bottom - pb.y - 45.0).abs() < FLOW_EPS);
                assert_eq!(tl.rows.iter().map(|r| r.height).collect::<Vec<_>>(), [20.0; 4]);
                let tall = &tl.rows[0].cells[0];
                assert!((tall.end.unwrap() - 70.0).abs() < FLOW_EPS);
                let para = tl.rows[0].items[tall.items.clone()]
                    .iter()
                    .find_map(|it| match it {
                        Placed::Lines { para, .. } => Some(para),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(para.lines.len(), 2);
                assert!(para.rp.widow_control);
                assert!(para.lines.iter().all(|line| (line.height - 35.0).abs() < FLOW_EPS));
                let short = &tl.rows[2].cells[1];
                assert_eq!(short.merge.as_ref().unwrap().key, (2, 1));
                assert!(short.center.is_none() && short.end.unwrap() <= tl.rows[2].height);
                let flow = table::MergeFlow::new(t, &tl, flow_capacity(t, &tl, pb, top));
                assert!(!flow.is_empty(), "{kind:?}: long merge must enroll");
                let row = flow.row(&tl.rows[2], 2).unwrap();
                assert!(
                    row.items
                        .iter()
                        .any(|it| matches!(it, Placed::Cell { row: 2, cell: 1, covered: false, rect, .. } if (rect.h - 40.0).abs() < FLOW_EPS)),
                    "excluded restart frame stays on the original route"
                );
                assert_eq!(row.items.iter().any(|it| matches!(it, Placed::Fill { .. })), matches!(kind, ExcludedMerge::Shaded));
                assert!(row.items.iter().any(|it| matches!(it, Placed::Lines { path, .. } if path.0 == [1, 2, 1, 0])));
                assert!(!row.items.iter().any(|it| matches!(it, Placed::Cell { cell: 0, .. })), "only the long merge is enrolled");
                drop(row);
                let prefix = flow_output(ctx, pb);
                let snap = Snap::take(pb, ctx);
                assert!(!place_table_rows(ctx, pb, t, &tl, 1, top, flow), "{kind:?}: overlapping excluded merge must reject late growth");
                assert_eq!(pb.pages.len(), 2, "failure follows actual page advance");
                assert!((pb.y - top - 20.0).abs() < FLOW_EPS, "row 2 must be placed before row 3 rejects growth");
                let pg = &pb.pages[1];
                assert!(
                    pg.items
                        .iter()
                        .any(|it| matches!(it, Placed::Cell { row: 2, cell: 1, covered: false, rect, .. } if (rect.h - 40.0).abs() < FLOW_EPS))
                );
                assert!(!pg.items.iter().any(|it| matches!(it, Placed::Cell { row: 3, .. })), "failed row must not be placed");
                assert!(
                    !pb.pages.iter().flat_map(|pg| &pg.items).any(|it| matches!(it, Placed::Lines { path, .. } if path.0 == [1, 0, 0, 0])),
                    "widow control must carry both tall lines before the late failure"
                );
                eprintln!(
                    "excluded-merge {kind:?}: original=20/20/20/20 pending=70 next_fragment=20 requested_last=50 pages={} y={}",
                    pb.pages.len(),
                    pb.y
                );
                snap.restore(pb, ctx);
                assert_eq!(flow_output(ctx, pb), prefix, "late rejection must restore the retained prefix");
            });
            assert_eq!(
                table_output(&d, TablePath::Wrapper),
                table_output(&d, TablePath::Original),
                "{kind:?}: exact original replay includes fills, hit frames and following content"
            );
            let layout = lay(&d);
            assert_eq!(layout.pages.len(), 2);
            let pg = &layout.pages[1];
            let frame = pg
                .items
                .iter()
                .find_map(|it| match it {
                    Placed::Cell { row: 2, cell: 1, covered: false, rect, table, .. } if table.0 == [1] => Some(*rect),
                    _ => None,
                })
                .unwrap();
            assert!((frame.h - 40.0).abs() < FLOW_EPS);
            assert_eq!(layout.cell_at(1, frame.x + frame.w / 2.0, frame.bottom() - 1.0).unwrap(), (StoryRef::Body, Path::top(1), 2, 1));
            if matches!(kind, ExcludedMerge::Shaded) {
                assert!(
                    pg.items.iter().any(|it| matches!(it, Placed::Fill { rect, .. } if *rect == frame)),
                    "shading and uncovered hit frame retain the same original band"
                );
            }
        }
    }
}

#[test]
fn bookmarks_follow_merge_lines() {
    let mut d = merge_overflow_doc(VAlign::Center, 10.0);
    let label = Path(vec![1, 0, 0, 0]);
    let comment = Path(vec![1, 0, 2, 0]);
    for (path, name, text) in [(&label, "MergeLabel", "Label"), (&comment, "MergeTail", "Comment 08")] {
        let p = d.para_mut(StoryRef::Body, path).unwrap();
        let off = p.text.find(text).unwrap();
        let end = off + text.len();
        let props = p.props_at(off).clone();
        p.insert_object(end, InlineObject::BookmarkEnd { name: name.into() }, &props).unwrap();
        p.insert_object(off, InlineObject::BookmarkStart { name: name.into() }, &props).unwrap();
    }
    let label_source = d.para(StoryRef::Body, &label).unwrap().clone();
    let comment_source = d.para(StoryRef::Body, &comment).unwrap().clone();
    assert_eq!(comment_source.plain_text().lines().count(), 12);
    with_merge_ctx(&d, |ctx, pb, _| {
        let t = d.body[1].as_table().unwrap();
        assert!(t.rows.iter().all(|r| r.props.cant_split && r.props.height_rule != HeightRule::Exact));
        assert_eq!(t.rows[0].cells[0].props.vmerge, VMerge::Restart);
        let tl = table::layout_table(ctx, StoryRef::Body, t, &[1], pb.col_w(), 1);
        let para = tl.rows[0]
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Lines { path, para, .. } if *path == comment => Some(para),
                _ => None,
            })
            .unwrap();
        assert_eq!(para.lines.len(), 12, "source contains the twelve intended lines");
        assert_eq!(para.line_of(comment_source.object_offsets()[0]), 8, "comment start marker must be on the carried line");
        eprintln!("bookmark-source comment_lines=12 neighbor_markers={} carried_marker_line=8", label_source.objects.len());
    });
    let layout = lay(&d);
    assert_eq!(layout.pages.len(), 2);
    for (path, source, marker_page, marker_line, count) in [(&label, &label_source, 0, 0, 1), (&comment, &comment_source, 1, 8, 12)] {
        let offsets = source.object_offsets();
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets, source.text.char_indices().filter_map(|(off, c)| (c == wordcraft_doc::para::OBJ).then_some(off)).collect::<Vec<_>>());
        assert!(matches!(&source.objects[..], [InlineObject::BookmarkStart { .. }, InlineObject::BookmarkEnd { .. }]));
        let mut lines = Vec::new();
        let mut ranges = Vec::new();
        let mut markers = Vec::new();
        for (page, pg) in layout.pages.iter().enumerate() {
            let bottom = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Placed::Cell { rect, table, cell: 1, .. } if table.0 == [1] => Some(rect.bottom()),
                    _ => None,
                })
                .fold(0.0, f32::max);
            let before = lines.len();
            for it in &pg.items {
                let Placed::Lines { path: own, para, l0, l1, y, .. } = it else { continue };
                if own != path {
                    continue;
                }
                assert_eq!(para.lines.len(), count, "bookmarks take no extra line");
                assert!(
                    *y >= pg.body.y - FLOW_EPS && crate::item_bottom(*y, para, *l0, *l1).unwrap() <= bottom + FLOW_EPS,
                    "bookmarked ink stays in the independent physical row frame"
                );
                for li in *l0..*l1 {
                    lines.push(li);
                    let line = &para.lines[li];
                    for cluster in &para.clusters[line.c0..line.c1] {
                        ranges.push(cluster.start..cluster.end);
                        if offsets.contains(&cluster.start) {
                            assert_eq!(cluster.kind, para::ClKind::Marker);
                            assert_eq!(cluster.adv, 0.0);
                            markers.push((cluster.start, page, li));
                        }
                    }
                }
            }
            if path == &comment {
                assert!(lines.len() > before, "each page owns comment lines");
            }
        }
        assert_eq!(lines, (0..count).collect::<Vec<_>>());
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, source.len());
        assert!(ranges.windows(2).all(|r| r[0].end == r[1].start), "source bytes appear once in order");
        let restored: String = ranges.iter().map(|r| source.text.get(r.clone()).unwrap()).collect();
        assert_eq!(restored, source.text);
        assert_eq!(
            markers,
            offsets.iter().map(|off| (*off, marker_page, marker_line)).collect::<Vec<_>>(),
            "each bookmark marker has one physical owner"
        );
        for (off, page, _) in markers {
            let pos = Pos { story: StoryRef::Body, path: path.clone(), off };
            let caret = layout.caret(&pos).unwrap();
            let after = layout.caret(&Pos { off: off + wordcraft_doc::para::OBJ.len_utf8(), ..pos }).unwrap();
            assert_eq!((caret.page, after.page), (page, page));
            assert!((caret.x - after.x).abs() < FLOW_EPS && (caret.top - after.top).abs() < FLOW_EPS, "bookmark has zero-width caret geometry");
            assert!(caret.top >= layout.pages[page].body.y - FLOW_EPS && caret.top + caret.height <= layout.pages[page].body.bottom() + FLOW_EPS);
        }
        assert_eq!(d.para(StoryRef::Body, path).unwrap(), source, "layout preserves source markers and offsets");
    }
    let pages = bookmark_pages(&d, &layout.pages, &d.sections());
    assert_eq!(pages.len(), 2);
    assert_eq!(pages.get("MergeLabel").map(|(page, _)| *page), Some(1));
    assert_eq!(pages.get("MergeTail").map(|(page, _)| *page), Some(2));
}

const LABEL_FILLS: [wordcraft_doc::Rgb; 2] = [wordcraft_doc::Rgb(255, 255, 255), wordcraft_doc::Rgb(30, 80, 160)];

fn filled_merge_doc(color: wordcraft_doc::Rgb) -> Document {
    use wordcraft_doc::para::{Anchor, Float, Wrap};
    let mut d = merge_overflow_doc(VAlign::Center, 10.0);
    let Block::Table(t) = Arc::make_mut(&mut d.body[1]) else { panic!("unexpected control block") };
    t.rows[0].cells[0].props.shading = Some(color);
    let p = d.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    let behind = Float { wrap: Wrap::BehindText, h_rel: Anchor::Page, v_rel: Anchor::Page, x: 8.0, y: 8.0, ..Default::default() };
    p.insert_object(0, rect_shape(8.0, 8.0, behind), &Default::default()).unwrap();
    d
}

#[test]
fn filled_label_stays_in_bounds() {
    for color in LABEL_FILLS {
        let d = filled_merge_doc(color);
        with_merge_ctx(&d, |ctx, pb, _| {
            let t = d.body[1].as_table().unwrap();
            let tl = table::layout_table(ctx, StoryRef::Body, t, &[1], pb.col_w(), 1);
            let cell = &tl.rows[0].cells[0];
            assert_eq!(cell.valign, VAlign::Center);
            assert!(cell.center.is_some() && cell.merge.is_some());
            let owned = &tl.rows[0].items[cell.items.clone()];
            let frame = owned
                .iter()
                .find_map(|it| match it {
                    Placed::Cell { rect, .. } => Some(*rect),
                    _ => None,
                })
                .unwrap();
            let fills: Vec<_> = owned
                .iter()
                .filter_map(|it| match it {
                    Placed::Fill { rect, color } => Some((*rect, *color)),
                    _ => None,
                })
                .collect();
            assert_eq!(fills, [(frame, color)], "one source fill exactly matches its restart frame");
            let count = tl.rows[0]
                .items
                .iter()
                .find_map(|it| match it {
                    Placed::Lines { path, para, .. } if path.0 == [1, 0, 2, 0] => Some(para.lines.len()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(count, 12);
            eprintln!("filled-source color={color:?} comment_lines={count} center=true fill_equals_frame=true");
        });
        let layout = lay(&d);
        assert_eq!(layout.pages.len(), 2);
        for (page, pg) in layout.pages.iter().enumerate() {
            let rows: Vec<_> = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Placed::Cell { rect, table, cell: 1, .. } if table.0 == [1] => Some(*rect),
                    _ => None,
                })
                .collect();
            assert!(!rows.is_empty());
            let top = rows.iter().map(|r| r.y).fold(f32::MAX, f32::min);
            let bottom = rows.iter().map(|r| r.bottom()).fold(0.0, f32::max);
            let frames: Vec<_> = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Placed::Cell { rect, table, row: 0, cell: 0, covered: false, .. } if table.0 == [1] => Some(*rect),
                    _ => None,
                })
                .collect();
            assert_eq!(frames.len(), 1);
            let frame = frames[0];
            assert!(
                (frame.y - top).abs() < FLOW_EPS && (frame.bottom() - bottom).abs() < FLOW_EPS,
                "filled label must match the independent physical row band"
            );
            let fills: Vec<_> = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Placed::Fill { rect, color } => Some((*rect, *color)),
                    _ => None,
                })
                .collect();
            assert_eq!(fills, [(frame, color)], "each physical fragment needs one matching fill, including the ink-empty continuation");
            assert!([frame.x, frame.y, frame.w, frame.h].iter().all(|v| v.is_finite()));
            assert!(frame.h > 0.0 && frame.y >= pg.body.y - FLOW_EPS && frame.bottom() <= pg.body.bottom() + FLOW_EPS, "no off-page fill");
            assert_eq!(layout.cell_at(page, frame.x + frame.w / 2.0, frame.y + frame.h / 2.0).unwrap(), (StoryRef::Body, Path::top(1), 0, 0));
        }
        for (path, count) in [(Path(vec![1, 0, 0, 0]), 1), (Path(vec![1, 0, 2, 0]), 12)] {
            let source = d.para(StoryRef::Body, &path).unwrap();
            let mut lines = Vec::new();
            let mut ranges = Vec::new();
            let mut owners = Vec::new();
            for (page, pg) in layout.pages.iter().enumerate() {
                let before = lines.len();
                for it in &pg.items {
                    let Placed::Lines { path: own, para, l0, l1, y, .. } = it else { continue };
                    if own != &path {
                        continue;
                    }
                    owners.push(page);
                    lines.extend(*l0..*l1);
                    let frame = pg
                        .items
                        .iter()
                        .find_map(|it| match it {
                            Placed::Cell { rect, table, row: 0, cell, covered: false, .. } if table.0 == [1] && *cell as u32 == path.0[2] => {
                                Some(*rect)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let bottom = crate::item_bottom(*y, para, *l0, *l1).unwrap();
                    assert!(*y >= frame.y - FLOW_EPS && bottom <= frame.bottom() + FLOW_EPS);
                    if count == 1 {
                        let expected = frame.y + ((frame.h - (bottom - *y) - LABEL_TOP_BAND) / 2.0).max(0.0) + LABEL_TOP_BAND;
                        assert!((*y - expected).abs() < FLOW_EPS, "filled label remains centered in its first physical fragment");
                    }
                    for line in &para.lines[*l0..*l1] {
                        ranges.extend(para.clusters[line.c0..line.c1].iter().map(|c| c.start..c.end));
                    }
                }
                if count == 12 {
                    assert!(lines.len() > before, "each page owns comment lines");
                }
            }
            assert_eq!(lines, (0..count).collect::<Vec<_>>());
            assert_eq!(ranges.first().unwrap().start, 0);
            assert_eq!(ranges.last().unwrap().end, source.len());
            assert!(ranges.windows(2).all(|r| r[0].end == r[1].start));
            assert_eq!(ranges.iter().map(|r| source.text.get(r.clone()).unwrap()).collect::<String>(), source.text);
            if count == 1 {
                assert_eq!(owners, [0], "label ink appears once on the first page");
            }
        }
    }
}

#[test]
fn filled_label_paint_order() {
    for color in LABEL_FILLS {
        let d = filled_merge_doc(color);
        with_merge_ctx(&d, |ctx, pb, top| {
            place_para(ctx, pb, d.body[0].as_para().unwrap(), 0, top);
            let behind = pb.behind[0];
            assert!(behind > 0, "fixture must exercise the behind-item offset");
            let anchor = pb.pages[0].items.len() - behind;
            assert!(anchor > 0, "preamble must precede the table");
            place_table(ctx, pb, d.body[1].as_table().unwrap(), 1, top);
            assert_eq!(pb.pages.len(), 2);
            for (page, pg) in pb.pages.iter().enumerate() {
                let behind = pb.behind[page];
                let expected_anchor = if page == 0 { anchor } else { 0 };
                if page != 0 {
                    assert_eq!(behind, 0);
                }
                let fills: Vec<_> = pg
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, it)| match it {
                        Placed::Fill { color: own, .. } if *own == color => Some(i),
                        _ => None,
                    })
                    .collect();
                assert_eq!(fills.len(), 1, "each physical fragment needs its own paint group");
                let index = fills[0].checked_sub(behind).unwrap();
                let groups: Vec<_> = pb.foreground[page].iter().filter(|g| g.range.contains(&index)).collect();
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].range, index..index + 1);
                assert_eq!(groups[0].layer, PaintLayer::Before(expected_anchor), "fill must use this table fragment's anchor");
                let mut ordered = pg.items.clone();
                order_foreground(&mut ordered, &pb.foreground[page], behind);
                let fill = ordered.iter().position(|it| matches!(it, Placed::Fill { color: own, .. } if *own == color)).unwrap();
                if page == 0 {
                    let preamble = ordered.iter().position(|it| matches!(it, Placed::Lines { path, .. } if path.0 == [0])).unwrap();
                    assert!(preamble < fill, "table fill must stay after the preamble");
                    assert!(
                        ordered[..behind].iter().any(|it| matches!(it, Placed::Object { path, .. } if path.0 == [0])),
                        "behind object stays at the back"
                    );
                }
                let mut borders = 0;
                for (i, it) in ordered.iter().enumerate() {
                    match it {
                        Placed::Rule { .. } => {
                            borders += 1;
                            assert!(fill < i, "fill must paint below shared table borders");
                        }
                        Placed::Lines { path, .. } if path.0.first() == Some(&1) => assert!(fill < i, "fill must paint below table ink"),
                        _ => {}
                    }
                }
                assert!(borders > 0, "fixture retains shared borders");
            }
        });
    }
}
