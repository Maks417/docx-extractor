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
    assert_eq!(rows[0][0], "A");
    assert_eq!(rows[0][1], "B");
    assert_eq!(rows[1][0], "C");
    assert_eq!(rows[1][1], "D");
}

#[test]
fn tracked_deletion_skipped() {
    let xml = wrap_body(
        r#"<w:p>
            <w:r><w:t>Keep</w:t></w:r>
            <w:del><w:r><w:delText>Remove</w:delText></w:r></w:del>
        </w:p>"#,
    );
    let tmp = write_docx(&xml);
    let doc = run(&tmp);
    let sections = doc["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["text"], "Keep");
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
    assert_eq!(sections[0]["rows"][0][0], "Line1\nLine2");
}

#[test]
fn nested_table_content_dropped() {
    // Only the outer table should appear; the inner table's cell text is dropped.
    let xml = wrap_body(
        r#"<w:tbl>
            <w:tr>
                <w:tc>
                    <w:p><w:r><w:t>Outer</w:t></w:r></w:p>
                    <w:tbl>
                        <w:tr><w:tc><w:p><w:r><w:t>Inner</w:t></w:r></w:p></w:tc></w:tr>
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
    // Inner table content not included in the cell
    assert_eq!(sections[0]["rows"][0][0], "Outer");
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
