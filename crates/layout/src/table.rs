//! Table layout: column widths, cell boxes, row heights, merged cells, borders and shading,
//! table-style conditional formatting. Rows are laid out as free-standing boxes; pagination
//! moves whole rows or splits them between lines (`split_row`).

use wordcraft_doc::numbering::Counters;
use wordcraft_doc::props::{Align, Border, Borders, HeightRule, Rgb, TextDirection, VAlign, VMerge};
use wordcraft_doc::styles::{PartSet, TablePart, TableStyleProps};
use wordcraft_doc::{Block, Blocks, Paragraph, Row, StoryRef, Table};
use wordcraft_geom::Rect;

use crate::para::{CellText, ClKind, ParaLayout};
use crate::{
    BoxContext, BoxLayout, Ctx, Foreground, LineFit, PaintLayer, Placed, extend_foreground, fit_box_tracked, fit_cell_box, layout_box,
    remap_foreground,
};

mod exact_width;
mod incomplete_grid;
mod merge_flow;
mod rounded_width;
pub(super) use merge_flow::MergeFlow;

/// Largest preferred table width, as a share of the text column, that a file can ask for.
const MAX_WIDTH_SHARE: f32 = 10.0;

#[derive(Clone)]
pub struct RowLayout {
    pub height: f32,
    /// Minimum retained on continuation fragments that can hold it.
    pub(crate) min_height: f32,
    /// Items relative to the table's top-left (x) and the row's top (y).
    pub items: Vec<Placed>,
    pub(super) foreground: Vec<Foreground>,
    /// The room the row keeps below its text: the bottom cell margin and border band. Part of a
    /// row split across pages keeps it too.
    pub bottom: f32,
    /// The row's cells: the items each one owns and where its content ends.
    pub cells: Vec<CellPart>,
}

/// An exact nested row's closing rule and the row's top, relative to its containing box.
#[derive(Clone)]
pub(crate) struct AnchoredRule {
    pub item: usize,
    pub top: f32,
}

/// A cell of a [`RowLayout`].
#[derive(Clone)]
pub(crate) struct MergeEdge {
    pub(crate) key: (usize, usize),
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    pub(crate) top: Option<Border>,
    pub(crate) bottom: Option<Border>,
}

#[derive(Clone, Copy, PartialEq)]
enum CenterKind {
    Merge,
    Fragment,
}

#[derive(Clone)]
pub(crate) struct CellCenter {
    kind: CenterKind,
    offset: f32,
    height: f32,
    content: std::ops::Range<usize>,
}

#[derive(Clone)]
pub struct CellPart {
    pub(crate) anchored_rules: Vec<AnchoredRule>,
    /// Index of this cell's closing rule in the row items.
    pub(crate) bottom_rule: Option<usize>,
    /// Reserved closing band for an unmerged, non-exact final row.
    pub(crate) bottom_band: f32,
    /// Top cell margin and visible border band retained on a continuation.
    pub(crate) top: f32,
    /// Source alignment used by the row-wide widow guard.
    pub(crate) valign: VAlign,
    pub(crate) merge: Option<MergeEdge>,
    pub(crate) center: Option<CellCenter>,
    /// Its items in the row's `items` (its fill, text, borders and cell area).
    pub items: std::ops::Range<usize>,
    /// Where its content ends below the row's top, its last paragraph's space after included
    /// (what the part of the row left on a page must hold); `None` for turned text and a cell
    /// covered by a vertical merge.
    pub end: Option<f32>,
}

pub struct TableLayout {
    /// Table x offset within the column.
    pub x: f32,
    pub width: f32,
    pub rows: Vec<RowLayout>,
}

const DEFAULT_MARGINS: [f32; 4] = [0.0, 5.4, 0.0, 5.4];

/// The longest line turned text gets when its row height isn't exact (a Letter page's body
/// height): longer text wraps.
const TURNED_MAX: f32 = 648.0;
/// The longest line turned text ever gets (Word's largest page, 22 inches).
const TURNED_LIMIT: f32 = 1584.0;

/// A cell whose text is turned, between the two passes of [`layout_table`].
struct Turned {
    turn: TextDirection,
    /// Line length it was laid out with, and how far its lines reach across the cell.
    len: f32,
    across: f32,
    /// List counters and equation count before the cell, to lay it out again the same way.
    snap: (Counters, u32),
    /// Its table style parts and cell index in the row.
    region: Region,
    ci: usize,
    /// Top margin plus top border band.
    top: f32,
    /// Top and bottom margins and border bands, independent of text direction.
    insets: f32,
}

/// How long the lines laid out in `items` are without the room they didn't use: the length
/// turned text needs so that nothing wraps.
fn natural_length(items: &[Placed]) -> f32 {
    let mut len = 0.0f32;
    for it in items {
        let Placed::Lines { para, l0, l1, x, .. } = it else { continue };
        let indents = para.rp.indent_left.max(0.0) + para.rp.indent_right.max(0.0) + para.rp.indent_first.max(0.0);
        for l in para.lines.get(*l0..*l1).unwrap_or(&[]) {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for v in &l.vis {
                lo = lo.min(v.x);
                hi = hi.max(v.x + v.w);
            }
            if l.vis.is_empty()
                && let (Some(a), Some(b)) = (l.xs.first(), l.xs.last())
            {
                (lo, hi) = (*a, *b);
            }
            let hy = l.hyphen.map_or(0.0, |h| h.2);
            if hi >= lo {
                len = len.max(x.max(0.0) + hi - lo + hy + indents);
            }
        }
    }
    // A little slack, so laying the text out again at this length doesn't wrap it.
    len + 0.5
}

/// The table's style with its based-on chain merged.
fn table_style(ctx: &Ctx, t: &Table) -> Option<TableStyleProps> {
    ctx.doc.styles.table_style(t.props.style.as_deref()?)
}

/// The table's default cell margins: its own, else its style's, else Word's.
fn default_margins(t: &Table, style: Option<&TableStyleProps>) -> [f32; 4] {
    let defaults = style.and_then(|s| s.parts.cell_margins).unwrap_or(DEFAULT_MARGINS);
    t.props
        .cell_margins
        .unwrap_or_else(|| t.props.partial_cell_margins.map_or(defaults, |sides| std::array::from_fn(|i| sides[i].unwrap_or(defaults[i]))))
}

fn row_margins(row: &wordcraft_doc::Row, defaults: [f32; 4]) -> [f32; 4] {
    let Some(props) = row.props.exceptions.as_ref() else { return defaults };
    props
        .cell_margins
        .unwrap_or_else(|| props.partial_cell_margins.map_or(defaults, |sides| std::array::from_fn(|i| sides[i].unwrap_or(defaults[i]))))
}

/// The margins (top, left, bottom, right; points) of cell `cell` of row `row`: its own, else the
/// row's exceptions, else the table's defaults. The layout uses the left and right ones as they are (the app's ruler
/// measures a cell's indents from them) but gives every cell in a row the row's largest top and
/// bottom margins.
pub fn cell_margins(doc: &wordcraft_doc::Document, t: &Table, row: usize, cell: usize) -> [f32; 4] {
    let style = t.props.style.as_deref().and_then(|id| doc.styles.table_style(id));
    let defaults = default_margins(t, style.as_ref());
    let Some(row) = t.rows.get(row) else { return defaults };
    let defaults = row_margins(row, defaults);
    row.cells.get(cell).map_or(defaults, |cell| cell.props.resolved_margins(defaults))
}

/// The most paragraphs [`measure_table_columns`] lays out: a huge table is measured by its first
/// rows only, so AutoFit stays quick.
const MEASURE_BUDGET: usize = 4000;
/// The line length paragraphs are measured at: wide enough that nothing real wraps.
const MEASURE_WIDE: f32 = 15_840.0;
/// What an empty cell's text needs (about a paragraph mark), points.
const EMPTY_TEXT: f32 = 12.0;

/// What one paragraph needs: the longest run of text without a break opportunity (`min`) and
/// its longest line when nothing wraps (`max`), indents included.
fn measure_para(p: &wordcraft_doc::Paragraph, env: &crate::para::ParaEnv) -> (f32, f32) {
    use crate::para::ClKind;
    let pl = crate::para::layout_para(p, env);
    let rp = &pl.rp;
    let indents = rp.indent_left.max(0.0) + rp.indent_right.max(0.0) + rp.indent_first.max(0.0);
    let (mut word, mut min) = (0.0f32, 0.0f32);
    for c in &pl.clusters {
        match c.kind {
            ClKind::Text | ClKind::Object(_) => word += c.adv.max(0.0),
            ClKind::Marker => {}
            _ => {
                min = min.max(word);
                word = 0.0;
            }
        }
        if c.break_after {
            min = min.max(word);
            word = 0.0;
        }
    }
    min = min.max(word);
    let mut max = 0.0f32;
    for l in &pl.lines {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in l.c0..l.c1 {
            let (Some(c), Some(a), Some(b)) = (pl.clusters.get(k), l.cl_left(k), l.cl_right(k)) else { continue };
            lo = lo.min(a.min(b));
            if !matches!(c.kind, ClKind::Space | ClKind::Marker | ClKind::LineBreak | ClKind::PageBreak | ClKind::ColumnBreak) {
                hi = hi.max(a.max(b));
            }
        }
        if hi > lo {
            max = max.max(hi - lo + l.hyphen.map_or(0.0, |h| h.2));
        }
    }
    let min = if min.is_finite() { min } else { 0.0 };
    let max = if max.is_finite() { max.max(min) } else { min };
    (min + indents, max + indents)
}

/// What each grid column of `t` needs for AutoFit to Contents, as (narrowest, widest) in points
/// with cell margins included: the narrowest fits each cell's longest word (text that can't
/// wrap), the widest fits every paragraph on one line. A cell spanning several columns spreads
/// what its columns lack over them. Paragraphs are laid out with the table style's formatting;
/// after [`MEASURE_BUDGET`] paragraphs the remaining rows are skipped.
pub fn measure_table_columns(doc: &wordcraft_doc::Document, t: &Table) -> Vec<(f32, f32)> {
    use wordcraft_doc::Block;
    let style = t.props.style.as_deref().and_then(|id| doc.styles.table_style(id));
    let margins_def = default_margins(t, style.as_ref());
    let ncols = t.cols().clamp(1, wordcraft_doc::table::MAX_COLS);
    let fields = crate::FieldCtx::default();
    let mut cols = vec![(0.0f32, 0.0f32); ncols];
    let mut measured = vec![false; ncols];
    // Cells spanning several columns: (first column, span, min, max), applied after the others.
    let mut spanned: Vec<(usize, usize, f32, f32)> = Vec::new();
    let mut budget = MEASURE_BUDGET;
    'rows: for (ri, row) in t.rows.iter().enumerate() {
        let margins_def = row_margins(row, margins_def);
        let mut g = row.first_grid();
        for cell in &row.cells {
            let span = cell.span();
            let margins = cell.props.resolved_margins(margins_def);
            let side = margins[1].max(0.0) + margins[3].max(0.0);
            let text = style.as_ref().map(|st| region_text(st, region_of(t, style.as_ref(), ri, g, span, ncols)));
            let env = crate::para::ParaEnv {
                doc,
                width: MEASURE_WIDE,
                label: None,
                fields: &fields,
                show_hidden: false,
                field_codes: false,
                hide_deleted: Default::default(),
                table: text.as_ref(),
                proofing: None,
                // Measurement uses a fixed environment (like show_hidden/proofing above).
                numeral: crate::NumeralMode::default(),
                exclusions: &[],
                eq_number: 0,
                read_line_spacing: None,
                grid: crate::para::Grid::default(),
            };
            let (mut min, mut max) = (EMPTY_TEXT, EMPTY_TEXT);
            if cell.props.vmerge != VMerge::Continue && !cell.props.text_direction.is_turned() {
                for b in &cell.blocks {
                    match &**b {
                        Block::Para(p) => {
                            if budget == 0 {
                                break 'rows;
                            }
                            budget -= 1;
                            let (a, b) = measure_para(p, &env);
                            min = min.max(a);
                            max = max.max(b);
                        }
                        // A nested table keeps its own column widths.
                        Block::Table(inner) => {
                            let w: f32 = inner.grid.iter().filter(|w| w.is_finite()).map(|w| w.clamp(0.0, 1584.0)).sum();
                            min = min.max(w);
                            max = max.max(w);
                        }
                    }
                }
            }
            let (min, max) = ((min + side).min(1584.0), (max + side).min(1584.0));
            if let Some(covered) = measured.get_mut(g..(g + span).min(ncols)) {
                covered.fill(true);
            }
            if span == 1 {
                if let Some(c) = cols.get_mut(g) {
                    c.0 = c.0.max(min);
                    c.1 = c.1.max(max);
                }
            } else {
                spanned.push((g, span, min, max));
            }
            g += span;
        }
        spanned.extend(row_gaps(row, g).map(|(g, span, w)| (g, span, w, w)));
    }
    for (g, span, min, max) in spanned {
        let Some(cs) = cols.get_mut(g..(g + span).min(ncols)) else { continue };
        if cs.is_empty() {
            continue;
        }
        let n = cs.len() as f32;
        let (have_min, have_max) = cs.iter().fold((0.0, 0.0), |(a, b), c| (a + c.0, b + c.1));
        for c in cs.iter_mut() {
            c.0 += ((min - have_min) / n).max(0.0);
            c.1 += ((max - have_max) / n).max(0.0);
            c.1 = c.1.max(c.0);
        }
    }
    // Columns outside completed cell measurements retain the default margin floor.
    let floor = margins_def[1].max(0.0) + margins_def[3].max(0.0) + EMPTY_TEXT;
    for (column, measured) in cols.iter_mut().zip(measured) {
        if !measured {
            column.0 = column.0.max(floor);
        }
        column.1 = column.1.max(column.0);
    }
    cols
}

/// Column widths for AutoFit to Contents, the way Word shares the room: every column gets its
/// widest content when that all fits in `avail`; otherwise each gets its narrowest plus a share
/// of the remaining room in proportion to how much wider its content would like to be. When
/// even the narrowest widths don't fit, the columns keep them (the table runs past the margin,
/// as in Word).
pub fn autofit_widths(cols: &[(f32, f32)], avail: f32) -> Vec<f32> {
    let clean = |v: f32| if v.is_finite() { v.clamp(0.0, 1584.0) } else { 0.0 };
    let cols: Vec<(f32, f32)> = cols.iter().map(|&(a, b)| (clean(a), clean(b).max(clean(a)))).collect();
    let avail = clean(avail);
    let (sum_min, sum_max) = cols.iter().fold((0.0f32, 0.0f32), |(a, b), c| (a + c.0, b + c.1));
    if sum_max <= avail {
        return cols.iter().map(|c| c.1).collect();
    }
    if sum_min >= avail || sum_max <= sum_min {
        return cols.iter().map(|c| c.0).collect();
    }
    let k = (avail - sum_min) / (sum_max - sum_min);
    cols.iter().map(|c| c.0 + (c.1 - c.0) * k).collect()
}

/// The first cell's left margin: how far its text sits inside the table's edge.
pub(crate) fn first_cell_left_margin(ctx: &Ctx, t: &Table) -> f32 {
    first_cell_left_margin_in(t, default_margins(t, table_style(ctx, t).as_ref()))
}

fn first_cell_left_margin_in(t: &Table, def: [f32; 4]) -> f32 {
    let Some(row) = t.rows.first() else { return def[1] };
    let def = row_margins(row, def);
    row.cells.first().map_or(def, |cell| cell.props.resolved_margins(def))[1]
}

/// What a table style gives the cells in one set of parts: shading, vertical alignment and each
/// part's cell borders, lowest precedence first.
struct RegionLook {
    fill: Option<Rgb>,
    v_align: Option<VAlign>,
    borders: Vec<(TablePart, Borders)>,
}

fn region_look(st: &TableStyleProps, region: Region) -> RegionLook {
    let mut out = RegionLook { fill: None, v_align: None, borders: Vec::new() };
    for part in region.iter() {
        let f = st.part(part);
        out.fill = f.fill.or(out.fill);
        out.v_align = f.v_align.or(out.v_align);
        // The whole table's borders are the table borders, applied on their own.
        if part != TablePart::WholeTable
            && let Some(b) = f.borders
        {
            out.borders.push((part, b));
        }
    }
    out
}

/// A cell's borders from the table style's parts it is in, later parts winning. Row parts give
/// their left/right edges at the table's sides and `inside_v` between cells; column parts their
/// top/bottom edges at the table's top and bottom and `between` between rows; corner cells their
/// four edges.
fn region_borders(parts: &[(TablePart, Borders)], (ri, nrows): (usize, usize), (first_g, last_g): (bool, bool)) -> Borders {
    let mut out = Borders::default();
    for (part, b) in parts {
        let b = *b;
        let mapped = if part.is_row() {
            Borders {
                top: b.top,
                bottom: b.bottom,
                left: if first_g { b.left } else { b.inside_v },
                right: if last_g { b.right } else { b.inside_v },
                between: None,
                inside_v: None,
            }
        } else if part.is_column() {
            Borders {
                top: if ri == 0 { b.top } else { b.between },
                bottom: if ri + 1 >= nrows { b.bottom } else { b.between },
                left: b.left,
                right: b.right,
                between: None,
                inside_v: None,
            }
        } else {
            Borders { between: None, inside_v: None, ..b }
        };
        out.overlay(&mapped);
    }
    out
}

pub fn layout_table(ctx: &mut Ctx, story: StoryRef, t: &Table, path: &[u32], avail: f32, depth: usize) -> TableLayout {
    let style = table_style(ctx, t);
    let prepared = exact_width::prepare(t, style.as_ref());
    let t = prepared.as_ref();
    // Cell text formatting and looks per set of style parts, built once per table.
    let mut cell_text: Vec<(Region, CellText)> = Vec::new();
    let mut looks: Vec<(Region, RegionLook)> = Vec::new();
    let ncols = t.cols().max(1);
    // Column widths.
    let mut grid: Vec<f32> = if let Some(grid) = incomplete_grid::recover(t) {
        grid
    } else if t.grid.len() == ncols {
        // Grid units can be narrower than a text box, including omitted row-end columns.
        t.grid
            .iter()
            .map(|&w| match w {
                w if !w.is_finite() => 72.0,
                w if w <= 0.0 => 4.0,
                w => w,
            })
            .collect()
    } else {
        vec![avail / ncols as f32; ncols]
    };
    apply_cell_widths(t, &mut grid);
    let mut total: f32 = grid.iter().sum();
    // A top-level table in an inherited header or footer takes its share of the defining
    // section's width (`Ctx::pct_base`).
    let pct_base = if depth <= 1 { ctx.pct_base.unwrap_or(avail) } else { avail };
    let target = match (t.props.width_pct, t.props.width) {
        // Word keeps a percentage past 100%; the cap only bounds hostile input.
        (Some(p), _) if p > 0.0 => Some(pct_base * (p / 100.0).min(MAX_WIDTH_SHARE)),
        (_, Some(w)) if w > 0.0 => Some(w),
        _ => None,
    };
    if let Some(tw) = target {
        if total > 0.0 && total < 1.0 {
            // Divide first: a tiny positive total can overflow the scale factor.
            grid.iter_mut().for_each(|w| *w = (*w / total) * tw);
        } else {
            let k = tw / total.max(1.0);
            grid.iter_mut().for_each(|w| *w *= k);
        }
        total = tw;
    }
    if total > avail * 1.5 && !t.props.fixed {
        let k = avail / total.max(1.0);
        grid.iter_mut().for_each(|w| *w *= k);
        total = avail;
    }
    if !t.props.fixed {
        // Measuring lays paragraphs out once more; it mustn't count their equations twice.
        let eq_count = ctx.eq_count;
        let content = column_content(ctx, t, style.as_ref(), depth, &grid);
        ctx.eq_count = eq_count;
        if let Some(content) = content {
            autofit(&mut grid, &content, t, avail);
            // Never wider than the room it had, however long an unbreakable word.
            let fitted: f32 = grid.iter().sum();
            let cap = avail.max(total);
            if fitted > cap {
                grid.iter_mut().for_each(|w| *w *= cap / fitted);
            }
            total = grid.iter().sum();
        }
    }
    let mut colx = Vec::with_capacity(ncols + 1);
    let mut acc = 0.0;
    for w in &grid {
        colx.push(acc);
        acc += w;
    }
    colx.push(acc);
    // Word 2013 and later (compatibility mode 15) put the table's border at the margin (plus its
    // indent); earlier modes line the first cell's text up with it instead.
    let margins_def = default_margins(t, style.as_ref());
    let indent = t.props.indent.unwrap_or(0.0);
    // The style's borders, overlaid by the table's own side by side.
    let mut tborders = style.as_ref().and_then(|s| s.parts.borders).unwrap_or_default();
    if let Some(own) = t.props.borders {
        tborders.overlay(&own);
    }
    let first_cell = t.rows.first().and_then(|r| r.cells.first());
    // A right-to-left table (`w:bidiVisual`) mirrors: its first column stands at the right, its
    // start alignment and indent are measured from the right margin, and each cell's start
    // (left) margin and border are on its right.
    let rtl = t.props.bidi_visual;
    // How far in from the start margin the table's edge sits when it is start-aligned.
    let start_offset = if ctx.doc.settings.compat_mode >= 15 {
        // The border is centred on the edge, so half of it sits outside: Word moves the table in
        // by that half.
        let border = first_cell.and_then(|c| c.props.borders.and_then(|b| b.left)).or(tborders.left);
        indent + border.filter(Border::is_visible).map_or(0.0, |b| b.width.clamp(0.0, 12.0) / 2.0)
    } else {
        indent - first_cell_left_margin_in(t, margins_def)
    };
    let x = match (t.props.align, rtl) {
        (Some(Align::Center), _) => (avail - total) / 2.0,
        // End alignment: the right margin, or the left one in a right-to-left table.
        (Some(Align::Right), false) => avail - total,
        (Some(Align::Right), true) => 0.0,
        (_, true) => avail - total - start_offset,
        (_, false) => start_offset,
    };
    let nrows = t.rows.len();
    // First pass: lay out every cell's content.
    struct CellBox {
        items: Vec<Placed>,
        anchored_rules: Vec<AnchoredRule>,
        foreground: Vec<Foreground>,
        h: f32,
        x: f32,
        w: f32,
        valign: VAlign,
        fill: Option<Rgb>,
        borders: Borders,
        top_band: f32,
        vmerge: VMerge,
        margins: [f32; 4],
        g: usize,
        span: usize,
        /// Turned text: `items` are still in the turned frame (placed in the second pass).
        turned: Option<Turned>,
        /// Where the content ends below the cell's top (before vertical alignment).
        end: f32,
    }
    let mut rows: Vec<Vec<CellBox>> = Vec::with_capacity(nrows);
    let mut heights: Vec<f32> = Vec::with_capacity(nrows);
    let mut bottoms: Vec<f32> = Vec::with_capacity(nrows);
    let mut minimums = Vec::with_capacity(nrows);
    for (ri, row) in t.rows.iter().enumerate() {
        let margins_def = row_margins(row, margins_def);
        let mut g = row.first_grid();
        let mut cells = Vec::with_capacity(row.cells.len());
        let mut rh = 0.0f32;
        let mut row_top_band = 0.0f32;
        let mut mark_height: Option<f32> = None;
        // The tallest cell margins plus border bands in the row.
        let mut row_insets = 0.0f32;
        let mut row_below = 0.0f32;
        // Every cell an empty one with a hidden end-of-cell mark: the row is only its borders.
        let mut collapsed = true;
        // Word gives every cell in a row the row's largest top and bottom margins: a cell without
        // its own lines up with a neighbour that has 5.65pt ones.
        let (row_top, row_bottom) = row
            .cells
            .iter()
            .map(|c| c.props.resolved_margins(margins_def))
            .fold((0.0f32, 0.0f32), |(top, bottom), m| (top.max(m[0]), bottom.max(m[2])));
        for (ci, cell) in row.cells.iter().enumerate() {
            let span = cell.span();
            let x0 = colx.get(g).copied().unwrap_or(acc);
            let x1 = colx.get((g + span).min(ncols)).copied().unwrap_or(acc);
            // Right to left: grid columns run from the right edge, and the start (left) margin
            // is on the cell's right.
            let (x0, x1) = if rtl { ((acc - x1).max(0.0), (acc - x0).max(0.0)) } else { (x0, x1) };
            let m = cell.props.resolved_margins(margins_def);
            let mut margins = if rtl { [row_top, m[3], row_bottom, m[1]] } else { [row_top, m[1], row_bottom, m[3]] };
            let region = region_of(t, style.as_ref(), ri, g, span, ncols);
            // Conditional formatting, later parts winning (ECMA-376 §17.7.6): whole table, column
            // bands, row bands, first/last column, header/total row, corner cells.
            if let Some(st) = &style
                && !cell_text.iter().any(|(r, _)| *r == region)
            {
                cell_text.push((region, region_text(st, region)));
                looks.push((region, region_look(st, region)));
            }
            let look = looks.iter().find(|(r, _)| *r == region).map(|(_, l)| l);
            let fill = cell.props.shading.or(look.and_then(|l| l.fill));
            let text = cell_text.iter().find(|(r, _)| *r == region).map(|(_, c)| c);
            let mut cpath = path.to_vec();
            cpath.push(ri as u32);
            cpath.push(ci as u32);
            // Effective borders: cell > table (outer vs inside).
            let tb = tborders;
            let edge =
                |own: Option<Border>, outer: bool, outer_b: Option<Border>, inner_b: Option<Border>| own.or(if outer { outer_b } else { inner_b });
            let cb = cell.props.borders.unwrap_or_default();
            // A row's table property exceptions (`w:tblPrEx`) override the table's borders for its cells.
            let row_borders = row.props.exceptions.as_ref().and_then(|e| e.borders).unwrap_or_default();
            // A row that skips grid columns (`w:gridBefore`/`w:gridAfter`) has its edge at its own
            // first and last cell.
            let (first_g, last_g) = (g == row.first_grid(), g + span >= ncols.saturating_sub(row.props.grid_after as usize));
            let rb = look.map(|l| region_borders(&l.borders, (ri, nrows), (first_g, last_g))).unwrap_or_default();
            let mut borders = Borders {
                top: edge(cb.top.or(if ri == 0 { row_borders.top } else { row_borders.between }).or(rb.top), ri == 0, tb.top, tb.between),
                bottom: edge(
                    cb.bottom.or(if ri + 1 == nrows { row_borders.bottom } else { row_borders.between }).or(rb.bottom),
                    ri + 1 == nrows,
                    tb.bottom,
                    tb.between,
                ),
                left: edge(
                    cb.left.or(if first_g { row_borders.left } else { row_borders.inside_v }).or(if first_g { rb.left } else { rb.inside_v }),
                    first_g,
                    tb.left,
                    tb.inside_v,
                ),
                right: edge(
                    cb.right.or(if last_g { row_borders.right } else { row_borders.inside_v }).or(if last_g { rb.right } else { rb.inside_v }),
                    last_g,
                    tb.right,
                    tb.inside_v,
                ),
                between: None,
                inside_v: None,
            };
            // The logical start (left) and end (right) borders, drawn mirrored in an RTL table.
            if rtl {
                std::mem::swap(&mut borders.left, &mut borders.right);
            }
            // A zero-margin fixed cell still clears the inward half of each side rule.
            // Tabbed exact rows keep their text frame: another line would be clipped.
            let tabbed_exact = row.props.height_rule == HeightRule::Exact
                && cell.blocks.iter().any(|block| matches!(&**block, Block::Para(p) if p.text.contains('\t')));
            if t.props.fixed && !rtl && !cell.props.text_direction.is_turned() && !tabbed_exact {
                for (side, border) in [(1, borders.left), (3, borders.right)] {
                    if margins[side] == 0.0 {
                        margins[side] = border.filter(Border::is_visible).map_or(0.0, |b| b.width.clamp(0.0, 12.0) / 2.0);
                    }
                }
            }
            let cw = (x1 - x0 - margins[1] - margins[3]).max(4.0);
            // Word keeps the cell's text clear of its top border (and the last row's of its bottom
            // border): the border's width sits on top of the cell margin.
            let band = |b: Option<Border>| b.filter(Border::is_visible).map_or(0.0, |b| b.width.clamp(0.0, 12.0));
            let (band_t, band_b) = (band(borders.top), if ri + 1 == nrows { band(borders.bottom) } else { 0.0 });
            // A shared edge painted by the row above still clears this cell's text and minimum.
            let band_t = if band_t > 0.0 || cell.props.vmerge == VMerge::Continue {
                band_t
            } else {
                rows.last()
                    .into_iter()
                    .flatten()
                    .filter(|above| above.g < g + span && g < above.g + above.span)
                    .filter(|above| {
                        above.vmerge == VMerge::None
                            || !t.cell_at_grid(ri, above.g).is_some_and(|ci| {
                                t.grid_col(ri, ci) == above.g && row.cells.get(ci).is_some_and(|c| c.props.vmerge == VMerge::Continue)
                            })
                    })
                    .map(|above| band(above.borders.bottom))
                    .fold(0.0, f32::max)
            };
            let insets = margins[0] + margins[2] + band_t + band_b;
            let turn = cell.props.text_direction;
            let mut turned = None;
            let mut anchored_rules = Vec::new();
            let (items, foreground, h) = if cell.props.vmerge == VMerge::Continue {
                (Vec::new(), Vec::new(), 0.0)
            } else if turn.is_turned() {
                collapsed = false;
                // Turned text: its lines run along the cell's height. An exact row height sets
                // their length. A minimum height also wraps text; overflowing columns are
                // clipped rather than making the row grow to the string's advance.
                let snap = (ctx.counters.clone(), ctx.eq_count);
                let preferred = row.props.height.filter(|h| h.is_finite() && *h > 0.0).map(|h| h.clamp(4.0, TURNED_LIMIT));
                let wrapped = preferred.filter(|_| row.props.height_rule == HeightRule::AtLeast);
                let len = match (row.props.height, row.props.height_rule, wrapped) {
                    (Some(h), HeightRule::Exact, _) if h > 0.0 => (h + row_bottom - insets).clamp(4.0, TURNED_LIMIT),
                    (_, _, Some(h)) => h,
                    _ => {
                        let probe = layout_box(ctx, story, &cell.blocks, &cpath, TURNED_MAX, BoxContext::Cell(text), depth, None).items;
                        (ctx.counters, ctx.eq_count) = (snap.0.clone(), snap.1);
                        natural_length(&probe).clamp(4.0, TURNED_MAX)
                    }
                };
                let BoxLayout { items, foreground, height, floats_bottom, .. } =
                    layout_box(ctx, story, &cell.blocks, &cpath, len, BoxContext::Cell(text), depth, None);
                let across = height.max(floats_bottom);
                turned = Some(Turned { turn, len, across, snap, region, ci, top: margins[0] + band_t, insets });
                // Automatic rows take the unturned line height, not the string's advance.
                // The second pass wraps at the final row height, even if columns overflow.
                let h = if preferred.is_none() { across } else { len };
                (items, foreground, h)
            } else {
                let BoxLayout { mut items, foreground, mut height, floats_bottom, last_top, anchored_rules: mut rules, .. } =
                    layout_box(ctx, story, &cell.blocks, &cpath, cw, BoxContext::Cell(text), depth, None);
                // A hidden end-of-cell mark: an empty last paragraph takes no room (Word for
                // Windows: a row of empty such cells is only its borders).
                if cell.props.hide_mark && cell.blocks.last().and_then(|b| b.as_para()).is_some_and(|p| p.text.is_empty()) {
                    height = last_top;
                } else {
                    collapsed = false;
                }
                for it in &mut items {
                    it.translate(x0 + margins[1], margins[0] + band_t);
                }
                for rule in &mut rules {
                    rule.top += margins[0] + band_t;
                }
                anchored_rules = rules;
                // The cell grows to hold its floating tables.
                (items, foreground, height.max(floats_bottom))
            };
            let h = h + insets;
            let end = h - margins[2] - band_b;
            if cell.props.vmerge != VMerge::Restart {
                rh = rh.max(h);
            }
            if cell.props.vmerge == VMerge::None
                && !cell.props.hide_mark
                && !turn.is_turned()
                && cell.blocks.len() == 1
                && let Some(Placed::Lines { para, .. }) = items.first()
                && items.iter().all(|it| matches!(it, Placed::Lines { para, .. } if para.text_len == 0 && para.label.is_none()))
            {
                // Paragraph spacing already contributes to the cell's own height.
                let strut = para.height + margins[0] + margins[2] + band_b;
                mark_height = Some(mark_height.unwrap_or(0.0).max(strut));
            }
            row_top_band = row_top_band.max(band_t);
            row_insets = row_insets.max(insets);
            row_below = row_below.max(margins[2] + band_b);
            cells.push(CellBox {
                items,
                anchored_rules,
                foreground,
                h,
                x: x0,
                w: x1 - x0,
                // The style's vertical alignment unless the cell has its own.
                // The cell's own alignment, even Top, beats the table style's.
                valign: cell.props.valign.or_else(|| look.and_then(|l| l.v_align)).unwrap_or_default(),
                fill,
                borders,
                top_band: band_t,
                vmerge: cell.props.vmerge,
                margins,
                g,
                span,
                turned,
                end,
            });
            g += span;
        }
        // Word adds the cells' top and bottom margins and border bands to an at-least row height:
        // a 30pt row with 5pt margins and 0.5pt borders is 40.5pt tall even when its text needs less.
        // An exact height gains only the bottom margin: it runs from the top border to the bottom
        // of the text area (a 35pt row with 4pt margins steps 39pt).
        // An empty cell strut clears the row's largest top border band. Nonempty
        // content keeps its own border inset, even beside a thicker border.
        let rh = mark_height.map_or(rh, |h| rh.max(h + row_top_band));
        let rh = match (row.props.height, row.props.height_rule) {
            (Some(h), HeightRule::Exact) if h > 0.0 => h + row_bottom,
            (Some(h), _) if h > 0.0 => rh.max(h + row_insets),
            _ => rh,
        };
        minimums.push(match (row.props.height, row.props.height_rule) {
            (Some(h), HeightRule::AtLeast) if h > 0.0 => h + row_insets,
            _ => 0.0,
        });
        bottoms.push(row_below);
        heights.push(if collapsed && !cells.is_empty() { rh.max(0.0) } else { rh.max(4.0) });
        rows.push(cells);
    }
    // Vertically merged cells: grow the last row of the merge if the content is taller.
    for ri in 0..rows.len() {
        let restarts: Vec<(usize, f32, usize)> =
            rows.get(ri).map(|r| r.iter().filter(|c| c.vmerge == VMerge::Restart).map(|c| (c.g, c.h, c.span)).collect()).unwrap_or_default();
        for (g, h, _) in restarts {
            let mut end = ri;
            while rows.get(end + 1).is_some_and(|r| r.iter().any(|c| c.g == g && c.vmerge == VMerge::Continue)) {
                end += 1;
            }
            // Exact rows clip a merged cell's overflow instead of moving the following content.
            if t.rows
                .get(ri..=end)
                .is_some_and(|rows| rows.iter().all(|row| row.props.height_rule == HeightRule::Exact && row.props.height.is_some_and(|h| h > 0.0)))
            {
                continue;
            }
            let have: f32 = heights.get(ri..=end).map(|s| s.iter().sum()).unwrap_or(0.0);
            if h > have
                && let Some(last) = heights.get_mut(end)
            {
                *last += h - have;
            }
        }
    }
    // Grid columns holding a vertical-merge continuation, per row.
    let cont: Vec<Vec<usize>> = t
        .rows
        .iter()
        .map(|r| {
            let mut gx = r.first_grid();
            r.cells
                .iter()
                .filter_map(|x| {
                    let g = gx;
                    gx += x.span();
                    (x.props.vmerge == VMerge::Continue).then_some(g)
                })
                .collect()
        })
        .collect();
    // Second pass: place cells with their row height, fills and borders.
    let mut out = Vec::with_capacity(rows.len());
    let mut merge_starts: std::collections::HashMap<usize, (usize, usize)> = std::collections::HashMap::new();
    for (ri, cells) in rows.into_iter().enumerate() {
        let rh = heights.get(ri).copied().unwrap_or(0.0);
        let exact = t.rows.get(ri).is_some_and(|r| r.props.height_rule == HeightRule::Exact && r.props.height.is_some_and(|h| h > 0.0));
        let merged = cells.iter().any(|c| c.vmerge != VMerge::None);
        let mut items = Vec::new();
        let mut parts = Vec::with_capacity(cells.len());
        let mut foreground = Vec::new();
        for (ci, c) in cells.into_iter().enumerate() {
            let first = items.len();
            // A restart cell spans several rows: its height is their sum.
            let mut span_h = rh;
            if c.vmerge == VMerge::Restart {
                let mut k = ri + 1;
                while cont.get(k).is_some_and(|v| v.contains(&c.g)) {
                    span_h += heights.get(k).copied().unwrap_or(0.0);
                    k += 1;
                }
            }
            let rect = Rect::new(c.x, 0.0, c.w, span_h);
            if c.vmerge != VMerge::Continue
                && let Some(f) = c.fill
            {
                let start = items.len();
                items.push(Placed::Fill { rect, color: f });
                foreground.push(Foreground { range: start..items.len(), layer: PaintLayer::CellFill });
            }
            // Behind-text frames count towards vertical alignment even without inline content.
            // They do not increase the row's flow height.
            let align_h = c
                .items
                .iter()
                .filter_map(|item| match item {
                    Placed::Object { rect, wrap: wordcraft_doc::para::Wrap::BehindText, .. } => Some(rect.bottom()),
                    _ => None,
                })
                .fold(c.h, f32::max);
            let mut dy = match c.valign {
                VAlign::Top => 0.0,
                VAlign::Center => ((span_h - align_h) / 2.0).max(0.0),
                VAlign::Bottom => (span_h - align_h).max(0.0),
            };
            let end = (c.vmerge != VMerge::Continue && c.turned.is_none()).then_some(c.end + dy);
            let mut cell_items = c.items;
            let mut cell_foreground = c.foreground;
            let mut anchored_rules = c.anchored_rules;
            let turned_clip = c.turned.is_some();
            if let Some(tn) = c.turned {
                dy = 0.0;
                let m = c.margins;
                // The lines run the cell's whole height (laid out again if the row grew).
                let len = (span_h - tn.insets).clamp(4.0, TURNED_LIMIT);
                let mut across = tn.across;
                if (len - tn.len).abs() > 0.01
                    && let Some(cell) = t.rows.get(ri).and_then(|r| r.cells.get(tn.ci))
                {
                    let mut cpath = path.to_vec();
                    cpath.extend([ri as u32, tn.ci as u32]);
                    let text = cell_text.iter().find(|(r, _)| *r == tn.region).map(|(_, c)| c);
                    // Same list numbers as the first layout: lay it out from the same counters.
                    let now = (std::mem::replace(&mut ctx.counters, tn.snap.0), std::mem::replace(&mut ctx.eq_count, tn.snap.1));
                    let b = layout_box(ctx, story, &cell.blocks, &cpath, len, BoxContext::Cell(text), depth, None);
                    (cell_items, cell_foreground, across) = (b.items, b.foreground, b.height.max(b.floats_bottom));
                    (ctx.counters, ctx.eq_count) = now;
                }
                // Lines stack across the cell from its start edge (right for top-to-bottom text,
                // left for bottom-to-top); the vertical alignment moves them across it.
                let cw = (c.w - m[1] - m[3]).max(0.0);
                let off = match c.valign {
                    VAlign::Top => 0.0,
                    VAlign::Center => ((cw - across) / 2.0).max(0.0),
                    VAlign::Bottom => (cw - across).max(0.0),
                };
                let (ox, oy) = match tn.turn {
                    TextDirection::Up => (c.x + m[1] + off, tn.top + len),
                    _ => (c.x + m[1] + cw - off, tn.top),
                };
                for it in &mut cell_items {
                    it.turn(tn.turn, ox, oy);
                }
            } else if exact {
                // Exact cells clip ink at the row edge, including partial overflow lines.
                if cell_foreground.is_empty() && anchored_rules.is_empty() {
                    cell_items = fit_cell_box(cell_items, span_h - c.margins[2], story);
                } else {
                    let mut map = vec![0..0; cell_items.len()];
                    cell_items = fit_box_tracked(cell_items, span_h - c.margins[2], LineFit::Clipped(story), |i, range| {
                        if let Some(slot) = map.get_mut(i) {
                            *slot = range;
                        }
                    });
                    cell_foreground = remap_foreground(&cell_foreground, &map);
                    anchored_rules = anchored_rules
                        .into_iter()
                        .filter_map(|r| {
                            let range = map.get(r.item)?;
                            (!range.is_empty()).then_some(AnchoredRule { item: range.start, top: r.top })
                        })
                        .collect();
                }
            }
            let content_start = items.len();
            for rule in &mut anchored_rules {
                rule.item += content_start;
                rule.top += dy;
            }
            extend_foreground(&mut foreground, &cell_foreground, content_start);
            for mut it in cell_items {
                it.translate(0.0, dy);
                items.push(it);
            }
            // Only plain paragraphs and inline pictures can move as one unmerged group.
            let fragment_flow = t.rows.get(ri).and_then(|r| r.cells.get(ci)).is_some_and(|cell| {
                !cell.props.hide_mark
                    && cell.blocks.iter().all(|block| {
                        block.as_para().is_some_and(|p| {
                            p.objects.iter().all(|object| {
                                matches!(object,
                        wordcraft_doc::para::InlineObject::Image { float, .. }
                        if float.wrap == wordcraft_doc::para::Wrap::Inline && float.spin().is_identity())
                            })
                        })
                    })
            });
            let center =
                (c.vmerge != VMerge::Continue && c.valign == VAlign::Center && !exact && dy > 0.0 && (c.vmerge == VMerge::Restart || fragment_flow))
                    .then_some(CellCenter {
                        kind: if c.vmerge == VMerge::Restart { CenterKind::Merge } else { CenterKind::Fragment },
                        offset: dy,
                        height: align_h,
                        content: content_start..items.len(),
                    });
            let b = c.borders;
            let (x0, x1, y0, y1) = (c.x, c.x + c.w, 0.0, rh);
            // Horizontal edges run on over half of each visible vertical edge, so the corners
            // close the way Word draws them instead of leaving a notch at thick borders.
            let half = |o: Option<Border>| o.filter(Border::is_visible).map_or(0.0, |e| e.width / 2.0);
            let (hx0, hx1) = (x0 - half(b.left), x1 + half(b.right));
            let bottom_band = if ri + 1 == nrows && !exact && !merged {
                b.bottom.filter(Border::is_visible).map_or(0.0, |b| b.width.clamp(0.0, 12.0))
            } else {
                0.0
            };
            let merge = if c.vmerge == VMerge::None {
                merge_starts.remove(&c.g);
                None
            } else {
                let start = if c.vmerge == VMerge::Restart {
                    ri
                } else {
                    merge_starts.get(&c.g).filter(|(_, last)| last.checked_add(1) == Some(ri)).map_or(ri, |(start, _)| *start)
                };
                merge_starts.insert(c.g, (start, ri));
                Some(MergeEdge {
                    key: (start, c.g),
                    x0: hx0,
                    x1: hx1,
                    top: b.top.filter(Border::is_visible),
                    bottom: b.bottom.filter(Border::is_visible),
                })
            };
            let top_vis = c.vmerge != VMerge::Continue;
            if top_vis && let Some(e) = b.top.filter(Border::is_visible) {
                items.push(Placed::Rule { x0: hx0, y0, x1: hx1, y1: y0, border: e });
            }
            let mut bottom_rule = None;
            let last_of_merge = c.vmerge == VMerge::None || !cont.get(ri + 1).is_some_and(|v| v.contains(&c.g));
            if last_of_merge && let Some(e) = b.bottom.filter(Border::is_visible) {
                bottom_rule = Some(items.len());
                let bottom_y = y1 - bottom_band / 2.0;
                items.push(Placed::Rule { x0: hx0, y0: bottom_y, x1: hx1, y1: bottom_y, border: e });
            }
            if let Some(e) = b.left.filter(Border::is_visible) {
                items.push(Placed::Rule { x0, y0, x1: x0, y1, border: e });
            }
            if let Some(e) = b.right.filter(Border::is_visible) {
                items.push(Placed::Rule { x0: x1, y0, x1, y1, border: e });
            }
            items.push(Placed::Cell {
                rect: Rect::new(c.x, 0.0, c.w, if c.vmerge == VMerge::Continue { rh } else { span_h }),
                covered: c.vmerge == VMerge::Continue,
                table: wordcraft_doc::Path(path.to_vec()),
                row: ri,
                cell: ci,
                story,
                clip: turned_clip || exact && c.h > span_h + 0.01,
            });
            let top = c.margins[0] + c.top_band;
            parts.push(CellPart { anchored_rules, bottom_rule, top, bottom_band, valign: c.valign, items: first..items.len(), end, center, merge });
        }
        out.push(RowLayout {
            height: rh,
            min_height: minimums.get(ri).copied().unwrap_or(0.0),
            items,
            foreground,
            bottom: bottoms.get(ri).copied().unwrap_or(0.0),
            cells: parts,
        });
    }
    TableLayout { x, width: total, rows: out }
}

// Attribute only the horizontal push_para prefix. Clipped side rules are not owned.
// A following flow item separates the run from ambiguous cell/merge closing edges.
#[derive(Clone, Copy, PartialEq)]
enum ParaEdge {
    Top,
    Bottom,
}

const PARA_RULE_LIMIT: usize = 4;

fn para_rule_owner(items: &[Placed], index: usize) -> Option<(usize, ParaEdge)> {
    for distance in 1..=PARA_RULE_LIMIT {
        let Some(owner) = index.checked_sub(distance) else { break };
        let Some(Placed::Lines { para, l0, l1, x, y, turn: TextDirection::Horizontal, .. }) = items.get(owner) else { continue };
        if l0 >= l1 {
            continue;
        }
        let (Some(first), Some(last), Some(borders)) =
            (para.lines.get(*l0), l1.checked_sub(1).and_then(|k| para.lines.get(k)), para.rp.borders.as_ref())
        else {
            continue;
        };
        if [borders.top, borders.bottom, borders.left, borders.right].iter().flatten().any(|b| !b.space.is_finite() || b.space < 0.0) {
            continue;
        }
        let top = borders.top.filter(|b| *l0 == 0 && b.is_visible());
        let bottom = borders.bottom.filter(|b| *l1 == para.lines.len() && b.is_visible());
        if top.is_none() && bottom.is_none() {
            continue;
        }
        // The emitter does not store width on Lines. Its first horizontal rule supplies rx;
        // horizontal centerlines, endpoints and their ordered styles must agree.
        let Some(Placed::Rule { x1: rx, .. }) = items.get(owner + 1) else { continue };
        let start = para.rp.indent_left.min(para.rp.indent_left + para.rp.indent_first);
        let left = x + if para.rp.bidi { para.rp.indent_right } else { start };
        let lx = left - borders.left.map_or(0.0, |b| b.space);
        let ty = y - borders.top.map_or(0.0, |b| b.space);
        let by = y + last.top + last.height - first.top + borders.bottom.map_or(0.0, |b| b.space);
        if ![lx, *rx, ty, by].iter().all(|v| v.is_finite()) || *rx <= lx {
            continue;
        }
        let expected = [(top, (lx, ty, *rx, ty), ParaEdge::Top), (bottom, (lx, by, *rx, by), ParaEdge::Bottom)];
        let mut next = owner + 1;
        let mut found = None;
        let mut valid = true;
        for (border, (ex0, ey0, ex1, ey1), edge) in expected {
            let Some(border) = border else { continue };
            let Some(Placed::Rule { x0, y0, x1, y1, border: actual }) = items.get(next) else {
                valid = false;
                break;
            };
            if *actual != border || ![(*x0, ex0), (*y0, ey0), (*x1, ex1), (*y1, ey1)].iter().all(|(a, b)| (a - b).abs() < 0.01) {
                valid = false;
                break;
            }
            if next == index {
                found = Some((owner, edge));
            }
            next += 1;
        }
        // Side rules may have been clipped or omitted by an earlier split. Recognize only
        // their style and horizontal position to find the boundary, never own or rebuild them.
        for (border, x) in [(borders.left.filter(Border::is_visible), lx), (borders.right.filter(Border::is_visible), *rx)] {
            if let Some(border) = border
                && let Some(Placed::Rule { x0, y0, x1, y1, border: actual }) = items.get(next)
                && *actual == border
                && (*x0 - x).abs() < 0.01
                && (*x1 - x).abs() < 0.01
                && y0.is_finite()
                && y1.is_finite()
                && (y0 - y1).abs() >= 0.01
            {
                next += 1;
            }
        }
        // Do not consume a possible cell/merge closing edge substituted for a missing rule.
        let flow_follows = matches!(
            items.get(next),
            Some(
                Placed::Lines { .. }
                    | Placed::Image { .. }
                    | Placed::Shape { .. }
                    | Placed::Graphic { .. }
                    | Placed::Clip { .. }
                    | Placed::Object { .. }
            )
        );
        if valid && flow_follows && found.is_some() {
            return found;
        }
    }
    None
}

// Crossing-cut and out-of-flow bottom rules retain their existing routing/overflow.
fn split_para_rule(items: &[Placed], splits: &[usize], index: usize, flow_bottom: f32, limit: f32) -> Option<(usize, ParaEdge)> {
    let (owner, edge) = para_rule_owner(items, index)?;
    let Placed::Lines { para, l0, l1, y, .. } = items.get(owner)? else { return None };
    let split = *splits.get(owner)?;
    if let Some(bottom) = para.rp.borders.as_ref().and_then(|b| b.bottom).filter(Border::is_visible) {
        let extent = crate::item_bottom(*y, para, *l0, *l1)? + bottom.space;
        if extent < 0.0 || extent > flow_bottom || ((edge == ParaEdge::Bottom || split == *l1) && extent > limit) {
            return None;
        }
    }
    Some((owner, edge))
}

/// Split a row at `cut` (points below the row's top) so the part above fits on this page.
/// Paragraph lines break between lines; fills, vertical rules and cell areas are cut; the
/// rest moves up to the top of the continuation. `None` when no line fits above the cut.
///
/// Like Word, the part left on the page keeps the cells' bottom margin and border below its
/// text and a cell's last paragraph its space after (a row only that much taller than the room
/// left splits rather than moving on whole), and widow control keeps two lines of a paragraph on
/// either side of the break. Each cell's text goes on at the top of the continuation, and the
/// continuation is as tall as the cell needing the most room, and at least the rest of the row
/// (a minimum height holds over the pages).
pub fn split_row(row: &RowLayout, cut: f32) -> Option<(RowLayout, RowLayout)> {
    if !row.cells.iter().any(|c| c.center.as_ref().is_some_and(|v| v.kind == CenterKind::Fragment)) {
        return split_row_inner(row, cut);
    }
    // Ordinary centered cells use normal paragraph ownership unless their whole group fits.
    let mut adjusted = row.clone();
    for cell in &mut adjusted.cells {
        if cell.center.as_ref().is_some_and(|v| v.kind == CenterKind::Fragment) {
            cell.center = None;
        }
    }
    let baseline = split_row_inner(&adjusted, cut)?;
    if row.cells.iter().any(|c| c.center.as_ref().is_some_and(|v| v.kind == CenterKind::Merge)) {
        return Some(baseline);
    }
    let first_height = baseline.0.height;
    let selected: Vec<bool> = row
        .cells
        .iter()
        .map(|cell| {
            cell.center.as_ref().is_some_and(|center| {
                center.kind == CenterKind::Fragment
                    && center.height <= first_height + 0.01
                    && row.items.get(center.content.clone()).is_some_and(|items| {
                        !items.is_empty()
                            && items.iter().all(|item| match item {
                                Placed::Lines { para, .. } => item.turned_bounds().is_none() && para.rp.borders.is_none(),
                                Placed::Object { story, path, off, wrap: wordcraft_doc::para::Wrap::Inline, text_box: None, spin, .. } => {
                                    spin.is_identity()
                                        && items.iter().any(|owner| match owner {
                                            Placed::Lines { story: s, path: p, para, l0, l1, .. } if s == story && p == path => {
                                                para.lines.get(*l0..*l1).is_some_and(|lines| {
                                                    lines.iter().any(|line| {
                                                        para.clusters.get(line.c0..line.c1).is_some_and(|clusters| {
                                                            clusters.iter().any(|c| c.start == *off && matches!(c.kind, ClKind::Object(_)))
                                                        })
                                                    })
                                                })
                                            }
                                            _ => false,
                                        })
                                }
                                Placed::Image { spin, .. } => spin.is_identity(),
                                _ => false,
                            })
                    })
            })
        })
        .collect();
    let picture = row.cells.iter().zip(&selected).any(|(cell, selected)| {
        *selected
            && cell
                .center
                .as_ref()
                .and_then(|v| row.items.get(v.content.clone()))
                .is_some_and(|items| items.iter().any(|item| matches!(item, Placed::Image { .. } | Placed::Object { .. })))
    });
    if !picture {
        return Some(baseline);
    }
    // A sibling must establish both the first fragment and a real continuation.
    // This leaves ambiguous empty or structural-only splits on the existing path.
    let real_text = |item: &Placed| text_lines(item).is_some_and(|(p, l0, l1, _)| l1 > l0 && (p.text_len > 0 || p.label.is_some()));
    let same_para = |source: &Placed, placed: &Placed| match (source, placed) {
        (Placed::Lines { story, path, para, .. }, Placed::Lines { story: s, path: p, para: q, .. }) => {
            story == s && path == p && std::sync::Arc::ptr_eq(para, q)
        }
        _ => false,
    };
    let retained = row.cells.iter().zip(&selected).any(|(cell, selected)| {
        !selected
            && row
                .items
                .get(cell.items.clone())
                .is_some_and(|items| items.iter().any(|source| baseline.0.items.iter().any(|placed| real_text(placed) && same_para(source, placed))))
    });
    let remainder = baseline
        .1
        .cells
        .iter()
        .zip(&selected)
        .any(|(cell, selected)| !selected && baseline.1.items.get(cell.items.clone()).is_some_and(|items| items.iter().any(real_text)));
    if !retained || !remainder {
        return Some(baseline);
    }
    for (cell, selected) in row.cells.iter().zip(&selected) {
        if !selected {
            continue;
        }
        let Some(center) = &cell.center else { continue };
        let shift = ((first_height - center.height) / 2.0).max(0.0) - center.offset;
        if let Some(items) = adjusted.items.get_mut(center.content.clone()) {
            for item in items {
                item.translate(0.0, shift);
            }
        }
    }
    for ((cell, source), selected) in adjusted.cells.iter_mut().zip(&row.cells).zip(&selected) {
        if *selected && let Some(center) = &source.center {
            let shift = ((first_height - center.height) / 2.0).max(0.0) - center.offset;
            cell.end = cell.end.map(|end| end + shift);
        }
    }
    // Recompute line ownership, border routing and continuation sizing after translation.
    let Some(candidate) = split_row_inner(&adjusted, cut) else { return Some(baseline) };
    if (candidate.0.height - first_height).abs() > 0.01 {
        return Some(baseline);
    }
    // Every selected line must remain whole, with its inline hit area on the same page.
    for (cell, selected) in row.cells.iter().zip(&selected) {
        if !selected {
            continue;
        }
        let Some(center) = &cell.center else { continue };
        for source in row.items.get(center.content.clone()).unwrap_or(&[]) {
            let present = candidate.0.items.iter().any(|placed| match (source, placed) {
                (Placed::Lines { l0, l1, .. }, Placed::Lines { l0: a, l1: b, .. }) => same_para(source, placed) && l0 == a && l1 == b,
                (Placed::Object { story, path, off, .. }, Placed::Object { story: s, path: p, off: o, rect, .. }) => {
                    story == s && path == p && off == o && rect.y >= 0.0 && rect.bottom() <= first_height - row.bottom + 0.01
                }
                (Placed::Image { media, story, path, off, .. }, Placed::Image { media: m, story: s, path: p, off: o, .. }) => {
                    media == m && story == s && path == p && off == o
                }
                _ => false,
            });
            if !present || candidate.1.items.iter().any(|placed| same_para(source, placed)) {
                return Some(baseline);
            }
        }
    }
    Some(candidate)
}

fn split_row_inner(row: &RowLayout, cut: f32) -> Option<(RowLayout, RowLayout)> {
    // Also rejects a NaN cut.
    if !(cut > 0.0 && cut < row.height) {
        return None;
    }
    let limit = cut - row.bottom.clamp(0.0, cut);
    let fits = |bottom: f32| bottom <= limit + 0.01;
    // Each cell's items: every item of a row is in one of its cells.
    let cells: Vec<&[Placed]> = row.cells.iter().map(|c| row.items.get(c.items.clone()).unwrap_or(&[])).collect();
    // A nested table's cell is deeper than the row's own.
    let own_depth = row.items.iter().filter_map(|it| if let Placed::Cell { table, .. } = it { Some(table.0.len()) } else { None }).min();
    // The first line of each paragraph that moves on (`l1` when all of them stay), by cell and
    // item.
    let splits: Vec<Vec<usize>> = cells
        .iter()
        .zip(&row.cells)
        .map(|(items, c)| {
            // Where the cell's text ends: the paragraph ending lowest is the cell's last.
            let lowest =
                items.iter().filter_map(|it| text_lines(it).and_then(|(p, l0, l1, y)| crate::item_bottom(y, p, l0, l1))).fold(f32::MIN, f32::max);
            items
                .iter()
                .map(|it| {
                    let Some((para, l0, l1, y)) = text_lines(it) else { return 0 };
                    // The cell's last paragraph ends with the cell's content (its space after).
                    let last = l1 == para.lines.len() && crate::item_bottom(y, para, l0, l1).is_some_and(|b| b >= lowest - 0.01);
                    line_split(para, (l0, l1), y, limit, c.end.filter(|_| last))
                })
                .collect()
        })
        .collect();
    // A top-aligned cell that widow/orphan control leaves with nothing on this page, though its first line
    // would fit there, takes the whole row on with it: Word doesn't leave the other cells behind
    // beside an empty one (observed with synthetic documents). Trailing space alone can move
    // the last paragraph without deferring the whole row.
    for ((items, splits), cell) in cells.iter().zip(&splits).zip(&row.cells) {
        let first = items.iter().zip(splits).find_map(|(it, &split)| text_lines(it).map(|t| (t, split)));
        if let Some(((para, l0, l1, y), split)) = first
            && cell.valign == VAlign::Top
            && split == l0
            && para.rp.widow_control
            && l1.saturating_sub(l0) > 1
            && crate::item_bottom(y, para, l0, l1).is_some_and(|bottom| !fits(bottom))
            && para.lines.get(l0).is_some_and(|l| fits(y + l.top - para.lines.get(l0).map_or(0.0, |f| f.top) + l.height))
        {
            return None;
        }
    }
    // The top of the first thing that moves on in each cell.
    let mut kept_any = false;
    let mut moved_content = false;
    let mut moved = vec![f32::MAX; cells.len()];
    for ((items, splits), m) in cells.iter().zip(&splits).zip(&mut moved) {
        let multiple_paras = items.iter().filter(|it| text_lines(it).is_some()).take(2).count() > 1;
        for (it, &split) in items.iter().zip(splits) {
            let top = if let Some(r) = it.turned_bounds() {
                // Turned text moves as a whole, like a picture.
                if fits(r.bottom()) {
                    kept_any = true;
                    None
                } else {
                    Some(r.y)
                }
            } else {
                match it {
                    Placed::Lines { para, l0, l1, y, .. } => {
                        kept_any |= split > *l0;
                        let base = para.lines.get(*l0).map(|l| l.top).unwrap_or(0.0);
                        (split < *l1).then(|| para.lines.get(split).map_or(*y, |l| y + l.top - base))
                    }
                    Placed::Image { rect, .. } | Placed::Shape { rect, .. } | Placed::Graphic { rect, .. } | Placed::Clip { rect, .. } => {
                        if fits(rect.bottom()) {
                            kept_any = true;
                            None
                        } else {
                            Some(rect.y)
                        }
                    }
                    // A nested table's cell the cut runs through goes on from the cut, so what is
                    // below the cut keeps its size and the text after it stays below it (a
                    // floating table's fixed-height row in an otherwise fitting cell splits the
                    // row there). A row that is just taller than its text (a minimum height)
                    // moves on whole, as in Word.
                    Placed::Cell { rect, table, .. } if Some(table.0.len()) > own_depth && rect.y < cut && rect.bottom() > cut + 0.01 => Some(cut),
                    _ => None,
                }
            };
            if let Some(top) = top {
                let empty_mark = matches!(it, Placed::Lines { para, .. }
                if !multiple_paras && para.text_len == 0 && para.label.is_none()
                && para.rp.borders.is_none_or(|b| {
                    ![b.top, b.bottom, b.left, b.right, b.between].into_iter().flatten().any(|b| b.is_visible())
                }));
                moved_content |= !empty_mark;
                *m = m.min(top);
            }
        }
    }
    let first_moved = moved.iter().copied().fold(f32::MAX, f32::min);
    // Single empty cell marks alone do not create a continuation fragment.
    if !kept_any || !moved_content || first_moved == f32::MAX {
        return None;
    }
    // A whole moved paragraph and its top border share one cell shift, including border space.
    for (((items, splits), c), anchor) in cells.iter().zip(&splits).zip(&row.cells).zip(&mut moved) {
        if c.center.is_some() {
            continue;
        }
        for (index, it) in items.iter().enumerate() {
            if let Some((owner, ParaEdge::Top)) = split_para_rule(items, splits, index, row.height - row.bottom, limit)
                && let Some(Placed::Lines { l0, .. }) = items.get(owner)
                && splits.get(owner) == Some(l0)
                && let Placed::Rule { y0, .. } = it
            {
                *anchor = anchor.min(*y0);
            }
        }
    }
    // Word closes the first fragment below its last fitted content.
    // Nested boxes crossing the cut retain their geometry.
    let mut kept_bottoms = Vec::with_capacity(cells.len());
    for (((items, splits), c), &moved_top) in cells.iter().zip(&splits).zip(&row.cells).zip(&moved) {
        let mut kept_bottom = 0.0f32;
        for (it, &split) in items.iter().zip(splits) {
            let bottom = if let Some(r) = it.turned_bounds() {
                fits(r.bottom()).then_some(r.bottom())
            } else {
                match it {
                    Placed::Lines { para, l0, y, .. } if split > *l0 => crate::item_bottom(*y, para, *l0, split),
                    Placed::Image { rect, .. } | Placed::Shape { rect, .. } | Placed::Graphic { rect, .. } | Placed::Clip { rect, .. }
                        if fits(rect.bottom()) =>
                    {
                        Some(rect.bottom())
                    }
                    Placed::Cell { rect, table, .. } if Some(table.0.len()) > own_depth && rect.y < cut => Some(rect.bottom().min(cut)),
                    _ => None,
                }
            };
            if let Some(bottom) = bottom {
                kept_bottom = kept_bottom.max(bottom);
            }
        }
        if c.center.is_none() {
            for (index, it) in items.iter().enumerate() {
                if let Some((owner, ParaEdge::Bottom)) = split_para_rule(items, splits, index, row.height - row.bottom, limit)
                    && let Some(Placed::Lines { l1, .. }) = items.get(owner)
                    && splits.get(owner) == Some(l1)
                    && let Placed::Rule { y0, .. } = it
                {
                    kept_bottom = kept_bottom.max(*y0);
                }
            }
        }
        // Generic retained rules bound the frame; metadata-owned rules keep their routes.
        for (index, it) in items.iter().enumerate() {
            if let Placed::Rule { x0, y0, x1, y1, .. } = it
                && [x0, y0, x1, y1].iter().all(|value| value.is_finite())
                && (y0 - y1).abs() < 0.01
                && *y0 > 0.01
                && *y0 < row.height - 0.01
                && *y0 <= cut
            {
                let source = c.items.start.saturating_add(index);
                if c.bottom_rule == Some(source)
                    || c.anchored_rules.iter().any(|rule| rule.item == source)
                    || (c.center.is_none() && split_para_rule(items, splits, index, row.height - row.bottom, limit).is_some())
                {
                    continue;
                }
                kept_bottom = kept_bottom.max(*y0);
            }
        }
        if moved_top == f32::MAX
            && let Some(end) = c.end.filter(|e| fits(*e))
        {
            kept_bottom = kept_bottom.max(end);
        }
        kept_bottoms.push(kept_bottom);
    }
    let floor = if row.min_height <= cut + 0.01 { row.min_height } else { cut };
    let plain_bottom = kept_bottoms.iter().zip(&row.cells).filter(|(_, c)| c.center.is_none()).map(|(b, _)| *b).fold(0.0f32, f32::max);
    let plain_height = (plain_bottom + row.bottom).max(floor).min(cut);
    // A fitting merged label uses the first physical fragment's vertical center.
    let kept_bottom = kept_bottoms
        .iter()
        .zip(&row.cells)
        .filter(|(_, c)| c.center.as_ref().is_none_or(|v| v.height > plain_height + 0.01))
        .map(|(b, _)| *b)
        .fold(0.0f32, f32::max);
    let first_height = (kept_bottom + row.bottom).max(floor).min(cut);
    let trim_first = |r: Rect| (r.y < first_height).then(|| Rect::new(r.x, r.y, r.w, r.h.min(first_height - r.y)));
    // Each cell resumes below its own top margin and border band.
    let shifts: Vec<f32> = moved.iter().zip(&row.cells).map(|(&m, c)| ((if m < f32::MAX { m } else { first_moved }) - c.top).max(0.0)).collect();
    // The continuation is as tall as the cell needing the most room (what it moves on and the
    // bottom margin, the rest of the row for turned text), and at least the rest of the row.
    let full = (row.height - shifts.iter().copied().fold(f32::MAX, f32::min)).max(4.0);
    let mut need = row.height - cut;
    for ((c, &m), &s) in row.cells.iter().zip(&moved).zip(&shifts) {
        if m < f32::MAX {
            need = need.max(c.end.map_or(row.height, |e| e + row.bottom) - s);
        }
    }
    // Word reapplies an at-least height on the continuation. Oversized minimums
    // that cannot fit the first fragment keep their existing distributed height.
    let min_height = if row.min_height <= cut + 0.01 { row.min_height } else { 0.0 };
    let height = need.clamp(4.0, full).max(min_height);
    // What runs to the row's bottom (cell areas, shading, side borders) ends at the
    // continuation's.
    let rest = (row.height - height).max(0.0);
    let geo_shift = |bottom: f32, shift: f32| if bottom >= row.height - 0.01 { rest } else { shift };
    let (mut a, mut b) = (Vec::new(), Vec::new());
    let map_len = if row.foreground.is_empty() { 0 } else { row.items.len() };
    let (mut map_a, mut map_b) = (vec![0..0; map_len], vec![0..0; map_len]);
    let mut parts = Vec::with_capacity(cells.len());
    for (((c, items), splits), &shift) in row.cells.iter().zip(&cells).zip(&splits).zip(&shifts) {
        let start = b.len();
        let mut bottom_rule = None;
        let mut anchored_rules = Vec::new();
        let center = c.center.as_ref().filter(|v| v.height <= plain_height + 0.01);
        for (index, (it, &split)) in items.iter().zip(splits).enumerate() {
            let source = c.items.start.saturating_add(index);
            let (start_a, start_b) = (a.len(), b.len());
            if let Some(center) = center
                && center.content.contains(&(c.items.start + index))
            {
                let mut it = it.clone();
                it.translate(0.0, ((first_height - center.height) / 2.0).max(0.0) - center.offset);
                a.push(it);
                if let Some(r) = map_a.get_mut(source) {
                    *r = start_a..a.len();
                }
                if let Some(r) = map_b.get_mut(source) {
                    *r = start_b..b.len();
                }
                continue;
            }
            if let Some(r) = it.turned_bounds() {
                let mut it = it.clone();
                if fits(r.bottom()) {
                    a.push(it);
                } else {
                    it.translate(0.0, -shift);
                    b.push(it);
                }
                if let Some(r) = map_a.get_mut(source) {
                    *r = start_a..a.len();
                }
                if let Some(r) = map_b.get_mut(source) {
                    *r = start_b..b.len();
                }
                continue;
            }
            match it {
                Placed::Lines { story, path, para, l0, l1, x, y, turn } => {
                    let base = para.lines.get(*l0).map(|l| l.top).unwrap_or(0.0);
                    if split > *l0 {
                        a.push(Placed::Lines {
                            story: *story,
                            path: path.clone(),
                            para: para.clone(),
                            l0: *l0,
                            l1: split,
                            x: *x,
                            y: *y,
                            turn: *turn,
                        });
                    }
                    if split < *l1 {
                        let top = para.lines.get(split).map(|l| y + l.top - base).unwrap_or(*y);
                        b.push(Placed::Lines {
                            story: *story,
                            path: path.clone(),
                            para: para.clone(),
                            l0: split,
                            l1: *l1,
                            x: *x,
                            y: top - shift,
                            turn: *turn,
                        });
                    }
                }
                Placed::Image { rect, .. }
                | Placed::Shape { rect, .. }
                | Placed::Graphic { rect, .. }
                | Placed::Clip { rect, .. }
                | Placed::Spun { rect, .. }
                | Placed::Object { rect, .. } => {
                    if fits(rect.bottom()) {
                        a.push(it.clone());
                    } else {
                        let mut it = it.clone();
                        it.translate(0.0, -shift);
                        b.push(it);
                    }
                }
                // Shading and cell areas: the part above the cut stays, the rest moves up with the
                // continuation (a nested table's cell below the cut moves whole).
                Placed::Fill { rect, color } => {
                    let (above, below) = split_rect(*rect, cut, geo_shift(rect.bottom(), shift));
                    a.extend(above.and_then(trim_first).map(|rect| Placed::Fill { rect, color: *color }));
                    b.extend(below.map(|rect| Placed::Fill { rect, color: *color }));
                }
                Placed::Cell { rect, covered, table, row: r, cell, story, clip } => {
                    let (above, below) = split_rect(*rect, cut, geo_shift(rect.bottom(), shift));
                    let cell_at =
                        |rect| Placed::Cell { rect, covered: *covered, table: table.clone(), row: *r, cell: *cell, story: *story, clip: *clip };
                    a.extend(above.and_then(trim_first).map(cell_at));
                    b.extend(below.map(cell_at));
                }
                Placed::Rule { x0, y0, x1, y1, border } => {
                    if c.center.is_none()
                        && let Some((owner, edge)) = split_para_rule(items, splits, index, row.height - row.bottom, limit)
                        && let Some(Placed::Lines { l0, l1, .. }) = items.get(owner)
                    {
                        let stays = match edge {
                            ParaEdge::Top => splits.get(owner).is_some_and(|split| split > l0),
                            ParaEdge::Bottom => splits.get(owner) == Some(l1),
                        };
                        if !stays {
                            let mut rule = it.clone();
                            rule.translate(0.0, -shift);
                            b.push(rule);
                        } else {
                            a.push(it.clone());
                        }
                        if let Some(r) = map_a.get_mut(source) {
                            *r = start_a..a.len();
                        }
                        if let Some(r) = map_b.get_mut(source) {
                            *r = start_b..b.len();
                        }
                        continue;
                    }
                    if (y0 - y1).abs() < 0.01 {
                        // Horizontal: the top edge stays, the bottom edge moves; both fragments close.
                        if let Some(rule) = c.anchored_rules.iter().find(|r| r.item == source) {
                            if rule.top < cut {
                                a.push(it.clone());
                            } else {
                                anchored_rules.push(AnchoredRule { item: b.len(), top: rule.top - shift });
                                b.push(Placed::Rule { x0: *x0, y0: y0 - shift, x1: *x1, y1: y1 - shift, border: *border });
                            }
                        } else if c.bottom_rule == Some(source) {
                            // Keep the entire closing stroke inside each physical fragment's band.
                            let above = first_height - c.bottom_band / 2.0;
                            let below = if c.bottom_band > 0.0 { height - c.bottom_band / 2.0 } else { height };
                            a.push(Placed::Rule { x0: *x0, y0: above, x1: *x1, y1: above, border: *border });
                            bottom_rule = Some(b.len());
                            b.push(Placed::Rule { x0: *x0, y0: below, x1: *x1, y1: below, border: *border });
                        } else if *y0 <= 0.01 {
                            a.push(it.clone());
                            b.push(Placed::Rule { x0: *x0, y0: 0.0, x1: *x1, y1: 0.0, border: *border });
                        } else if *y0 <= cut {
                            a.push(it.clone());
                        } else {
                            b.push(Placed::Rule { x0: *x0, y0: y0 - shift, x1: *x1, y1: y1 - shift, border: *border });
                        }
                    } else {
                        let (t, bt) = (y0.min(*y1), y0.max(*y1));
                        if t < first_height {
                            a.push(Placed::Rule { x0: *x0, y0: t, x1: *x1, y1: bt.min(first_height), border: *border });
                        }
                        if bt > cut {
                            let shift = geo_shift(bt, shift);
                            b.push(Placed::Rule { x0: *x0, y0: (t - shift).max(0.0), x1: *x1, y1: bt - shift, border: *border });
                        }
                    }
                }
            }
            if let Some(r) = map_a.get_mut(source) {
                *r = start_a..a.len();
            }
            if let Some(r) = map_b.get_mut(source) {
                *r = start_b..b.len();
            }
        }
        parts.push(CellPart {
            anchored_rules,
            bottom_rule,
            bottom_band: c.bottom_band,
            top: c.top,
            valign: c.valign,
            items: start..b.len(),
            end: c.end.filter(|_| center.is_none()).map(|e| e - shift),
            center: None,
            merge: None,
        });
    }
    // The part left on the page is placed and never split again.
    Some((
        RowLayout {
            height: first_height,
            min_height: 0.0,
            items: a,
            foreground: remap_foreground(&row.foreground, &map_a),
            bottom: row.bottom,
            cells: Vec::new(),
        },
        RowLayout { height, min_height, items: b, foreground: remap_foreground(&row.foreground, &map_b), bottom: row.bottom, cells: parts },
    ))
}

/// A cell paragraph's placed lines (not turned text): the paragraph, its lines `l0..l1` and the
/// top of line `l0`.
fn text_lines(it: &Placed) -> Option<(&ParaLayout, usize, usize, f32)> {
    match it {
        Placed::Lines { para, l0, l1, y, .. } if it.turned_bounds().is_none() => Some((para, *l0, *l1, *y)),
        _ => None,
    }
}

/// The first of lines `l0..l1` of `para` (placed with line `l0`'s top at `y`) that does not fit
/// above `limit`: the last line of a cell's last paragraph needs room down to the cell's content
/// `end`, its space after. Widow control keeps two lines on either side of the break, or moves
/// the paragraph on whole.
fn line_split(para: &ParaLayout, (l0, l1): (usize, usize), y: f32, limit: f32, end: Option<f32>) -> usize {
    let n = para.lines.len();
    let mut k = l0;
    while k < l1 {
        let Some(bottom) = crate::item_bottom(y, para, l0, k + 1) else { break };
        let bottom = if k + 1 == n { end.map_or(bottom, |e| e.max(bottom)) } else { bottom };
        if bottom > limit + 0.01 {
            break;
        }
        k += 1;
    }
    // The body's rule differs: there a widow takes a line along only when three or more lines are
    // on the page (`place_para_in_flow`).
    if para.rp.widow_control && k > l0 && k < l1 {
        // A widow: take one more line along.
        if n - k == 1 {
            k -= 1;
        }
        // An orphan (the first line alone): the paragraph moves on whole.
        if l0 == 0 && k == 1 {
            k = 0;
        }
    }
    k
}

/// The parts of `r` above `cut` and below it, the latter moved up by `shift` (onto the
/// continuation of a row split at `cut`).
fn split_rect(r: Rect, cut: f32, shift: f32) -> (Option<Rect>, Option<Rect>) {
    let above = (r.y < cut).then(|| Rect::new(r.x, r.y, r.w, (r.bottom().min(cut) - r.y).max(0.0)));
    let top = (r.y - shift).max(0.0);
    let below = (r.bottom() > cut).then(|| Rect::new(r.x, top, r.w, (r.bottom() - shift - top).max(0.0)));
    (above, below)
}

/// A cell's table style parts.
type Region = PartSet;

/// The parts of the cell at grid column `g` spanning `span` of `ncols` columns in row `ri`.
fn region_of(t: &Table, style: Option<&TableStyleProps>, ri: usize, g: usize, span: usize, ncols: usize) -> Region {
    let size = style.and_then(|s| s.parts.band_size).unwrap_or(1);
    PartSet::of_cell(&t.props.look, size, (ri, t.rows.len()), (g, span, ncols))
}

/// The table style's text formatting in `region`: each part's character formatting and paragraph
/// alignment over the last, lowest precedence first (ECMA-376 §17.7.6).
fn region_text(st: &TableStyleProps, region: Region) -> CellText {
    let mut chr = wordcraft_doc::CharProps::default();
    let mut para = st.para.clone();
    for part in region.iter() {
        let f = st.part(part);
        chr.overlay(&f.chr);
        if f.align.is_some() {
            para.align = f.align;
        }
    }
    CellText { para, chr }
}

/// Each column's narrowest and widest content, cell margins included: its longest word and its
/// longest line. A cell spanning columns shares what they lack between them; nested tables count
/// with their own. `None` when every cell is empty.
fn column_content(ctx: &mut Ctx, t: &Table, style: Option<&TableStyleProps>, depth: usize, grid: &[f32]) -> Option<Vec<(f32, f32)>> {
    let margins_def = default_margins(t, style);
    let mut cols = vec![(0.0f32, 0.0f32); t.cols().max(1)];
    let mut spanning = Vec::new();
    let mut any = false;
    for (ri, row) in t.rows.iter().enumerate() {
        let margins_def = row_margins(row, margins_def);
        let mut g = row.first_grid();
        for cell in &row.cells {
            let span = cell.span();
            if cell.props.vmerge != VMerge::Continue {
                let text = style.map(|st| region_text(st, region_of(t, style, ri, g, cell.span(), cols.len())));
                let (lo, hi) = blocks_content(ctx, &cell.blocks, text.as_ref(), depth);
                any |= hi > 0.0;
                let m = cell.props.resolved_margins(margins_def);
                let need = (lo + m[1] + m[3], hi + m[1] + m[3]);
                match cols.get_mut(g) {
                    Some(col) if span == 1 => *col = (col.0.max(need.0), col.1.max(need.1)),
                    _ => spanning.push((g, span, need)),
                }
            }
            g += span;
        }
        spanning.extend(row_gaps(row, g).map(|(g, span, w)| (g, span, (w, w))));
    }
    for (g, span, need) in spanning {
        let Some(cs) = cols.get_mut(g..(g + span).min(t.cols())) else { continue };
        let n = cs.len().max(1) as f32;
        let have_hi: f32 = cs.iter().map(|c| c.1).sum();
        let preferred = (!is_automatic(t)).then(|| grid.get(g..g + cs.len())).flatten();
        share_span_min(cs, preferred, need.0);
        for c in cs {
            c.1 += (need.1 - have_hi).max(0.0) / n;
        }
    }
    any.then_some(cols)
}

// Share a span's extra minimum by unused preferred width, so a fitting span never
// widens a column past its preference. Automatic tables share the extra equally.
fn share_span_min(cols: &mut [(f32, f32)], grid: Option<&[f32]>, need: f32) {
    let extra = (need - cols.iter().map(|c| c.0).sum::<f32>()).max(0.0);
    let weight = |w: f32, min: f32| if w.is_finite() { (w.clamp(0.0, UNBOUNDED) - min).max(0.0) } else { 0.0 };
    let total = grid.map_or(0.0, |grid| grid.iter().zip(cols.iter()).map(|(w, c)| weight(*w, c.0)).sum::<f32>());
    if total > 0.0 {
        if let Some(grid) = grid {
            for (c, w) in cols.iter_mut().zip(grid) {
                c.0 += extra * weight(*w, c.0) / total;
            }
        }
        return;
    }
    let share = extra / cols.len().max(1) as f32;
    for c in cols {
        c.0 += share;
    }
}

/// The grid columns row `row` leaves empty before its first cell and after its last one (which
/// ends at grid column `end`) when it gives their width (`w:wBefore`, `w:wAfter`): (first column,
/// columns, width in points). Column widths make room for them like for an empty cell.
fn row_gaps(row: &Row, end: usize) -> impl Iterator<Item = (usize, usize, f32)> {
    let p = &row.props;
    [(0, row.first_grid(), p.w_before), (end, p.grid_after as usize, p.w_after)]
        .into_iter()
        .filter(|&(_, n, _)| n > 0)
        .filter_map(|(g, n, w)| w.filter(|w| w.is_finite() && *w > 0.0).map(|w| (g, n, w.min(1584.0))))
}

/// The narrowest and widest a block list can be laid out.
fn blocks_content(ctx: &mut Ctx, blocks: &Blocks, text: Option<&CellText>, depth: usize) -> (f32, f32) {
    let (mut lo, mut hi) = (0.0f32, 0.0f32);
    for b in blocks.iter() {
        let (l, h) = match &**b {
            Block::Para(p) => {
                let label = list_level(ctx, p).map(|l| (String::new(), l));
                ctx.para_measured(p, UNBOUNDED, text, label, para_content)
            }
            // A nested table: its fixed width, else its columns' but at least its preferred width
            // (its own, else its grid's when it has preferred widths at all).
            Block::Table(nt) if depth < 8 => {
                let style = table_style(ctx, nt);
                let prepared = exact_width::prepare(nt, style.as_ref());
                let nt = prepared.as_ref();
                let grid = if is_automatic(nt) { 0.0 } else { nt.grid.iter().filter(|w| w.is_finite()).map(|w| w.max(0.0)).sum() };
                let width = nt.props.width.filter(|w| w.is_finite()).unwrap_or(grid).clamp(0.0, UNBOUNDED);
                if nt.props.fixed && width > 0.0 {
                    (width, width)
                } else {
                    let cols = column_content(ctx, nt, style.as_ref(), depth + 1, &nt.grid).unwrap_or_default();
                    let (l, h) = cols.iter().fold((0.0, 0.0), |(l, h), c| (l + c.0, h + c.1));
                    (width.max(l), width.max(h))
                }
            }
            Block::Table(_) => (0.0, 0.0),
        };
        lo = lo.max(l);
        hi = hi.max(h);
    }
    (lo, hi)
}

/// The list level `p` is in, for its indents (without counting it: measuring isn't laying out).
fn list_level(ctx: &Ctx, p: &Paragraph) -> Option<wordcraft_doc::numbering::Level> {
    let n = p.props.numbering.or_else(|| ctx.doc.styles.resolve_para(&p.props).numbering).filter(|n| n.num != 0)?;
    ctx.doc.numbering.level(n.num, n.level).cloned()
}

/// A width to lay a paragraph out in so that only its own line breaks end lines.
const UNBOUNDED: f32 = 100_000.0;

/// A paragraph's longest word and longest line (between line breaks), indents included.
fn para_content(pl: &ParaLayout) -> (f32, f32) {
    let indents = pl.rp.indent_left.max(0.0) + pl.rp.indent_right.max(0.0) + pl.rp.indent_first.max(0.0);
    let (mut word, mut line, mut spaces) = (0.0f32, 0.0f32, 0.0f32);
    let (mut lo, mut hi) = (0.0f32, 0.0f32);
    for c in &pl.clusters {
        let adv = if c.adv.is_finite() { c.adv.clamp(0.0, UNBOUNDED) } else { 0.0 };
        match c.kind {
            ClKind::Space => {
                lo = lo.max(word);
                word = 0.0;
                spaces += adv;
            }
            ClKind::LineBreak | ClKind::PageBreak | ClKind::ColumnBreak => {
                lo = lo.max(word);
                hi = hi.max(line);
                (word, line, spaces) = (0.0, 0.0, 0.0);
            }
            _ => {
                word += adv;
                line += spaces + adv;
                spaces = 0.0;
                if c.break_after {
                    lo = lo.max(word);
                    word = 0.0;
                }
            }
        }
    }
    (lo.max(word) + indents, hi.max(line) + indents)
}

/// Whether nothing in `t` asks for a width (its own or any cell's): its columns then follow
/// their content alone.
fn is_automatic(t: &Table) -> bool {
    t.props.width.is_none()
        && t.props.width_pct.is_none()
        && t.rows.iter().all(|r| r.cells.iter().all(|c| c.props.width.is_none() && c.props.width_pct.is_none()))
}

// Complete point-width rows can retain their proportions after a percentage table is resized.
fn point_row_total(t: &Table, columns: usize) -> Option<f32> {
    if t.props.fixed || !t.props.width_pct.is_some_and(|p| p.is_finite() && p > 0.0) {
        return None;
    }
    for row in &t.rows {
        if row.first_grid() != 0
            || row.props.grid_after != 0
            || row.cells.iter().fold(0usize, |n, cell| n.saturating_add(cell.span())) != columns
            || row.cells.iter().any(|cell| cell.props.width_pct.is_some() || !cell.props.width.is_some_and(|w| w.is_finite() && w > 0.0))
        {
            return None;
        }
    }
    let total: f32 = t.rows.first()?.cells.iter().filter_map(|cell| cell.props.width).sum();
    (total.is_finite() && total > 0.0).then_some(total)
}

// Complete point rows constrain internal boundaries. A full-grid header adds no interior edge.
fn title_point_widths(t: &Table, grid: &[f32]) -> Option<Vec<f32>> {
    const WIDTH_EPSILON: f32 = 0.01;
    const MIN_COMPLETE_ROWS: usize = 2;
    if t.props.fixed || t.props.width_pct.is_some() || grid.is_empty() {
        return None;
    }
    let target = t.props.width.filter(|w| w.is_finite() && *w > 0.0)?;
    if grid.iter().any(|w| !w.is_finite() || *w <= 0.0) {
        return None;
    }
    let grid_total: f32 = grid.iter().sum();
    if !grid_total.is_finite() || (target - grid_total).abs() > WIDTH_EPSILON {
        return None;
    }
    let headers = t.rows.iter().take_while(|row| row.props.header).count();
    if headers == 0 {
        return None;
    }
    let mut widths: Option<Vec<f32>> = None;
    let mut complete = 0usize;
    for row in &t.rows {
        if row.first_grid() != 0 || row.props.grid_after != 0 {
            return None;
        }
        let mut covered = 0usize;
        for cell in &row.cells {
            if cell.props.width_pct.is_some() || !cell.props.width.is_some_and(|w| w.is_finite() && w > 0.0) {
                return None;
            }
            covered = covered.checked_add(cell.span())?;
        }
        if covered != grid.len() {
            return None;
        }
        if row.cells.len() != grid.len() || row.cells.iter().any(|cell| cell.span() != 1) {
            continue;
        }
        let candidate: Vec<_> = row.cells.iter().filter_map(|cell| cell.props.width).collect();
        if let Some(previous) = &widths {
            if previous.iter().zip(&candidate).any(|(a, b)| (a - b).abs() > WIDTH_EPSILON) {
                return None;
            }
        } else {
            widths = Some(candidate);
        }
        complete += 1;
    }
    if complete < MIN_COMPLETE_ROWS {
        return None;
    }
    let widths = widths?;
    let total: f32 = widths.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let mut undersized_title = false;
    for (index, row) in t.rows.iter().enumerate() {
        let mut start = 0usize;
        let mut declared = 0.0f32;
        for cell in &row.cells {
            let end = start.checked_add(cell.span())?;
            let expected: f32 = widths.get(start..end)?.iter().sum();
            let width = cell.props.width?;
            if index < headers && row.cells.len() == 1 && cell.span() == grid.len() {
                if width > total + WIDTH_EPSILON {
                    return None;
                }
                undersized_title |= width + WIDTH_EPSILON < total;
            } else {
                declared += width;
                let boundary: f32 = widths.get(..end)?.iter().sum();
                if !declared.is_finite() || (width - expected).abs() > WIDTH_EPSILON || (declared - boundary).abs() > WIDTH_EPSILON {
                    return None;
                }
            }
            start = end;
        }
    }
    undersized_title.then_some(widths)
}

/// Consistent cell boundaries replace a stale grid before the table width is applied.
/// Every boundary must be declared; missing or conflicting preferences keep the grid.
fn apply_cell_widths(t: &Table, grid: &mut [f32]) {
    if let Some(widths) = title_point_widths(t, grid) {
        grid.copy_from_slice(&widths);
        return;
    }
    const WIDTH_EPSILON: f32 = 0.01;
    let total: f32 = grid.iter().sum();
    if grid.is_empty() || !total.is_finite() {
        return;
    }
    let total = point_row_total(t, grid.len()).unwrap_or(total);
    let mut edges = vec![None; grid.len() + 1];
    edges[0] = Some(0.0);
    edges[grid.len()] = Some(total);
    for row in &t.rows {
        let pct_total = rounded_pct_total(row, grid.len());
        let (mut g, mut x) = (0usize, 0.0f32);
        for cell in &row.cells {
            let width = match (cell.props.width_pct, cell.props.width) {
                (Some(pct), _) => total * pct / pct_total,
                (_, Some(width)) => width,
                _ => return,
            };
            if !width.is_finite() || width <= 0.0 {
                return;
            }
            g = g.saturating_add(cell.span());
            x += width;
            let Some(edge) = edges.get_mut(g) else { return };
            if !x.is_finite() || edge.is_some_and(|previous: f32| (previous - x).abs() > WIDTH_EPSILON) {
                return;
            }
            if edge.is_none() {
                *edge = Some(x);
            }
        }
        if g != grid.len() {
            return;
        }
    }
    let widths: Option<Vec<f32>> = edges.windows(2).map(|pair| Some(pair[1]? - pair[0]?)).collect();
    if let Some(widths) = widths.filter(|widths| widths.iter().all(|w| *w > 0.0)) {
        grid.copy_from_slice(&widths);
    }
}

// Word retains a complete percentage row rounded down by one OOXML fiftieth-percent unit.
// Normalize that remainder before solving shared boundaries; real conflicts still reject the row.
fn rounded_pct_total(row: &Row, columns: usize) -> f32 {
    const PERCENT_TOTAL: f64 = 100.0;
    const OOXML_PCT_UNIT: f64 = 1.0 / 50.0;
    const FLOAT_SLOP: f64 = f32::EPSILON as f64 * PERCENT_TOTAL;
    if row.first_grid() != 0 || row.props.grid_after != 0 || row.cells.iter().fold(0usize, |n, c| n.saturating_add(c.span())) != columns {
        return PERCENT_TOTAL as f32;
    }
    let total: Option<f64> = row.cells.iter().map(|c| c.props.width_pct.filter(|p| p.is_finite() && *p > 0.0).map(f64::from)).sum();
    match total {
        Some(sum) if PERCENT_TOTAL - sum > FLOAT_SLOP && PERCENT_TOTAL - sum <= OOXML_PCT_UNIT + FLOAT_SLOP => sum as f32,
        _ => PERCENT_TOTAL as f32,
    }
}

/// Word's autofit for a table whose widths may follow its content (`tblLayout` not fixed).
/// - With no preferred width anywhere (the table's and every cell's automatic) the columns come
///   from their content alone: each its widest line if they all fit in `avail`, else shared out
///   between narrowest and widest in proportion to how much each can give.
/// - Otherwise the grid stands, but no column is narrower than its longest word: the columns with
///   room to spare give up the difference.
fn autofit(grid: &mut [f32], content: &[(f32, f32)], t: &Table, avail: f32) {
    if grid.len() != content.len() || !avail.is_finite() {
        return;
    }
    if is_automatic(t) {
        let (lo, hi) = content.iter().fold((0.0f32, 0.0f32), |(l, h), c| (l + c.0, h + c.1.max(c.0)));
        let k = if hi <= avail {
            1.0
        } else if lo >= avail || hi - lo < 0.01 {
            0.0
        } else {
            (avail - lo) / (hi - lo)
        };
        for (w, c) in grid.iter_mut().zip(content) {
            *w = (c.0 + (c.1.max(c.0) - c.0) * k).max(4.0);
        }
        return;
    }
    let short: f32 = grid.iter().zip(content).map(|(w, c)| (c.0 - *w).max(0.0)).sum();
    if short < 0.01 {
        return;
    }
    // Donors give equal widths until they reach their own content minimum.
    let mut rooms: Vec<_> = grid.iter().zip(content).map(|(w, c)| *w - c.0).filter(|room| *room > 0.0).collect();
    rooms.sort_by(f32::total_cmp);
    let mut remaining = short;
    let mut share = 0.0;
    for (i, room) in rooms.iter().enumerate() {
        share = remaining / (rooms.len() - i) as f32;
        if share <= *room {
            break;
        }
        remaining -= room;
    }
    for (w, c) in grid.iter_mut().zip(content) {
        *w = if *w < c.0 { c.0 } else { *w - (*w - c.0).min(share) };
    }
}

#[cfg(test)]
mod width_tests {
    use super::*;

    fn span_floor_table() -> Table {
        let mut table = Table::new(1, 2, 100.0);
        table.props.style = None;
        table.props.cell_margins = Some([0.0, 20.0, 0.0, 20.0]);
        table.rows[0].props.exceptions = Some(Box::new(wordcraft_doc::props::TableProps { cell_margins: Some([0.0; 4]), ..Default::default() }));
        table.rows[0].cells.truncate(1);
        table.rows[0].cells[0].props.span = 2;
        table.rows[0].cells[0].blocks.clear();
        table
    }

    #[test]
    fn span_minima_share_cell_floor() {
        let table = span_floor_table();
        assert_eq!(measure_table_columns(&wordcraft_doc::Document::new(), &table), vec![(6.0, 6.0); 2]);
    }

    #[test]
    fn span_minima_keep_uncovered() {
        let mut table = span_floor_table();
        table.grid.resize(4, 25.0);
        let mut sibling = wordcraft_doc::table::Cell::empty();
        sibling.blocks.clear();
        sibling.props.margins = Some([0.0, 3.0, 0.0, 7.0]);
        table.rows[0].cells.push(sibling);
        assert_eq!(measure_table_columns(&wordcraft_doc::Document::new(), &table), vec![(6.0, 6.0), (6.0, 6.0), (22.0, 22.0), (52.0, 52.0)]);
    }

    #[test]
    fn span_minima_cover_clamped_grid() {
        let mut table = span_floor_table();
        let mut leading = wordcraft_doc::table::Cell::empty();
        leading.blocks.clear();
        table.rows[0].cells.insert(0, leading);
        table.rows[0].cells[1].props.span = u32::MAX;
        let columns = measure_table_columns(&wordcraft_doc::Document::new(), &table);
        assert_eq!(columns.len(), wordcraft_doc::table::MAX_COLS);
        assert_eq!(columns[0], (12.0, 12.0));
        let share = 12.0 / (wordcraft_doc::table::MAX_COLS - 1) as f32;
        assert_eq!(columns[1..], vec![(share, share); wordcraft_doc::table::MAX_COLS - 1]);
    }

    #[test]
    fn span_minima_keep_budget_tail() {
        let mut table = span_floor_table();
        table.grid.resize(5, 20.0);
        let mut spent = wordcraft_doc::table::Cell::empty();
        spent.blocks = vec![wordcraft_doc::para_block(wordcraft_doc::Paragraph::new()); MEASURE_BUDGET];
        table.rows[0].cells.push(spent);
        let mut tail = wordcraft_doc::table::Cell::empty();
        tail.props.span = 2;
        table.rows[0].cells.push(tail);
        assert_eq!(
            measure_table_columns(&wordcraft_doc::Document::new(), &table),
            vec![(6.0, 6.0), (6.0, 6.0), (12.0, 12.0), (52.0, 52.0), (52.0, 52.0)]
        );
    }

    #[test]
    fn span_minima_skip_partial_cell() {
        let mut table = span_floor_table();
        table.rows[0].cells[0].blocks = vec![wordcraft_doc::para_block(wordcraft_doc::Paragraph::new()); MEASURE_BUDGET + 1];
        assert_eq!(measure_table_columns(&wordcraft_doc::Document::new(), &table), vec![(52.0, 52.0); 2]);
    }

    #[test]
    fn span_minima_keep_fork_gaps() {
        let mut table = span_floor_table();
        table.grid.resize(4, 25.0);
        table.rows[0].props.grid_before = 1;
        table.rows[0].props.grid_after = 1;
        table.rows[0].props.w_before = Some(30.0);
        table.rows[0].props.w_after = Some(70.0);
        assert_eq!(measure_table_columns(&wordcraft_doc::Document::new(), &table), vec![(52.0, 52.0), (6.0, 6.0), (6.0, 6.0), (70.0, 70.0)]);
    }

    fn merged_width_table() -> Table {
        let mut table = Table::new(3, 4, 400.0);
        let rows = [vec![(4, 400.0)], vec![(2, 150.0), (1, 200.0), (1, 50.0)], vec![(1, 60.0), (1, 90.0), (2, 250.0)]];
        for (row, widths) in table.rows.iter_mut().zip(rows) {
            row.cells.truncate(widths.len());
            for (cell, (span, width)) in row.cells.iter_mut().zip(widths) {
                cell.props.span = span;
                cell.props.width = Some(width);
            }
        }
        table
    }

    #[test]
    fn span_respects_column_capacity() {
        let grid = [150.0, 100.0, 25.0, 25.0];
        let mut content = [(10.0, 10.0); 4];
        share_span_min(&mut content, Some(&grid), 300.0);
        assert!((content.iter().map(|c| c.0).sum::<f32>() - 300.0).abs() < 0.01);
        for (cell, width) in content.iter().zip(grid) {
            assert!(cell.0 <= width + 0.01, "minimum {} exceeds fitting column {width}", cell.0);
            assert!(cell.0 >= 10.0);
        }
    }

    #[test]
    fn autofit_shares_donor_loss() {
        let table = Table::new(1, 4, 260.0);
        let mut grid = [30.0, 120.0, 20.0, 90.0];
        let content = [(28.0, 28.0), (40.0, 120.0), (40.0, 40.0), (30.0, 90.0)];
        autofit(&mut grid, &content, &table, 260.0);
        assert_eq!(grid, [28.0, 111.0, 40.0, 81.0]);
    }

    #[test]
    fn autofit_exhausts_donors() {
        let table = Table::new(1, 2, 50.0);
        let mut grid = [30.0, 20.0];
        autofit(&mut grid, &[(25.0, 25.0), (40.0, 40.0)], &table, 50.0);
        assert_eq!(grid, [25.0, 40.0]);
    }

    #[test]
    fn autofit_keeps_fitting_widths() {
        let table = Table::new(1, 3, 120.0);
        let mut grid = [30.0, 40.0, 50.0];
        autofit(&mut grid, &[(10.0, 10.0), (20.0, 20.0), (30.0, 30.0)], &table, 120.0);
        assert_eq!(grid, [30.0, 40.0, 50.0]);
    }

    #[test]
    fn merged_rows_replace_stale_grid() {
        let table = merged_width_table();
        let mut grid = table.grid.clone();
        apply_cell_widths(&table, &mut grid);
        assert_eq!(grid, [60.0, 90.0, 200.0, 50.0]);
    }

    #[test]
    fn percent_rows_replace_grid() {
        let mut table = merged_width_table();
        for cell in table.rows.iter_mut().flat_map(|row| &mut row.cells) {
            cell.props.width_pct = cell.props.width.map(|width| width / 4.0);
            cell.props.width = None;
        }
        let mut grid = table.grid.clone();
        apply_cell_widths(&table, &mut grid);
        assert_eq!(grid, [60.0, 90.0, 200.0, 50.0]);

        table.rows[1].cells[0].props.width_pct = None;
        table.rows[1].cells[0].props.width = Some(150.0);
        grid.copy_from_slice(&table.grid);
        apply_cell_widths(&table, &mut grid);
        assert_eq!(grid, [60.0, 90.0, 200.0, 50.0]);
    }

    fn rounded_width_table() -> Table {
        let mut table = Table::new(3, 6, 468.0);
        table.grid = vec![25.0, 25.0, 30.0, 300.0, 44.0, 44.0];
        for row in &mut table.rows {
            for (cell, pct) in row.cells.iter_mut().zip([18.58, 7.28, 16.04, 18.36, 17.38, 22.34]) {
                cell.props.width = None;
                cell.props.width_pct = Some(pct);
            }
        }
        table
    }

    #[test]
    fn rounded_percent_rows_fit() {
        for total in [468.0, 422.05, 1584.0] {
            let table = rounded_width_table();
            let mut grid: Vec<_> = table.grid.iter().map(|width| width * total / 468.0).collect();
            apply_cell_widths(&table, &mut grid);
            for (width, pct) in grid.iter().zip([18.58, 7.28, 16.04, 18.36, 17.38, 22.34]) {
                assert!((width - total * pct / 99.98).abs() < 0.01, "{grid:?}");
            }
            assert!((grid.iter().sum::<f32>() - total).abs() < 0.01);
        }
    }

    #[test]
    fn rounded_spans_share_edges() {
        let mut table = rounded_width_table();
        table.rows[0].cells.remove(1);
        table.rows[0].cells[0].props.span = 2;
        table.rows[0].cells[0].props.width_pct = Some(25.86);
        let mut grid = table.grid.clone();
        apply_cell_widths(&table, &mut grid);
        assert!((grid[0] - 468.0 * 18.58 / 99.98).abs() < 0.01);
        assert!((grid[1] - 468.0 * 7.28 / 99.98).abs() < 0.01);
    }

    #[test]
    fn rounding_has_one_unit_limit() {
        for (first, normalized) in [(18.58, true), (18.5801, true), (18.599998, false), (18.5799, false), (18.56, false), (18.62, false)] {
            let mut table = rounded_width_table();
            table.rows[0].cells[0].props.width_pct = Some(first);
            let total = rounded_pct_total(&table.rows[0], 6);
            assert_eq!(total < 100.0, normalized, "first {first}: {total}");
        }
        let mut table = rounded_width_table();
        table.rows[0].props.grid_before = 1;
        assert_eq!(rounded_pct_total(&table.rows[0], 6), 100.0);
        table.rows[0].props.grid_before = 0;
        table.rows[0].props.grid_after = 1;
        assert_eq!(rounded_pct_total(&table.rows[0], 6), 100.0);
    }

    #[test]
    fn rounding_keeps_conflict_guard() {
        for case in 0..8 {
            let mut table = rounded_width_table();
            match case {
                0 => table.rows[1].cells[0].props.width_pct = Some(18.56),
                1 => table.rows[1].cells[0].props.width_pct = Some(18.60),
                2 => table.rows[1].cells[0].props.width_pct = Some(f32::NAN),
                3 => table.rows[1].cells[0].props.width_pct = None,
                4 => table.rows[1].cells[0].props.width_pct = Some(0.0),
                5 => table.rows[1].cells[0].props.span = 2,
                6 => {
                    table.rows[1].cells[0].props.width_pct = Some(19.58);
                    table.rows[1].cells[3].props.width_pct = Some(17.36);
                }
                _ => table.rows.iter_mut().for_each(|row| row.cells[0].props.width_pct = Some(18.5799)),
            }
            let mut grid = table.grid.clone();
            apply_cell_widths(&table, &mut grid);
            assert_eq!(grid, table.grid, "case {case}");
        }
    }

    #[test]
    fn uncertain_widths_keep_grid() {
        for case in 0..7 {
            let mut table = merged_width_table();
            match case {
                0 => {
                    table.rows[2].cells[1].props.width = Some(95.0);
                    table.rows[2].cells[2].props.width = Some(245.0);
                }
                1 => table.rows[2].cells[1].props.width = None,
                2 => table.rows[2].cells[1].props.width_pct = Some(20.0),
                3 => table.rows.truncate(2),
                4 => table.rows[2].cells[1].props.span = 2,
                5 => table.rows[2].cells[1].props.width = Some(0.0),
                _ => table.rows[2].cells[1].props.width = Some(f32::NAN),
            }
            let mut grid = table.grid.clone();
            apply_cell_widths(&table, &mut grid);
            assert_eq!(grid, table.grid, "case {case}");
        }
    }
    fn point_width_table() -> Table {
        let mut table = merged_width_table();
        table.props.width_pct = Some(75.0);
        table.grid = vec![90.0, 100.0, 160.0, 150.0];
        table
    }

    #[test]
    fn point_rows_replace_grid() {
        let table = point_width_table();
        let mut grid = table.grid.clone();
        apply_cell_widths(&table, &mut grid);
        assert_eq!(grid, [60.0, 90.0, 200.0, 50.0]);
    }

    #[test]
    fn point_rows_reject_conflicts() {
        for case in 0..10 {
            let mut table = point_width_table();
            match case {
                0 => table.props.fixed = true,
                1 => table.props.width_pct = None,
                2 => table.props.width_pct = Some(f32::NAN),
                3 => table.rows[1].cells[0].props.width_pct = Some(30.0),
                4 => table.rows[1].cells[0].props.width = None,
                5 => table.rows[1].cells[0].props.width = Some(f32::INFINITY),
                6 => table.rows[1].props.grid_before = 1,
                7 => table.rows[1].props.grid_after = 1,
                8 => table.rows[1].cells[0].props.width = Some(151.0),
                _ => table.rows[2].cells[0].props.width = Some(0.0),
            }
            let mut grid = table.grid.clone();
            apply_cell_widths(&table, &mut grid);
            assert_eq!(grid, table.grid, "case {case}");
        }
    }
}

#[cfg(test)]
mod para_rule_controls {
    use super::*;
    use wordcraft_doc::props::{BorderStyle, LineSpacing, ParaProps};
    use wordcraft_doc::{Document, Path};

    fn emitted_rules(mask: u8) -> Vec<Placed> {
        emitted_text(mask, "Owned")
    }

    fn emitted_text(mask: u8, text: &str) -> Vec<Placed> {
        let border = Border { space: 8.0, ..Border::single(2.0) };
        let hidden = Border { style: BorderStyle::None, ..border };
        let edge = |bit| Some(if mask & bit != 0 { border } else { hidden });
        let mut doc = Document::new();
        doc.body.clear();
        assert!(doc.body.is_empty());
        let mut paragraph = Paragraph::with_text(text, Default::default());
        paragraph.props = ParaProps {
            borders: Some(Borders { top: edge(1), bottom: edge(2), left: edge(4), right: edge(8), ..Default::default() }),
            line_spacing: Some(LineSpacing::Exactly(20.0)),
            widow_control: Some(false),
            ..Default::default()
        };
        assert_eq!(paragraph.text, text);
        doc.body.push(wordcraft_doc::para_block(paragraph));
        assert_eq!(doc.body.len(), 1);
        let layout = crate::layout(&doc, &mut crate::LayoutCache::new(), &crate::LayoutOptions::default());
        let para = layout
            .pages
            .iter()
            .flat_map(|p| &p.items)
            .find_map(|it| match it {
                Placed::Lines { path, para, .. } if path.0 == [0] => Some(para.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(para.text_len, text.len());
        assert_eq!(para.lines.len(), text.split(wordcraft_doc::para::LINE_BREAK).count());
        let mut items = Vec::new();
        crate::push_para(&mut items, StoryRef::Body, &[0], &para, 0, para.lines.len(), 0.0, 60.0, 100.0);
        items.push(Placed::Image {
            rect: Rect::new(5.0, 150.0, 20.0, 20.0),
            media: "synthetic".into(),
            crop: [0.0; 4],
            story: StoryRef::Body,
            path: Path::top(1),
            off: 0,
            spin: Default::default(),
        });
        items
    }

    #[test]
    fn exact_para_rule_streams() {
        for mask in 0..16 {
            let items = emitted_rules(mask);
            let mut rule = 1;
            for (bit, edge) in [(1, Some(ParaEdge::Top)), (2, Some(ParaEdge::Bottom)), (4, None), (8, None)] {
                if mask & bit == 0 {
                    continue;
                }
                let expected = edge.map(|edge| (0, edge));
                assert!(para_rule_owner(&items, rule) == expected, "mask {mask}, rule {rule}");
                rule += 1;
            }
            assert_eq!(items.len(), 2 + mask.count_ones() as usize);
        }
        let mut interrupted = emitted_rules(15);
        interrupted.remove(2);
        for index in 1..interrupted.len() {
            assert!(para_rule_owner(&interrupted, index).is_none(), "missing bottom invalidates the complete run");
        }
        let mut turned = emitted_rules(15);
        if let Placed::Lines { turn, .. } = &mut turned[0] {
            *turn = TextDirection::Down;
        }
        assert!(para_rule_owner(&turned, 1).is_none());

        // A matching cell/merge edge after the run makes its boundary ambiguous.
        let mut outer = emitted_rules(3);
        outer.insert(3, outer[1].clone());
        assert!(para_rule_owner(&outer, 1).is_none());
        assert!(para_rule_owner(&outer, 2).is_none());
        assert!(para_rule_owner(&outer, 3).is_none());
        let mut cell_boundary = emitted_rules(3);
        cell_boundary.pop();
        cell_boundary.push(Placed::Cell {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            covered: false,
            table: Path::top(0),
            row: 0,
            cell: 0,
            story: StoryRef::Body,
            clip: false,
        });
        assert!(para_rule_owner(&cell_boundary, 1).is_none());
        assert!(para_rule_owner(&cell_boundary, 2).is_none());
    }

    #[test]
    fn three_fragment_rule_prefix() {
        for mask in [3, 15] {
            for whole_first in [false, true] {
                let emitted = emitted_text(mask, "One\nTwo\nThree\nFour\nFive\nSix");
                let para = emitted
                    .iter()
                    .find_map(|it| match it {
                        Placed::Lines { para, .. } => Some(para.clone()),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(para.lines.len(), 6);
                for (l0, l1, edge) in [(0, 2, Some(ParaEdge::Top)), (2, 4, None), (4, 6, Some(ParaEdge::Bottom))] {
                    let mut range = Vec::new();
                    crate::push_para(&mut range, StoryRef::Body, &[0], &para, l0, l1, 0.0, 20.0, 100.0);
                    range.push(emitted.last().unwrap().clone());
                    assert!(para_rule_owner(&range, 1) == edge.map(|edge| (0, edge)), "actual emitter range {l0}..{l1}");
                }
                let image = |y| Placed::Image {
                    rect: Rect::new(0.0, y, 10.0, 10.0),
                    media: "synthetic".into(),
                    crop: [0.0; 4],
                    story: StoryRef::Body,
                    path: Path::top(1),
                    off: 0,
                    spin: Default::default(),
                };
                let mut items = vec![image(0.0)];
                crate::push_para(&mut items, StoryRef::Body, &[0], &para, 0, para.lines.len(), 0.0, 20.0, 100.0);
                items.push(image(200.0));
                let row = RowLayout {
                    height: 240.0,
                    min_height: 0.0,
                    foreground: Vec::new(),
                    bottom: 4.0,
                    cells: vec![CellPart {
                        anchored_rules: Vec::new(),
                        bottom_rule: None,
                        bottom_band: 0.0,
                        valign: VAlign::Top,
                        top: 2.0,
                        items: 0..items.len(),
                        end: None,
                        center: None,
                        merge: None,
                    }],
                    items,
                };
                let cuts = if whole_first { [30.0, 60.0, 110.0] } else { [70.0, 55.0, 70.0] };
                let (first, next) = split_row(&row, cuts[0]).unwrap();
                let owner = next.items.iter().position(|it| matches!(it, Placed::Lines { .. })).unwrap();
                let prefix = if whole_first { ParaEdge::Top } else { ParaEdge::Bottom };
                assert!(para_rule_owner(&next.items, owner + 1) == Some((owner, prefix)), "first continuation prefix");
                let (second, next) = split_row(&next, cuts[1]).unwrap();
                let owner = next.items.iter().position(|it| matches!(it, Placed::Lines { .. })).unwrap();
                assert!(
                    para_rule_owner(&next.items, owner + 1) == Some((owner, ParaEdge::Bottom)),
                    "second continuation bottom prefix despite clipped sides"
                );
                let (third, tail) = split_row(&next, cuts[2]).unwrap();
                let lines = |part: &RowLayout| {
                    part.items
                        .iter()
                        .filter_map(|it| match it {
                            Placed::Lines { l0, l1, .. } => Some(*l0..*l1),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                if whole_first {
                    assert!(lines(&first).is_empty());
                    assert_eq!(lines(&second), vec![0..2]);
                    assert_eq!(lines(&third), vec![2..6]);
                } else {
                    assert_eq!(lines(&first), vec![0..2]);
                    assert_eq!(lines(&second), vec![2..4]);
                    assert_eq!(lines(&third), vec![4..6]);
                }
                assert!(lines(&tail).is_empty());
                assert_eq!([&first, &second, &third].into_iter().flat_map(lines).flatten().collect::<Vec<_>>(), (0..6).collect::<Vec<_>>());
                let horizontal = |part: &RowLayout| {
                    part.items
                        .iter()
                        .filter_map(|it| match it {
                            Placed::Rule { y0, y1, .. } if y0 == y1 => Some(*y0),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(horizontal(&first), if whole_first { vec![] } else { vec![12.0] });
                assert_eq!(horizontal(&second), if whole_first { vec![2.0] } else { vec![] });
                let bottom = if whole_first { 90.0 } else { 50.0 };
                assert_eq!(horizontal(&third), vec![bottom]);
                assert!(horizontal(&tail).is_empty());
                assert_eq!(third.height, bottom + 4.0, "recovered bottom prefix extends retained extent once");
                let last_bottom = third
                    .items
                    .iter()
                    .find_map(|it| match it {
                        Placed::Lines { para, l0, l1, y, .. } => crate::item_bottom(*y, para, *l0, *l1),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(bottom - last_bottom, 8.0, "bottom spacing survives both continuations");
                for border in [&first, &second, &third].into_iter().flat_map(|part| &part.items).filter_map(|it| match it {
                    Placed::Rule { border, .. } => Some(border),
                    _ => None,
                }) {
                    assert_eq!(border.width, 2.0, "no stroke-half-width trimming");
                }
            }
        }
    }

    #[test]
    fn centered_rule_fallback() {
        let items = emitted_rules(3);
        let row = RowLayout {
            height: 200.0,
            min_height: 0.0,
            foreground: Vec::new(),
            bottom: 4.0,
            cells: vec![CellPart {
                anchored_rules: Vec::new(),
                bottom_rule: None,
                bottom_band: 0.0,
                valign: VAlign::Top,
                top: 2.0,
                items: 0..items.len(),
                end: None,
                merge: None,
                center: Some(CellCenter { kind: CenterKind::Merge, offset: 0.0, height: 300.0, content: 0..items.len() }),
            }],
            items,
        };
        // Oversized center groups use the existing ordinary geometry, without new ownership.
        // Keep a preceding fitting item so the row can split.
        let mut row = row;
        row.items.insert(
            0,
            Placed::Image {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                media: "synthetic".into(),
                crop: [0.0; 4],
                story: StoryRef::Body,
                path: Path::top(2),
                off: 0,
                spin: Default::default(),
            },
        );
        row.cells[0].items.end += 1;
        let (first, rest) = split_row(&row, 70.0).unwrap();
        let rules = |part: &RowLayout| {
            part.items
                .iter()
                .filter_map(|it| match it {
                    Placed::Rule { y0, y1, .. } if y0 == y1 => Some(*y0),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(first.height, 56.0);
        assert_eq!(rules(&first), vec![52.0]);
        assert_eq!(rules(&rest), vec![30.0]);
        let line_y = rest
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Lines { y, .. } => Some(*y),
                _ => None,
            })
            .unwrap();
        assert_eq!(line_y, 2.0);

        // A genuinely fitting grouped paragraph keeps the existing center override.
        row.min_height = 40.0;
        row.cells[0].center = Some(CellCenter { kind: CenterKind::Merge, offset: 52.0, height: 36.0, content: 1..4 });
        let (first, rest) = split_row(&row, 70.0).unwrap();
        assert_eq!(first.height, 40.0);
        assert_eq!(rules(&first), vec![2.0, 38.0]);
        assert!(rules(&rest).is_empty());
        let line_y = first
            .items
            .iter()
            .find_map(|it| match it {
                Placed::Lines { y, .. } => Some(*y),
                _ => None,
            })
            .unwrap();
        assert_eq!(line_y, 10.0);
    }

    fn centered_frame_row() -> RowLayout {
        let mut items = emitted_rules(3);
        items.insert(
            0,
            Placed::Image {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                media: "synthetic".into(),
                crop: [0.0; 4],
                story: StoryRef::Body,
                path: Path::top(2),
                off: 0,
                spin: Default::default(),
            },
        );
        let rect = Rect::new(0.0, 0.0, 100.0, 200.0);
        items.push(Placed::Fill { rect, color: wordcraft_doc::props::Rgb(220, 220, 220) });
        items.push(Placed::Cell { rect, covered: false, table: Path::top(0), row: 0, cell: 0, story: StoryRef::Body, clip: false });
        RowLayout {
            height: 200.0,
            min_height: 0.0,
            foreground: Vec::new(),
            bottom: 4.0,
            cells: vec![CellPart {
                anchored_rules: Vec::new(),
                bottom_rule: None,
                bottom_band: 0.0,
                valign: VAlign::Top,
                top: 2.0,
                items: 0..items.len(),
                end: None,
                merge: None,
                center: Some(CellCenter { kind: CenterKind::Merge, offset: 0.0, height: 300.0, content: 0..items.len() }),
            }],
            items,
        }
    }

    #[test]
    fn center_rule_frame_bounds() {
        let row = centered_frame_row();
        let (first, rest) = split_row(&row, 70.0).unwrap();
        for item in &first.items {
            if let Placed::Rule { y0, y1, .. } = item
                && y0 == y1
            {
                assert!(*y0 <= first.height, "retained centerline {y0} outside first frame {}", first.height);
            }
        }
        assert_eq!(first.height, 56.0);
        let rules = |part: &RowLayout| {
            part.items
                .iter()
                .filter_map(|item| match item {
                    Placed::Rule { y0, y1, border, .. } if y0 == y1 => Some((*y0, border.width)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(rules(&first), [(52.0, 2.0)]);
        assert_eq!(rules(&rest), [(30.0, 2.0)]);
        assert!(rest.items.iter().any(|item| matches!(item, Placed::Lines { l0: 0, l1: 1, y, .. } if *y == 2.0)));
        let frames: Vec<_> = first
            .items
            .iter()
            .filter_map(|item| match item {
                Placed::Cell { rect, .. } | Placed::Fill { rect, .. } => Some(rect.bottom()),
                _ => None,
            })
            .collect();
        assert_eq!(frames, [56.0, 56.0]);
    }

    #[test]
    fn fitting_rule_frame_unchanged() {
        let mut row = centered_frame_row();
        row.min_height = 40.0;
        row.cells[0].center = Some(CellCenter { kind: CenterKind::Merge, offset: 52.0, height: 36.0, content: 1..4 });
        let (first, rest) = split_row(&row, 70.0).unwrap();
        assert_eq!(first.height, 40.0);
        let rules: Vec<_> = first
            .items
            .iter()
            .filter_map(|item| match item {
                Placed::Rule { y0, y1, .. } if y0 == y1 => Some(*y0),
                _ => None,
            })
            .collect();
        assert_eq!(rules, [2.0, 38.0]);
        assert!(!rest.items.iter().any(|item| matches!(item, Placed::Rule { .. })));
        assert!(first.items.iter().any(|item| matches!(item, Placed::Lines { l0: 0, l1: 1, y, .. } if *y == 10.0)));
    }
}

#[cfg(test)]
mod fragment_controls;

#[cfg(test)]
#[path = "table/title_width_controls.rs"]
mod title_width_controls;

#[cfg(test)]
#[path = "table/exact_width_controls.rs"]
mod exact_width_controls;

#[cfg(test)]
#[path = "table/side_border_controls.rs"]
mod side_border_controls;
