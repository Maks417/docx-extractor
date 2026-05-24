use assert_cmd::Command;
use std::io::{Cursor, Write as _};
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

// ── DOCX builder helper ────────────────────────────────────────────────────────

const DOCUMENT_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#;

const DOCUMENT_FOOTER: &str = "</w:document>";

fn wrap_body(body_content: &str) -> String {
    format!("{DOCUMENT_HEADER}<w:body>{body_content}</w:body>{DOCUMENT_FOOTER}")
}

fn build_docx(document_xml: &str) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document_xml.as_bytes()).unwrap();
    zip.finish().unwrap().into_inner()
}

fn write_docx(document_xml: &str) -> NamedTempFile {
    let mut tmp = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    tmp.write_all(&build_docx(document_xml)).unwrap();
    tmp
}

fn run(tmp: &NamedTempFile) -> serde_json::Value {
    let output = Command::cargo_bin("docx-extractor")
        .unwrap()
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[test]
fn simple_paragraph() {
    let xml = wrap_body("<w:p><w:r><w:t>Hello World</w:t></w:r></w:p>");
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["type"], "paragraph");
    assert_eq!(sections[0]["text"], "Hello World");
}

#[test]
fn heading_detection() {
    let xml = wrap_body(
        r#"<w:p>
            <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
            <w:r><w:t>My Title</w:t></w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["type"], "heading");
    assert_eq!(sections[0]["level"], 1);
    assert_eq!(sections[0]["text"], "My Title");
}

#[test]
fn all_heading_levels() {
    let mut body = String::new();
    for i in 1..=9 {
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading{i}"/></w:pPr>
            <w:r><w:t>H{i}</w:t></w:r></w:p>"#
        ));
    }
    let xml = wrap_body(&body);
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 9);
    for (i, s) in sections.iter().enumerate() {
        assert_eq!(s["type"], "heading");
        assert_eq!(s["level"], (i + 1) as u64);
    }
}

#[test]
fn simple_table() {
    let xml = wrap_body(
        r#"<w:tbl>
            <w:tr>
                <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
                <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
            </w:tr>
            <w:tr>
                <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
                <w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc>
            </w:tr>
        </w:tbl>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["type"], "table");
    let rows = sections[0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0]["text"], "A");
    assert_eq!(rows[0][1]["text"], "B");
    assert_eq!(rows[1][0]["text"], "C");
    assert_eq!(rows[1][1]["text"], "D");
}

#[test]
fn tracked_deletion_recorded_in_revisions() {
    // Deleted text is NOT in body text, but IS surfaced in revisions[].
    let xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>Keep</w:t></w:r>
            <w:del w:author="Jane" w:date="2024-01-15T10:30:00Z">
                <w:r><w:delText>Remove</w:delText></w:r>
            </w:del>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["text"], "Keep");
    let revisions = doc["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["kind"], "delete");
    assert_eq!(revisions[0]["text"], "Remove");
    assert_eq!(revisions[0]["author"], "Jane");
    assert_eq!(revisions[0]["anchor"]["section_index"], 0);
}

#[test]
fn tracked_insertion_recorded_in_revisions() {
    // Inserted text IS in body, AND is surfaced in revisions[].
    let xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>Hello </w:t></w:r>
            <w:ins w:author="Jane" w:date="2024-01-15T10:30:00Z">
                <w:r><w:t>brave new </w:t></w:r>
            </w:ins>
            <w:r><w:t>world</w:t></w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections[0]["text"], "Hello brave new world");
    let revisions = doc["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["kind"], "insert");
    assert_eq!(revisions[0]["text"], "brave new ");
    assert_eq!(revisions[0]["author"], "Jane");
    assert_eq!(revisions[0]["anchor"]["section_index"], 0);
    assert_eq!(revisions[0]["anchor"]["char_start"], 6);
    assert_eq!(revisions[0]["anchor"]["char_end"], 16);
}

#[test]
fn empty_paragraphs_skipped() {
    let xml = wrap_body(
        "<w:p/><w:p><w:r><w:t>   </w:t></w:r></w:p><w:p><w:r><w:t>Real</w:t></w:r></w:p>",
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["text"], "Real");
}

#[test]
fn tab_and_linebreak_in_paragraph() {
    let xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>A</w:t><w:tab/><w:t>B</w:t><w:br/><w:t>C</w:t></w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections[0]["text"], "A\tB\nC");
}

#[test]
fn multiple_paragraphs_in_cell_joined() {
    let xml = wrap_body(
        r#"<w:tbl><w:tr><w:tc>
            <w:p><w:r><w:t>Line1</w:t></w:r></w:p>
            <w:p><w:r><w:t>Line2</w:t></w:r></w:p>
        </w:tc></w:tr></w:tbl>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections[0]["rows"][0][0]["text"], "Line1\nLine2");
}

#[test]
fn nested_table_content_flattened() {
    // Nested-table content is flattened into the outer cell text (rows joined
    // by '\n', cells within a row joined by ' | ').
    let xml = wrap_body(
        r#"<w:tbl>
            <w:tr>
                <w:tc>
                    <w:p><w:r><w:t>Outer</w:t></w:r></w:p>
                    <w:tbl>
                        <w:tr>
                            <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
                            <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
                        </w:tr>
                        <w:tr>
                            <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
                            <w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc>
                        </w:tr>
                    </w:tbl>
                </w:tc>
            </w:tr>
        </w:tbl>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["type"], "table");
    let cell = sections[0]["rows"][0][0]["text"].as_str().unwrap();
    assert!(cell.starts_with("Outer"), "got: {cell}");
    assert!(cell.contains("A | B"), "got: {cell}");
    assert!(cell.contains("C | D"), "got: {cell}");
}

#[test]
fn invalid_docx_exits_nonzero() {
    let mut tmp = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    tmp.write_all(b"this is not a zip file").unwrap();
    let output = Command::cargo_bin("docx-extractor")
        .unwrap()
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn missing_file_exits_nonzero() {
    let output = Command::cargo_bin("docx-extractor")
        .unwrap()
        .arg("/nonexistent/path/to/file.docx")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn pretty_flag_produces_formatted_json() {
    let xml = wrap_body("<w:p><w:r><w:t>Hi</w:t></w:r></w:p>");
    let tmp = write_docx(&xml);
    let output = Command::cargo_bin("docx-extractor")
        .unwrap()
        .arg("--pretty")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    // Pretty-printed JSON contains newlines and indentation.
    assert!(text.contains('\n'));
    assert!(text.contains("  "));
}

#[test]
fn source_field_is_filename() {
    let xml = wrap_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let source = doc["source"].as_str().unwrap();
    // Should be just the filename, not a full path.
    assert!(!source.contains('/') && !source.contains('\\'));
    assert!(source.ends_with(".docx"));
}

#[test]
fn localized_heading_via_outline_level() {
    // Non-English style name + outlineLvl should still emit a Heading.
    let xml = wrap_body(
        r#"<w:p>
            <w:pPr>
                <w:pStyle w:val="Titre1"/>
                <w:outlineLvl w:val="0"/>
            </w:pPr>
            <w:r><w:t>French Heading</w:t></w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["type"], "heading");
    assert_eq!(sections[0]["level"], 1); // outlineLvl 0 → level 1
    assert_eq!(sections[0]["text"], "French Heading");
}

#[test]
fn metadata_extracted_from_core_xml() {
    let core_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<cp:coreProperties
  xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
  xmlns:dc="http://purl.org/dc/elements/1.1/"
  xmlns:dcterms="http://purl.org/dc/terms/"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>My Report</dc:title>
  <dc:creator>Jane Smith</dc:creator>
  <dcterms:created xsi:type="dcterms:W3CDTF">2024-01-15T10:30:00Z</dcterms:created>
</cp:coreProperties>"#;

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(wrap_body("<w:p/>").as_bytes()).unwrap();
    zip.start_file("docProps/core.xml", opts).unwrap();
    zip.write_all(core_xml.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let mut tmp = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    tmp.write_all(&bytes).unwrap();

    let doc = run(&tmp);
    assert_eq!(doc["metadata"]["title"], "My Report");
    assert_eq!(doc["metadata"]["author"], "Jane Smith");
    assert_eq!(doc["metadata"]["created"], "2024-01-15T10:30:00Z");
    assert!(doc["metadata"]["modified"].is_null());
}

#[test]
fn text_box_content_extracted() {
    // A text box: <w:pict><v:textbox><w:txbxContent> … </w:txbxContent></v:textbox></w:pict>
    let xml = wrap_body(
        r#"<w:p>
            <w:r>
                <w:pict>
                    <v:shape>
                        <v:textbox>
                            <w:txbxContent>
                                <w:p><w:r><w:t>Box text</w:t></w:r></w:p>
                            </w:txbxContent>
                        </v:textbox>
                    </v:shape>
                </w:pict>
            </w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    let texts: Vec<&str> = sections
        .iter()
        .map(|s| s["text"].as_str().unwrap())
        .collect();
    assert!(
        texts.contains(&"Box text"),
        "text box content not found: {texts:?}"
    );
}

#[test]
fn list_items_detected() {
    let xml = wrap_body(
        r#"<w:p>
            <w:pPr>
                <w:numPr>
                    <w:ilvl w:val="0"/>
                    <w:numId w:val="1"/>
                </w:numPr>
            </w:pPr>
            <w:r><w:t>Item A</w:t></w:r>
        </w:p>
        <w:p>
            <w:pPr>
                <w:numPr>
                    <w:ilvl w:val="1"/>
                    <w:numId w:val="1"/>
                </w:numPr>
            </w:pPr>
            <w:r><w:t>Nested Item</w:t></w:r>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0]["type"], "list_item");
    assert_eq!(sections[0]["level"], 0);
    assert_eq!(sections[0]["text"], "Item A");
    assert_eq!(sections[1]["type"], "list_item");
    assert_eq!(sections[1]["level"], 1);
    assert_eq!(sections[1]["text"], "Nested Item");
}

#[test]
fn hyperlink_url_preserved() {
    // Build a DOCX with a .rels file mapping rId1 → https://example.com, and a
    // hyperlink element referencing that relationship.
    let rels_xml = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com" TargetMode="External"/>
</Relationships>"#;

    let doc_xml = wrap_body(
        r#"<w:p>
            <w:hyperlink r:id="rId1"
              xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
              <w:r><w:t>click here</w:t></w:r>
            </w:hyperlink>
        </w:p>"#,
    );

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(doc_xml.as_bytes()).unwrap();
    zip.start_file("word/_rels/document.xml.rels", opts)
        .unwrap();
    zip.write_all(rels_xml.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    let mut tmp = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    tmp.write_all(&bytes).unwrap();

    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    let text = sections[0]["text"].as_str().unwrap();
    assert_eq!(text, "[click here](https://example.com)");
}

// ── Multi-file DOCX builder for advanced features ──────────────────────────────

fn build_multifile_docx(files: &[(&str, &[u8])]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default();
    for (name, content) in files {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(content).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn write_multifile_docx(files: &[(&str, &[u8])]) -> NamedTempFile {
    let mut tmp = tempfile::Builder::new().suffix(".docx").tempfile().unwrap();
    tmp.write_all(&build_multifile_docx(files)).unwrap();
    tmp
}

#[test]
fn image_attached_to_paragraph() {
    // 1x1 transparent PNG.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let rels_xml = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>
</Relationships>"#;

    let doc_xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>Before</w:t></w:r>
            <w:r>
              <w:drawing xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                         xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                <a:blip r:embed="rId7"/>
              </w:drawing>
            </w:r>
            <w:r><w:t> after</w:t></w:r>
        </w:p>"#,
    );

    let tmp = write_multifile_docx(&[
        ("word/document.xml", doc_xml.as_bytes()),
        ("word/_rels/document.xml.rels", rels_xml.as_bytes()),
        ("word/media/image1.png", PNG),
    ]);

    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["text"], "Before after");
    let images_attached = sections[0]["images"].as_array().unwrap();
    assert_eq!(images_attached.len(), 1);
    assert_eq!(images_attached[0], "image1.png");
    // Top-level images[] still has the file.
    assert_eq!(doc["images"][0]["id"], "image1.png");
}

#[test]
fn footnote_extracted() {
    let footnotes_xml = r#"<?xml version="1.0"?>
<w:footnotes xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:footnote w:type="separator" w:id="0"><w:p><w:r><w:t>sep</w:t></w:r></w:p></w:footnote>
  <w:footnote w:id="1">
    <w:p><w:r><w:t>This is footnote one.</w:t></w:r></w:p>
  </w:footnote>
</w:footnotes>"#;

    let doc_xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>See note</w:t></w:r>
            <w:r><w:footnoteReference w:id="1"/></w:r>
            <w:r><w:t>.</w:t></w:r>
        </w:p>"#,
    );

    let tmp = write_multifile_docx(&[
        ("word/document.xml", doc_xml.as_bytes()),
        ("word/footnotes.xml", footnotes_xml.as_bytes()),
    ]);

    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections[0]["text"], "See note.");
    assert_eq!(sections[0]["footnote_refs"][0], 1);

    let footnotes = doc["footnotes"].as_array().unwrap();
    assert_eq!(footnotes.len(), 1);
    assert_eq!(footnotes[0]["id"], 1);
    assert_eq!(footnotes[0]["sections"][0]["text"], "This is footnote one.");
}

#[test]
fn header_extracted() {
    let header_xml = r#"<?xml version="1.0"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>Page header text</w:t></w:r></w:p>
</w:hdr>"#;

    let rels_xml = r#"<?xml version="1.0"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
</Relationships>"#;

    let doc_xml = format!(
        "{}<w:body>{}<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId10\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/></w:sectPr></w:body>{}",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
        "<w:p><w:r><w:t>Body</w:t></w:r></w:p>",
        "</w:document>"
    );

    let tmp = write_multifile_docx(&[
        ("word/document.xml", doc_xml.as_bytes()),
        ("word/_rels/document.xml.rels", rels_xml.as_bytes()),
        ("word/header1.xml", header_xml.as_bytes()),
    ]);

    let doc = run(&tmp);
    let headers = doc["headers"].as_array().unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0]["type"], "default");
    assert_eq!(headers[0]["sections"][0]["text"], "Page header text");
}

#[test]
fn comment_extracted_with_anchor() {
    let comments_xml = r#"<?xml version="1.0"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:comment w:id="0" w:author="Reviewer" w:date="2024-02-01T12:00:00Z">
    <w:p><w:r><w:t>Looks good.</w:t></w:r></w:p>
  </w:comment>
</w:comments>"#;

    let doc_xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>The </w:t></w:r>
            <w:commentRangeStart w:id="0"/>
            <w:r><w:t>quick brown</w:t></w:r>
            <w:commentRangeEnd w:id="0"/>
            <w:r><w:t> fox</w:t></w:r>
        </w:p>"#,
    );

    let tmp = write_multifile_docx(&[
        ("word/document.xml", doc_xml.as_bytes()),
        ("word/comments.xml", comments_xml.as_bytes()),
    ]);

    let doc = run(&tmp);
    let comments = doc["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["id"], 0);
    assert_eq!(comments[0]["author"], "Reviewer");
    assert_eq!(comments[0]["sections"][0]["text"], "Looks good.");
    let anchor = &comments[0]["anchor"];
    assert_eq!(anchor["section_index"], 0);
    assert_eq!(anchor["char_start"], 4);
    assert_eq!(anchor["char_end"], 15);
}
