//! Original header field control with a conflicting ordinary tab stop.

use std::io::{Read, Write};

fn header_field_docx() -> Vec<u8> {
    let document = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>Body</w:t></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="h"/><w:pgSz w:w="15840" w:h="12240"/><w:pgMar w:top="1152" w:right="1152" w:bottom="1152" w:left="1152" w:header="720"/></w:sectPr></w:body></w:document>"#;
    let header = r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/></w:tabs></w:pPr><w:r><w:t>Chapter</w:t></w:r><w:r><w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/></w:r><w:r><w:t xml:space="preserve">Page </w:t></w:r><w:fldSimple w:instr="PAGE"><w:r><w:t>16</w:t></w:r></w:fldSimple><w:r><w:t xml:space="preserve"> of </w:t></w:r><w:fldSimple w:instr="NUMPAGES"><w:r><w:t>16</w:t></w:r></w:fldSimple></w:p></w:hdr>"#;
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="h" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="d" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in [("_rels/.rels", root), ("word/_rels/document.xml.rels", rels), ("word/document.xml", document), ("word/header1.xml", header)]
    {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn alignment_tab_keeps_fields() {
    let document = wordcraft_docx::read(&header_field_docx()).unwrap();
    let saved = wordcraft_docx::write(&document).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
    let header = zip.file_names().find(|name| name.starts_with("word/header") && name.ends_with(".xml")).unwrap().to_owned();
    let mut xml = String::new();
    zip.by_name(&header).unwrap().read_to_string(&mut xml).unwrap();

    assert!(xml.contains(r#"<w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/>"#), "{xml}");
    assert!(xml.contains("PAGE") && xml.contains("NUMPAGES"), "{xml}");
    let reopened = wordcraft_docx::read(&saved).unwrap();
    let text = |doc: &wordcraft_doc::Document| {
        doc.parts
            .values()
            .filter(|part| part.kind == wordcraft_doc::PartKind::Header)
            .flat_map(|part| part.blocks.iter())
            .filter_map(|b| b.as_para())
            .map(|p| p.plain_text())
            .collect::<Vec<_>>()
    };
    assert_eq!(text(&document), text(&reopened));
}
