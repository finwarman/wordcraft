//! WordCraft layout: turns a [`Document`] into pages.
//!
//! - [`para`]: one paragraph → lines (shaping, first-fit line breaking, tabs, list labels,
//!   alignment). Results are memoised per paragraph revision in a [`LayoutCache`].
//! - [`layout`]: sections → pages and columns; tables; keep/widow rules; page breaks; headers
//!   and footers; page-number fields.
//! - [`display`]: a page → draw items (glyph runs, rules, fills, images) for the renderers.
//! - [`hit`]: point ↔ position, caret geometry, line navigation, selection rectangles.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod balloons;
mod contour;
pub mod display;
pub mod fields;
pub mod hit;
pub mod kinsoku;
pub mod math;
pub mod outline;
pub mod para;
pub mod styleref;
mod table;
mod textbox;
mod vertical;
pub mod wordart;
mod work;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use wordcraft_doc::numbering::{Counters, Level};
use wordcraft_doc::para::{Anchor, Float, FloatAlign, InlineObject, Wrap, WrapText};
use wordcraft_doc::props::{Border, CharProps, HeightRule, Rgb, TableFloat, TextDirection};
use wordcraft_doc::section::{HeaderSet, SectionProps, SectionStart};
use wordcraft_doc::{Block, Blocks, Document, Paragraph, Path, StoryRef};
use wordcraft_geom::{Point, Rect, Spin};

pub use fields::FieldCtx;
pub use hit::VisualStep;
pub use para::{LineEnd, ParaLayout};
pub use table::{autofit_widths, cell_margins, measure_table_columns};

mod boxes;
mod cache;
mod ctx;
mod floats;
mod flow;
mod foreground;
mod headers;
mod notes;
mod page;
mod pass;
mod placed;
mod table_flow;

#[cfg(test)]
use boxes::{BOX_INSET_X, BOX_INSET_Y};
use boxes::{BoxInput, BoxLayout, Edges, fit_box, layout_box, push_para};
pub use cache::LayoutCache;
use cache::{MAX_TOTAL_PASSES, Totals};
#[cfg(test)]
use ctx::picture_outline;
use ctx::{Ctx, Pass};
use floats::{EarlyFloats, MAX_REWRAP_PASSES, WrapArea, merge_early};
pub use floats::{is_floating, shape_figure_alt};
#[cfg(test)]
use floats::{left_out_objects, wrap_area};
#[cfg(test)]
use headers::MAX_HEADER_ITEMS;
pub use notes::DECOR_STORY;
pub(crate) use notes::NOTE_SEP_W;
use notes::{MAX_NOTE_PASSES, footnote_pages};
#[cfg(test)]
use notes::{NOTE_SEP, note_numbers};
use page::item_bottom;
use pass::layout_pass;
pub use pass::now_ms;
pub use placed::{HfSlot, Placed, turn_point, turn_rect, unturn_point};

/// How the document is viewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Print,
    /// One page as wide as the window, no page breaks.
    Web,
    /// Draft: like web, page breaks shown as rules.
    Draft,
    /// Reading: reflow into viewport-height narrow pages, navigated horizontally.
    Read,
    /// Outline: like draft, the body as an outline of its headings ([`outline`]).
    Outline,
}

/// How digits look on screen (Word › File › Options › Advanced › Numeral).
///
/// Display only: the document text keeps the typed codepoints, so find, spelling, copying and
/// the saved file all see the original digits. Only ASCII digits are substituted: Arabic-Indic
/// (U+0660–U+0669) and Extended Arabic-Indic digits (U+06F0–U+06F9, Persian/Urdu ۰۱۲۳…) always
/// show as typed, so fonts that ligate them (e.g. the end-of-ayah mark U+06DD) keep working.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumeralMode {
    /// Western digits everywhere (`0123456789`); the default, today's behaviour.
    #[default]
    Arabic,
    /// Eastern Arabic-Indic digits everywhere (`٠١٢٣٤٥٦٧٨٩`).
    Hindi,
    /// Hindi digits in and around right-to-left text, Western digits elsewhere (numbers take
    /// an even bidi level, so the surrounding text decides).
    Context,
    /// Hindi digits when the system locale is an Arabic-script language, else Western digits.
    /// Call [`NumeralMode::resolve`] before laying out.
    System,
}

impl NumeralMode {
    /// Short UI label (Word's option names).
    pub fn label(self) -> &'static str {
        match self {
            NumeralMode::Arabic => "Arabic",
            NumeralMode::Hindi => "Hindi",
            NumeralMode::Context => "Context",
            NumeralMode::System => "System",
        }
    }

    /// Parse a command value (`"arabic" | "hindi" | "context" | "system"`, any case).
    pub fn parse(s: &str) -> Option<NumeralMode> {
        match s.to_lowercase().as_str() {
            "arabic" => Some(NumeralMode::Arabic),
            "hindi" => Some(NumeralMode::Hindi),
            "context" => Some(NumeralMode::Context),
            "system" => Some(NumeralMode::System),
            _ => None,
        }
    }

    /// Resolve `System` against the system locale (a BCP 47 or POSIX tag such as `ar-EG` or
    /// `fa_IR.UTF-8`, found by the shell: layout never reads the environment). Hindi digits for
    /// Arabic-script languages, else Western; unknown locale = Western. Other modes pass through.
    pub fn resolve(self, system_locale: Option<&str>) -> NumeralMode {
        if self != NumeralMode::System {
            return self;
        }
        if system_locale.is_some_and(hindi_locale) { NumeralMode::Hindi } else { NumeralMode::Arabic }
    }

    /// The displayed form of `c` in a run shaped with direction `rtl`.
    ///
    /// One character maps to exactly one character, so shaped text stays codepoint-aligned
    /// with the document text (only byte offsets shift). A `System` mode that was not
    /// resolved falls back to Western digits.
    pub fn map(self, c: char, rtl: bool) -> char {
        let hindi = match self {
            NumeralMode::Arabic => false,
            NumeralMode::Hindi => true,
            NumeralMode::Context => rtl,
            NumeralMode::System => false,
        };
        if hindi { if c.is_ascii_digit() { char::from_u32(0x660 + (c as u32 - '0' as u32)).unwrap_or(c) } else { c } } else { c }
    }
}

/// Does locale `tag` use Hindi digits (Arabic, Persian, Urdu, Pashto, Sindhi, Kurdish)?
fn hindi_locale(tag: &str) -> bool {
    let lang = tag.split(['_', '-', '.', '@']).next().unwrap_or_default().to_ascii_lowercase();
    matches!(lang.as_str(), "ar" | "fa" | "ur" | "ps" | "sd" | "ku" | "ckb")
}

#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    pub view: ViewMode,
    /// Width of the window in Web/Draft views, points.
    pub web_width: f32,
    /// Page height in Reading Mode (document points).
    pub read_height: f32,
    /// Transient read-only line-spacing override; ignored by print/export views.
    pub read_line_spacing: Option<f32>,
    pub show_hidden: bool,
    /// Final text ("No Markup"): tracked deletions take no space and draw nothing.
    pub hide_deleted: bool,
    /// Tracked deletions that take no space and draw nothing although markup is shown: those
    /// Track Changes Options mark Hidden (deletions, or moved-away text). `hide_deleted` and
    /// `deletions_in_balloons` leave every deletion out whatever this says.
    pub hide_marked: HideDeleted,
    /// Show revisions in balloons: tracked deletions in the body take no space and draw nothing
    /// (the editor shows them in balloons beside the page). Headers, footers, notes and text
    /// boxes get no balloons, so their deletions stay inline.
    pub deletions_in_balloons: bool,
    /// Check spelling as you type (squiggles).
    pub proofing: bool,
    /// How digits look on screen (display only; the text is untouched). Callers resolve
    /// `System` ([`NumeralMode::resolve`]); an unresolved `System` shows Western digits.
    pub numeral: NumeralMode,
    /// Print Markup (PDF, print, page images): widen each page by [`balloons::MARKUP_AREA`]
    /// and show the comments there in balloons joined to their anchors, as Word prints them.
    /// The editor draws its own interactive balloons and leaves this off.
    pub comment_balloons: bool,
    /// Show field codes (`{ PAGE }`, shaded) instead of field results (View › Field Codes, Alt+F9).
    /// A field whose own `code` flag is set shows the other one.
    pub field_codes: bool,
    /// Outline view: what's collapsed and shown (used when `view` is [`ViewMode::Outline`]).
    pub outline: Option<outline::OutlineView>,
    /// What the spelling check skips, and whether grammar is marked too (File › Options › Proofing).
    pub proof: ProofOptions,
    /// The word being typed, which gets no spelling squiggle until the caret leaves it (#1552).
    pub typing_word: Option<TypingWord>,
    /// Document file name for FILENAME fields ([`FieldCtx::filename`]).
    pub filename: Option<Arc<str>>,
    /// Document full path for FILENAME \p fields ([`FieldCtx::filepath`]).
    pub filepath: Option<Arc<str>>,
}

/// The word being typed ([`LayoutOptions::typing_word`]): the paragraph by its revision
/// ([`Paragraph::rev`]) and the word's byte range in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypingWord {
    pub rev: u64,
    pub start: usize,
    pub end: usize,
}

/// File › Options › Proofing choices that change the squiggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProofOptions {
    /// Mark grammar errors as you type.
    pub grammar: bool,
    pub spelling: wordcraft_proof::Options,
}

impl Default for ProofOptions {
    fn default() -> Self {
        ProofOptions { grammar: true, spelling: wordcraft_proof::Options::default() }
    }
}

impl LayoutOptions {
    /// Which tracked deletions in `story` take no space and draw nothing.
    pub fn hides_deleted(&self, story: StoryRef) -> HideDeleted {
        if self.hide_deleted || (self.deletions_in_balloons && story == StoryRef::Body) { HideDeleted::ALL } else { self.hide_marked }
    }
}

/// Which tracked deletions are left out of the layout: ordinary ones, and the text a tracked
/// move took away (the source of a move, `w:moveFrom`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HideDeleted {
    pub plain: bool,
    pub moved_from: bool,
}

impl HideDeleted {
    pub const ALL: HideDeleted = HideDeleted { plain: true, moved_from: true };
    /// Whether any deletion is left out.
    pub fn any(self) -> bool {
        self.plain || self.moved_from
    }
    /// Whether text deleted by revision `del` (`None`: not deleted) is left out.
    pub fn hides(self, doc: &Document, del: Option<u32>) -> bool {
        del.is_some() && if doc.move_name(del).is_some() { self.moved_from } else { self.plain }
    }
}

#[derive(Clone)]
struct CellClip {
    story: StoryRef,
    prefix: Vec<u32>,
    items: std::ops::Range<usize>,
    rect: Rect,
}

/// Physical copies are indexed by source path; each vector is ordered by placement end.
type Clips = HashMap<StoryRef, HashMap<Vec<u32>, Vec<CellClip>>>;

/// Cell markers follow content. Unclipped copies still close the preceding occurrence.
fn clipped_cells(items: &[Placed]) -> Clips {
    let mut previous = HashMap::new();
    let mut clips = Clips::new();
    for (index, item) in items.iter().enumerate() {
        crate::work::steps(1);
        let Placed::Cell { rect, table, row, cell, story, clip, .. } = item else { continue };
        let start = previous.insert((*story, table, *row, *cell), index + 1).unwrap_or(0);
        if !clip {
            continue;
        }
        let (Ok(row), Ok(cell)) = (u32::try_from(*row), u32::try_from(*cell)) else { continue };
        let mut prefix = table.0.clone();
        prefix.extend([row, cell]);
        let entry = CellClip { story: *story, prefix, items: start..index, rect: *rect };
        clips.entry(entry.story).or_default().entry(entry.prefix.clone()).or_default().push(entry);
    }
    clips
}

/// Look up only ancestors, then the physical copy containing this item. Empty clips stay empty.
fn cell_clip(clips: &Clips, story: StoryRef, path: &[u32], index: usize) -> Option<Rect> {
    let cells = clips.get(&story)?;
    (0..=path.len())
        .filter_map(|n| {
            crate::work::steps(1);
            let copies = cells.get(path.get(..n)?)?;
            let at = copies.partition_point(|copy| {
                crate::work::steps(1);
                copy.items.end <= index
            });
            let copy = copies.get(at)?;
            copy.items.contains(&index).then_some(copy.rect)
        })
        .reduce(|a, b| {
            let (x, y) = (a.x.max(b.x), a.y.max(b.y));
            Rect::new(x, y, a.right().min(b.right()) - x, a.bottom().min(b.bottom()) - y)
        })
}

#[derive(Clone, Copy)]
enum LineFit {
    Complete,
    Clipped(StoryRef),
}

fn fit_cell_box(items: Vec<Placed>, height: f32, story: StoryRef) -> Vec<Placed> {
    fit_lines(items, height, LineFit::Clipped(story))
}

/// Owners keep partial ink; foreign sourced decorations follow their text's fitting policy.
fn cell_keeps(item: &Placed, owner: StoryRef) -> bool {
    match item {
        Placed::Lines { story, .. } | Placed::Fill { source: Some((story, _)), .. } | Placed::Rule { source: Some((story, _)), .. } => {
            *story == owner
        }
        _ => true,
    }
}

fn fit_lines(items: Vec<Placed>, height: f32, fit: LineFit) -> Vec<Placed> {
    let limit = height + 0.5;
    let mut out = Vec::with_capacity(items.len());
    let mut kept_line = false;
    for it in items {
        // Exact cells retain their own source under the paint clip. Embedded stories still fit.
        if let LineFit::Clipped(owner) = fit
            && cell_keeps(&it, owner)
        {
            kept_line |= matches!(it, Placed::Lines { .. });
            out.push(it);
            continue;
        }
        if let Some(r) = it.turned_bounds() {
            // Turned text (in a table cell) stays whole.
            if r.y < limit {
                out.push(it);
            }
            continue;
        }
        match it {
            Placed::Lines { story, path, para, l0, l1, x, y, turn } => {
                let Some(first) = para.lines.get(l0) else { continue };
                let mut end = l0;
                for k in l0..l1 {
                    let Some(l) = para.lines.get(k) else { break };
                    if y + (l.top - first.top) + l.height > limit && kept_line {
                        break;
                    }
                    end = k + 1;
                    kept_line = true;
                }
                if end > l0 {
                    out.push(Placed::Lines { story, path, para, l0, l1: end, x, y, turn });
                }
            }
            Placed::Fill { rect, color, source } if rect.y < limit => {
                out.push(Placed::Fill { rect: Rect::new(rect.x, rect.y, rect.w, rect.h.min(limit - rect.y)), color, source });
            }
            Placed::Rule { x0, y0, x1, y1, border, source } if y0.min(y1) < limit => {
                out.push(Placed::Rule { x0, y0: y0.min(limit), x1, y1: y1.min(limit), border, source });
            }
            Placed::Image { rect, .. }
            | Placed::Shape { rect, .. }
            | Placed::Graphic { rect, .. }
            | Placed::Spun { rect, .. }
            | Placed::Clip { rect, .. }
            | Placed::Cell { rect, .. }
            | Placed::Object { rect, .. }
                if rect.y < limit =>
            {
                out.push(it);
            }
            _ => {}
        }
    }
    out
}

impl Placed {
    /// The physical source owning this item, including grouped drawing members.
    fn source(&self) -> Option<(StoryRef, &Path, Option<usize>)> {
        let mut pending = vec![self];
        while let Some(item) = pending.pop() {
            match item {
                Placed::Lines { story, path, .. } => return Some((*story, path, None)),
                Placed::Image { story, path, off, .. } | Placed::Graphic { story, path, off, .. } => return Some((*story, path, Some(*off))),
                Placed::Fill { source: Some((story, path)), .. } | Placed::Rule { source: Some((story, path)), .. } => {
                    return Some((*story, path, None));
                }
                Placed::Shape { object: Some(pos), .. } => return Some((pos.story, &pos.path, Some(pos.off))),
                Placed::Clip { items, .. } => pending.extend(items.iter().rev()),
                _ => {}
            }
        }
        None
    }
}

/// Bound nested backgrounds after fitting; owned paint and floats retain their physical clips.
fn fit_exact_cell(items: Vec<Placed>, height: f32, story: StoryRef) -> Vec<Placed> {
    let limit = height + 0.5;
    fit_cell_box(items, height, story)
        .into_iter()
        .filter_map(|item| {
            crate::work::steps(1);
            match item {
                Placed::Lines { .. }
                | Placed::Cell { .. }
                | Placed::Image { .. }
                | Placed::Shape { .. }
                | Placed::Graphic { .. }
                | Placed::Clip { .. }
                | Placed::Object { .. }
                | Placed::Spun { .. } => Some(item),
                Placed::Fill { source: Some((owner, _)), .. } | Placed::Rule { source: Some((owner, _)), .. } if owner == story => Some(item),
                Placed::Fill { rect, color, source } if rect.y < limit => {
                    Some(Placed::Fill { rect: Rect::new(rect.x, rect.y, rect.w, rect.h.min(limit - rect.y)), color, source })
                }
                Placed::Rule { x0, y0, x1, y1, border, source } if y0.min(y1) < limit => {
                    Some(Placed::Rule { x0, y0: y0.min(limit), x1, y1: y1.min(limit), border, source })
                }
                _ => None,
            }
        })
        .collect()
}

/// One laid-out page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub w: f32,
    pub h: f32,
    /// Index into `Document::sections()`.
    pub section: usize,
    /// Page number as displayed (after section restarts).
    pub number: u32,
    pub items: Vec<Placed>,
    /// Header / footer items (drawn dimmed while editing the body).
    pub header: Vec<Placed>,
    pub footer: Vec<Placed>,
    /// The body text area (margins).
    pub body: Rect,
    pub header_story: Option<u32>,
    pub footer_story: Option<u32>,
    /// Which of its section's headers/footers this page shows (first page, even page or the
    /// default), whether or not the section has one: double-clicking an empty header area
    /// creates that one (#960).
    pub hf_slot: HfSlot,
    /// Top-level body block index at the start of the page.
    pub first_block: usize,
    /// Width of the markup area on the right of `w` that holds comment balloons (Print Markup,
    /// [`LayoutOptions::comment_balloons`]); 0 without one. The document page is `w - markup` wide.
    pub markup: f32,
    /// The top-level body tables placed on this page as floating tables (`w:tblpPr`): a click
    /// outside one doesn't land in its cells ([`DocLayout::hit`]).
    pub float_tables: Vec<Path>,
}

/// The whole layout.
#[derive(Clone, Debug, Default)]
pub struct DocLayout {
    pub pages: Vec<Page>,
    /// Index: (story, paragraph path) → (page, item index) of each placed piece, in order.
    pub index: HashMap<(StoryRef, Path), Vec<(usize, usize)>>,
    /// Layout time, milliseconds.
    pub ms: f64,
    /// Text box stories laid out (each time a box's text was).
    pub text_boxes: usize,
}

/// Word excludes a zero-numbered title page from NUMPAGES (MS-OI29500 §2.1.489).
fn num_pages(doc: &Document, physical: u32) -> u32 {
    let omit_title = doc.sections().first().is_some_and(|(_, s)| s.title_page && s.page_num_start == Some(0));
    physical.saturating_sub(u32::from(omit_title))
}

/// Lay out the whole document.
pub fn layout(doc: &Document, cache: &mut LayoutCache, opts: &LayoutOptions) -> DocLayout {
    let t0 = now_ms();
    // Page totals shown in the body (`NUMPAGES`, `SECTIONPAGES`) are known only after
    // paginating: lay out again with the totals found until they hold (bounded). Footnotes
    // numbered again on each page likewise settle over a few more passes.
    let per_page = opts.view == ViewMode::Print
        && doc.sections().iter().any(|(_, s)| doc.note_options(s, false).num_restart == wordcraft_doc::section::NoteRestart::EachPage);
    let mut note_pages = HashMap::new();
    let mut note_passes = 1;
    let mut totals = Totals::first_guess(doc.sections().len());
    let mut passes = 0;
    let mut floats = FloatReplay::default();
    let (mut early, mut rewraps) = (EarlyFloats::new(), 0);
    let mut back_passes = 0;
    let mut back_wraps = BackBodyWraps::new();
    let mut l = loop {
        #[cfg(test)]
        REPLAY_STATS.with(|c| c.borrow_mut().passes += 1);
        let mut prior = PriorBodyWraps::new();
        for (&(block, _), (page, area)) in &back_wraps {
            prior.entry(*page).or_default().push((block, area.clone()));
        }
        let mut found = BackBodyWraps::new();
        let pass = layout_pass(
            doc,
            cache,
            opts,
            &note_pages,
            &totals,
            passes < MAX_TOTAL_PASSES,
            ReplayInput { prior: &prior, found: &mut found, early: &early, may_rewrap: rewraps < MAX_REWRAP_PASSES, floats: &mut floats },
        );
        #[cfg(test)]
        if found != back_wraps && back_passes == MAX_BACK_WRAP_PASSES {
            REPLAY_STATS.with(|c| c.borrow_mut().cap_fallbacks += 1);
        }
        if found != back_wraps && back_passes < MAX_BACK_WRAP_PASSES {
            #[cfg(test)]
            REPLAY_STATS.with(|c| c.borrow_mut().back_retries += 1);
            back_wraps = found;
            back_passes += 1;
            if let Pass::Rewrap(found) = &pass {
                #[cfg(test)]
                REPLAY_STATS.with(|c| c.borrow_mut().early_with_back += 1);
                merge_early(&mut early, found.clone());
                rewraps += 1;
            }
            if let Pass::Again(t) = pass {
                #[cfg(test)]
                REPLAY_STATS.with(|c| c.borrow_mut().totals_with_back += 1);
                totals = t;
                passes += 1;
            }
            continue;
        }
        match pass {
            Pass::Again(t) => {
                #[cfg(test)]
                REPLAY_STATS.with(|c| c.borrow_mut().total_retries += 1);
                totals = t;
                passes += 1;
            }
            Pass::FloatReplay => continue,
            Pass::Rewrap(found) => {
                #[cfg(test)]
                REPLAY_STATS.with(|c| c.borrow_mut().early_retries += 1);
                merge_early(&mut early, found);
                rewraps += 1;
            }
            Pass::Done(l) => {
                if per_page && note_passes < MAX_NOTE_PASSES {
                    // Number each page's footnotes from where the pass before placed them, until they stay.
                    let placed = footnote_pages(doc, &l.pages);
                    if placed != note_pages {
                        #[cfg(test)]
                        REPLAY_STATS.with(|c| c.borrow_mut().note_retries += 1);
                        note_pages = placed;
                        note_passes += 1;
                        continue;
                    }
                }
                break l;
            }
        }
    };
    l.ms = now_ms() - t0;
    l
}

#[cfg(test)]
thread_local! {
    /// How many wrap relays (#1374) the layouts on this thread made: tests check none happen
    /// without something new to wrap around.
    static WRAP_RELAYS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Pair each character with one bookmark object without rescanning text prefixes.
fn only_bookmarks(p: &Paragraph) -> bool {
    let mut objects = p.objects.iter();
    for c in p.text.chars() {
        if c != wordcraft_doc::para::OBJ || !matches!(objects.next(), Some(InlineObject::BookmarkStart { .. } | InlineObject::BookmarkEnd { .. })) {
            return false;
        }
    }

    objects.next().is_none()
}

fn empty_break_page(pb: &page::PageBuilder) -> bool {
    pb.after_break_mark
        && pb.at_top()
        && pb.notes.is_empty()
        && pb.pages.len() > 1
        && pb.pages.last().is_some_and(|p| p.items.len() == pb.page_items_start)
}

#[cfg(test)]
mod cell_hit_tests;

/// Whether a wrap area reaches into the reserved bottom-margin band.
fn footer_band_overlap(rect: Rect, sect: &SectionProps) -> bool {
    rect.bottom() > sect.page_h - sect.margin_bottom && rect.y < sect.page_h
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_header_clearance;

#[cfg(test)]
mod merge_flow_controls;
#[cfg(test)]
mod tests_section_marks;

#[cfg(test)]
mod break_overflow_controls;
#[derive(Clone, Debug, PartialEq)]
struct TableWrap {
    block: usize,
    page: usize,
    area: Rect,
}

#[derive(Default)]
struct FloatReplay {
    planned: Vec<TableWrap>,
    attempts: usize,
    disabled: bool,
}

const MAX_FLOAT_PASSES: usize = 3;

/// Seeded decoration stays between prepended paint and body items.
#[derive(Clone, Copy, Default)]
struct PagePrefix {
    behind: usize,
    decor: usize,
}

#[cfg(test)]
#[path = "tests/alignment_prefix.rs"]
mod alignment_prefix_tests;
#[cfg(test)]
mod tests_blank_pages;
// A corrected frame reaching earlier body text is supplied before that text on the next pass.
type BackBodyWraps = BTreeMap<(usize, usize), (usize, WrapArea)>;
type PriorBodyWraps = HashMap<usize, Vec<(usize, WrapArea)>>;
const MAX_BACK_WRAP_PASSES: usize = 4;

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct ReplayStats {
    passes: usize,
    back_retries: usize,
    total_retries: usize,
    early_retries: usize,
    early_with_back: usize,
    totals_with_back: usize,
    note_retries: usize,
    cap_fallbacks: usize,
    wrap_attempts: usize,
    wrap_accepted: usize,
    wrap_rejected: usize,
}

#[cfg(test)]
thread_local! {
    static REPLAY_STATS: std::cell::RefCell<ReplayStats> = const { std::cell::RefCell::new(ReplayStats {
        passes: 0, back_retries: 0, total_retries: 0, early_retries: 0, early_with_back: 0, totals_with_back: 0, note_retries: 0,
        cap_fallbacks: 0, wrap_attempts: 0, wrap_accepted: 0, wrap_rejected: 0,
    }) };
}

/// Per-paragraph replay policy travels with its geometry, including through continuation.
#[derive(Clone, Debug, Default, PartialEq)]
struct BodyExclusions {
    areas: Vec<para::Exclusion>,
    split: Vec<usize>,
}

impl BodyExclusions {
    fn wait(areas: Vec<para::Exclusion>) -> Self {
        Self { areas, split: Vec::new() }
    }

    fn append_wait(&mut self, areas: impl IntoIterator<Item = para::Exclusion>) {
        self.areas.extend(areas);
    }

    fn append_split(&mut self, areas: impl IntoIterator<Item = para::Exclusion>) {
        for area in areas {
            self.split.push(self.areas.len());
            self.areas.push(area);
        }
    }
}

fn accept_wrap_relay(
    pl: &mut Arc<ParaLayout>,
    rel: &mut BodyExclusions,
    relaid: Arc<ParaLayout>,
    here: BodyExclusions,
    resume: usize,
    top: f32,
) -> Option<usize> {
    let k = relaid.lines.iter().position(|l| l.start == resume)?;
    if !relaid.lines.get(k).is_some_and(|l| (l.top - top).abs() < 0.01) {
        return None;
    }
    (*pl, *rel) = (relaid, here);
    Some(k)
}
#[cfg(test)]
use floats::continued_exclusions;

struct ReplayInput<'a> {
    floats: &'a mut FloatReplay,
    prior: &'a PriorBodyWraps,
    found: &'a mut BackBodyWraps,
    early: &'a EarlyFloats,
    may_rewrap: bool,
}
