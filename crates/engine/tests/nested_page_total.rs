//! Synthetic nested fields exercise import, layout and real DOCX serialization.
use std::io::{Cursor, Read, Write};

use serde_json::json;
use wordcraft_doc::{Document, InlineObject, Paragraph, PartKind, Path, Pos, StoryRef, para_block};
use wordcraft_engine::Session;
use wordcraft_layout::{DocLayout, LayoutCache, LayoutOptions, Placed};

const CACHED_TOTAL: &str = "26";
const CHILD_CACHE: &str = "29";

enum Cache {
    Text,
    Instruction,
}
enum Locks {
    Open,
    Parent,
    Child,
}

fn open_formula(before: &str, after: &str, cache: Cache, locks: Locks) -> Document {
    let parent_lock = if matches!(locks, Locks::Parent) { r#" w:fldLock="1""# } else { "" };
    let child_lock = if matches!(locks, Locks::Child) { r#" w:fldLock="1""# } else { "" };
    let cache_tag = if matches!(cache, Cache::Instruction) { "instrText" } else { "t" };
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:fldChar w:fldCharType="begin" w:dirty="true"{parent_lock}/></w:r>
<w:r><w:instrText xml:space="preserve">{before}</w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="begin" w:dirty="true"{child_lock}/></w:r>
<w:r><w:instrText>NUMPAGES</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>
<w:r><w:rPr><w:b/><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="20"/></w:rPr><w:{cache_tag}>{CHILD_CACHE}</w:{cache_tag}></w:r>
<w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText xml:space="preserve">{after}</w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{CACHED_TOTAL}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Second page</w:t></w:r></w:p></w:body></w:document>"#
    );
    read_document(&xml)
}

fn read_document(xml: &str) -> Document {
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in [("_rels/.rels", rels), ("word/document.xml", xml)] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    wordcraft_docx::read(&zip.finish().unwrap().into_inner()).unwrap()
}

fn first_field(doc: &Document) -> &InlineObject {
    &doc.para(StoryRef::Body, &Path::top(0)).unwrap().objects[0]
}

fn document_xml(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

fn shown(layout: &DocLayout) -> Vec<String> {
    layout
        .pages
        .iter()
        .flat_map(|page| page.items.iter().chain(&page.footer))
        .filter_map(
            |item| {
                if let Placed::Lines { para, .. } = item { Some(para.shown.iter().map(|x| x.1.as_str()).collect::<String>()) } else { None }
            },
        )
        .filter(|text| !text.is_empty())
        .collect()
}

#[test]
fn roundtrip_keeps_nested_total() {
    for locks in [Locks::Open, Locks::Parent, Locks::Child] {
        let mut doc = open_formula("= ", " - 3", Cache::Text, locks);
        let original = serde_json::to_value(first_field(&doc)).unwrap();
        for _ in 0..2 {
            let bytes = wordcraft_docx::write(&doc).unwrap();
            let xml = document_xml(&bytes);
            assert_eq!(xml.matches("NUMPAGES").count(), 1, "{xml}");
            assert_eq!(xml.matches(r#"w:fldCharType="begin""#).count(), 2, "{xml}");
            assert!(xml.contains(CHILD_CACHE) && xml.contains(CACHED_TOTAL));
            doc = wordcraft_docx::read(&bytes).unwrap();
            assert_eq!(serde_json::to_value(first_field(&doc)).unwrap(), original);
        }
    }
}

#[test]
fn instruction_cache_is_kept() {
    let doc = open_formula("= ", " - 3", Cache::Instruction, Locks::Open);
    let saved = document_xml(&wordcraft_docx::write(&doc).unwrap());
    assert!(saved.contains("NUMPAGES") && saved.contains(CHILD_CACHE), "{saved}");
    assert_eq!(doc.plain_text(StoryRef::Body).lines().next(), Some(CACHED_TOTAL));
}

#[test]
fn nested_totals_follow_pages() {
    let mut doc = open_formula("= ", " - 3", Cache::Text, Locks::Open);
    let mut footer = Paragraph::new();
    footer.insert_object(0, first_field(&doc).clone(), &Default::default()).unwrap();
    let id = doc.add_part(PartKind::Footer, vec![para_block(footer)]);
    doc.last_section.footers.default = Some(id);
    let mut cache = LayoutCache::new();
    let first = wordcraft_layout::layout(&doc, &mut cache, &LayoutOptions::default());
    assert_eq!(first.pages.len(), 2);
    assert_eq!(shown(&first), ["-1", "-1", "-1"]);
    let mut third = Paragraph::with_text("Third page", Default::default());
    third.props.page_break_before = Some(true);
    doc.body.push(para_block(third));
    let next = wordcraft_layout::layout(&doc, &mut cache, &LayoutOptions::default());
    assert_eq!(next.pages.len(), 3);
    assert_eq!(shown(&next), ["0", "0", "0", "0"]);
}

#[test]
fn unsupported_nested_is_cached() {
    for (before, after) in [("IF ", " = 2"), ("= ", " * 3"), ("= ", " - 3 + 1"), ("= ", " - 3 extra")] {
        let doc = open_formula(before, after, Cache::Instruction, Locks::Open);
        let InlineObject::Field { instr, result, .. } = first_field(&doc) else { panic!("unsupported field was promoted") };
        assert_eq!(instr, format!("{before}{after}").trim());
        assert_eq!(result, CACHED_TOTAL);
    }
}

#[test]
fn locked_totals_keep_cache() {
    for locks in [Locks::Parent, Locks::Child] {
        let doc = open_formula("= ", " - 3", Cache::Text, locks);
        let result = wordcraft_layout::layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        assert_eq!(shown(&result), [CACHED_TOTAL]);
    }
}

#[test]
fn nested_fields_keep_controls() {
    let doc = open_formula("= ", " - 3", Cache::Text, Locks::Open);
    let mut session = Session::new(doc);
    session.sel = wordcraft_engine::Selection::caret(Pos::body(0, 0));
    let selected = session.run("fields.selected", &json!({})).unwrap();
    assert_eq!(selected.as_array().unwrap().len(), 1);
    session.run("fields.lock", &json!({})).unwrap();
    assert_eq!(session.run("fields.selected", &json!({})).unwrap()[0]["locked"], true);
    session.run("fields.unlock", &json!({})).unwrap();
    session.run("fields.toggleCode", &json!({})).unwrap();
    assert_eq!(session.run("fields.selected", &json!({})).unwrap()[0]["code"], true);
    session.run("fields.toggleCode", &json!({})).unwrap();
    session.run("fields.unlink", &json!({})).unwrap();
    assert_eq!(session.doc.para(StoryRef::Body, &Path::top(0)).unwrap().plain_text(), "-1");
}

fn pdf_text(bytes: &[u8]) -> Vec<String> {
    use hayro_interpret::font::Glyph;
    use hayro_interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask,
        interpret_page,
    };
    use hayro_syntax::Pdf;
    struct Ex(String);
    impl Device<'_> for Ex {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {}
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, g: &Glyph<'_>, _: kurbo::Affine, _: kurbo::Affine, _: &Paint<'_>, mode: &GlyphDrawMode) {
            // Synthetic bold fills and then strokes each glyph: one character, two draws.
            if matches!(mode, GlyphDrawMode::Stroke(_)) {
                return;
            }
            match g.as_unicode() {
                Some(hayro_cmap::BfString::Char(c)) => self.0.push(c),
                Some(hayro_cmap::BfString::String(s)) => self.0.push_str(&s),
                None => self.0.push('\u{FFFD}'),
            }
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }
    let pdf = Pdf::new(bytes.to_vec()).expect("parse");
    let cache = InterpreterCache::new();
    pdf.pages()
        .iter()
        .map(|page| {
            let mut ctx =
                Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &cache, pdf.xref(), InterpreterSettings::default());
            let mut ex = Ex(String::new());
            interpret_page(page, &mut ctx, &mut ex);
            ex.0
        })
        .collect()
}

#[test]
fn ref_keeps_nested_cache() {
    let mut doc = open_formula("= ", " - 3", Cache::Text, Locks::Open);
    let para = doc.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    para.insert_object(0, InlineObject::BookmarkStart { name: "Total".into() }, &Default::default()).unwrap();
    para.insert_object(para.len(), InlineObject::BookmarkEnd { name: "Total".into() }, &Default::default()).unwrap();
    let mut reference = Paragraph::new();
    reference
        .insert_object(
            0,
            InlineObject::Field {
                instr: "REF Total".into(),
                result: "Saved reference".into(),
                locked: false,
                dirty: false,
                code: false,
                form: String::new(),
            },
            &Default::default(),
        )
        .unwrap();
    doc.body.push(para_block(reference));
    let bytes = wordcraft_engine::io::save_bytes("probe.pdf", &doc).unwrap();
    assert!(pdf_text(&bytes).join("").contains("Saved reference"));
}

#[test]
fn code_views_keep_identity() {
    for per_field in [false, true] {
        for global in [false, true] {
            let mut session = Session::new(open_formula("= ", " - 3", Cache::Text, Locks::Open));
            session.sel = wordcraft_engine::Selection::caret(Pos::body(0, 0));
            if per_field {
                session.run("fields.toggleCode", &json!({})).unwrap();
            }
            let layout =
                wordcraft_layout::layout(&session.doc, &mut LayoutCache::new(), &LayoutOptions { field_codes: global, ..Default::default() });
            let text = shown(&layout).join("");
            if per_field != global {
                assert!(text.contains("NUMPAGES"), "{text}");
            } else {
                assert_eq!(text, "-1");
            }
        }
    }
}

#[test]
fn editing_code_keeps_field() {
    let mut session = Session::new(open_formula("= ", " - 3", Cache::Text, Locks::Open));
    session.sel = wordcraft_engine::Selection::caret(Pos::body(0, 0));
    session.run("fields.setCode", &json!({"code":"NUMPAGES"})).unwrap();
    let selected = session.run("fields.selected", &json!({})).unwrap();
    assert_eq!(selected[0]["instr"], "NUMPAGES");
    assert_eq!(selected.as_array().unwrap().len(), 1);
}

#[test]
fn simple_child_keeps_total() {
    let xml = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText>= </w:instrText></w:r>
<w:fldSimple w:instr="NUMPAGES" w:dirty="true"><w:r><w:rPr><w:b/></w:rPr><w:instrText>{CHILD_CACHE}</w:instrText></w:r></w:fldSimple>
<w:r><w:instrText> - 3</w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>
<w:r><w:t>{CACHED_TOTAL}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Second page</w:t></w:r></w:p></w:body></w:document>"#
    );
    let mut doc = read_document(&xml);
    let original = serde_json::to_value(first_field(&doc)).unwrap();
    for _ in 0..2 {
        let laid = wordcraft_layout::layout(&doc, &mut LayoutCache::new(), &LayoutOptions::default());
        assert_eq!(laid.pages.len(), 2);
        assert_eq!(shown(&laid), ["-1"]);
        let bytes = wordcraft_docx::write(&doc).unwrap();
        let saved = document_xml(&bytes);
        assert_eq!(saved.matches("NUMPAGES").count(), 1);
        assert!(saved.contains(CHILD_CACHE) && saved.contains(CACHED_TOTAL));
        doc = wordcraft_docx::read(&bytes).unwrap();
        assert_eq!(serde_json::to_value(first_field(&doc)).unwrap(), original);
    }
}
