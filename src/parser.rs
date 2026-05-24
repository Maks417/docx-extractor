use std::collections::HashMap;
use std::io::Read;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use quick_xml::events::attributes::Attributes;
use quick_xml::events::Event;
use quick_xml::Reader;
use thiserror::Error;
use zip::ZipArchive;

use crate::output::{Document, Image, Metadata, Section};

/// Errors that can occur while extracting a DOCX file.
#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Not a valid DOCX file (missing word/document.xml)")]
    NotDocx,
}

/// Extract text, images, and metadata from the DOCX file at `path`.
pub fn extract(path: &str) -> Result<Document, ExtractError> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;

    if archive.by_name("word/document.xml").is_err() {
        return Err(ExtractError::NotDocx);
    }

    let rels = parse_relationships(&mut archive)?;
    let sections = parse_document(&mut archive, &rels.hyperlinks)?;
    let images = extract_images(&mut archive, &rels.images)?;
    let metadata = parse_core_props(&mut archive)?;

    let source = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    Ok(Document {
        source,
        metadata,
        sections,
        images,
    })
}

// ── Core properties (metadata) ────────────────────────────────────────────────

fn parse_core_props(
    archive: &mut ZipArchive<std::fs::File>,
) -> Result<Option<Metadata>, ExtractError> {
    let mut content = String::new();
    match archive.by_name("docProps/core.xml") {
        Ok(mut f) => f.read_to_string(&mut content)?,
        Err(_) => return Ok(None),
    };

    let mut meta = Metadata::default();
    let mut reader = Reader::from_str(&content);
    let mut buf = Vec::new();
    let mut current: Option<&'static str> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                current = match e.local_name().as_ref() {
                    b"title" => Some("title"),
                    b"creator" => Some("author"),
                    b"lastModifiedBy" => Some("last_modified_by"),
                    b"created" => Some("created"),
                    b"modified" => Some("modified"),
                    _ => None,
                };
            }
            Ok(Event::Text(ref e)) => {
                if let Some(field) = current {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if !text.is_empty() {
                        match field {
                            "title" => meta.title = Some(text),
                            "author" => meta.author = Some(text),
                            "last_modified_by" => meta.last_modified_by = Some(text),
                            "created" => meta.created = Some(text),
                            "modified" => meta.modified = Some(text),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Err(e) => return Err(ExtractError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    // Return None if the file exists but all fields are empty.
    let has_any = meta.title.is_some()
        || meta.author.is_some()
        || meta.last_modified_by.is_some()
        || meta.created.is_some()
        || meta.modified.is_some();

    Ok(if has_any { Some(meta) } else { None })
}

// ── Relationships ──────────────────────────────────────────────────────────────

struct Rels {
    images: HashMap<String, String>,
    hyperlinks: HashMap<String, String>,
}

fn parse_relationships(archive: &mut ZipArchive<std::fs::File>) -> Result<Rels, ExtractError> {
    let mut rels = Rels {
        images: HashMap::new(),
        hyperlinks: HashMap::new(),
    };

    let mut content = String::new();
    match archive.by_name("word/_rels/document.xml.rels") {
        Ok(mut f) => f.read_to_string(&mut content)?,
        Err(_) => return Ok(rels),
    };

    let mut reader = Reader::from_str(&content);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
                if e.local_name().as_ref() == b"Relationship" =>
            {
                let mut id: Option<String> = None;
                let mut target: Option<String> = None;
                let mut rel_type = String::new();

                for attr in e.attributes().flatten() {
                    match attr.key.local_name().as_ref() {
                        b"Id" => id = Some(lossy(&attr.value)),
                        b"Target" => target = Some(lossy(&attr.value)),
                        b"Type" => rel_type = lossy(&attr.value),
                        _ => {}
                    }
                }

                if let (Some(id), Some(target)) = (id, target) {
                    if rel_type.contains("image") {
                        // Target is relative to word/
                        rels.images.insert(id, format!("word/{target}"));
                    } else if rel_type.contains("hyperlink") {
                        rels.hyperlinks.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ExtractError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(rels)
}

// ── Document parser ────────────────────────────────────────────────────────────

/// Tracks which XML elements are currently open. The vec acts as a nesting
/// stack: Start pushes, End pops the rightmost matching tag.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Ctx {
    Body,
    Para,
    ParaProps,
    NumPr,
    Run,
    Text,
    Del,
    Tbl,
    Row,
    Cell,
}

fn has(ctx: &[Ctx], tag: Ctx) -> bool {
    ctx.contains(&tag)
}

fn table_depth(ctx: &[Ctx]) -> usize {
    ctx.iter().filter(|&&t| t == Ctx::Tbl).count()
}

fn pop_tag(ctx: &mut Vec<Ctx>, tag: Ctx) {
    if let Some(pos) = ctx.iter().rposition(|&t| t == tag) {
        ctx.remove(pos);
    }
}

pub fn parse_document(
    archive: &mut ZipArchive<std::fs::File>,
    hyperlink_rels: &HashMap<String, String>,
) -> Result<Vec<Section>, ExtractError> {
    let mut content = String::new();
    archive
        .by_name("word/document.xml")?
        .read_to_string(&mut content)?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(false);

    let mut sections: Vec<Section> = Vec::new();
    let mut buf = Vec::new();
    let mut ctx: Vec<Ctx> = Vec::new();

    let mut para_style: Option<String> = None;
    let mut para_outline_level: Option<u8> = None;
    let mut para_text = String::new();
    let mut para_is_list = false;
    let mut para_list_level: u8 = 0;
    let mut cell_text = String::new();

    // Text-box tracking: suspend/restore outer paragraph state while inside a
    // <w:txbxContent> so its paragraphs are processed independently.
    let mut txbx_depth: usize = 0;
    let mut saved_outer_para: Option<(String, Option<String>, bool, u8)> = None;
    let mut current_row: Vec<String> = Vec::new();
    let mut current_table: Vec<Vec<String>> = Vec::new();

    // Hyperlink tracking: URL of the currently open <w:hyperlink> and the
    // text-buffer offset where its content starts (so we can wrap it on close).
    let mut hyperlink_url: Option<String> = None;
    let mut hyperlink_text_start: usize = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.local_name().as_ref() {
                b"body" => ctx.push(Ctx::Body),

                b"p" if has(&ctx, Ctx::Body) => {
                    let td = table_depth(&ctx);
                    let in_cell = has(&ctx, Ctx::Cell);
                    ctx.push(Ctx::Para);
                    if td == 0 {
                        para_style = None;
                        para_outline_level = None;
                        para_text.clear();
                        para_is_list = false;
                        para_list_level = 0;
                    } else if td == 1 && in_cell && !cell_text.is_empty() {
                        // Separate paragraphs within a cell with a newline.
                        cell_text.push('\n');
                    }
                    // td > 1: paragraph inside a nested table — ignore.
                }

                b"pPr" if has(&ctx, Ctx::Para) || has(&ctx, Ctx::Cell) => {
                    ctx.push(Ctx::ParaProps);
                }
                b"numPr" if has(&ctx, Ctx::ParaProps) => ctx.push(Ctx::NumPr),

                b"r" if !has(&ctx, Ctx::Del)
                    && (table_depth(&ctx) == 0 && has(&ctx, Ctx::Para)
                        || table_depth(&ctx) == 1 && has(&ctx, Ctx::Cell)) =>
                {
                    ctx.push(Ctx::Run);
                }
                b"t" if has(&ctx, Ctx::Run) => ctx.push(Ctx::Text),

                b"tbl" if has(&ctx, Ctx::Body) => {
                    ctx.push(Ctx::Tbl);
                    if table_depth(&ctx) == 1 {
                        current_table.clear();
                    }
                }
                b"tr" if table_depth(&ctx) == 1 => {
                    ctx.push(Ctx::Row);
                    current_row.clear();
                }
                b"tc" if table_depth(&ctx) == 1 && has(&ctx, Ctx::Row) => {
                    ctx.push(Ctx::Cell);
                    cell_text.clear();
                }

                b"del" => ctx.push(Ctx::Del),

                b"txbxContent" if has(&ctx, Ctx::Body) => {
                    txbx_depth += 1;
                    if txbx_depth == 1 {
                        // Suspend the outer paragraph so text box paragraphs
                        // are processed as independent top-level sections.
                        saved_outer_para = Some((
                            std::mem::take(&mut para_text),
                            para_style.take(),
                            para_is_list,
                            para_list_level,
                        ));
                        para_is_list = false;
                        para_list_level = 0;
                    }
                }

                b"hyperlink" if has(&ctx, Ctx::Body) => {
                    // Resolve external URL via relationship ID, or use anchor as "#anchor".
                    let url = e.attributes().flatten().find_map(|attr| {
                        if attr.key.as_ref() == b"r:id" {
                            hyperlink_rels.get(lossy(&attr.value).as_str()).cloned()
                        } else if attr.key.local_name().as_ref() == b"anchor" {
                            Some(format!("#{}", lossy(&attr.value)))
                        } else {
                            None
                        }
                    });
                    if let Some(u) = url {
                        hyperlink_url = Some(u);
                        hyperlink_text_start = if table_depth(&ctx) == 0 {
                            para_text.len()
                        } else {
                            cell_text.len()
                        };
                    }
                }

                _ => {}
            },

            Ok(Event::Empty(ref e)) => match e.local_name().as_ref() {
                b"pStyle" if has(&ctx, Ctx::ParaProps) => {
                    para_style = get_attr(e.attributes(), b"val");
                }
                b"outlineLvl" if has(&ctx, Ctx::ParaProps) => {
                    // Stores the outline depth (0 = top level) for localized heading styles.
                    if let Some(v) = get_attr(e.attributes(), b"val") {
                        para_outline_level = v.parse::<u8>().ok().filter(|&l| l <= 8);
                    }
                }
                b"ilvl" if has(&ctx, Ctx::NumPr) => {
                    if let Some(v) = get_attr(e.attributes(), b"val") {
                        para_list_level = v.parse().unwrap_or(0);
                    }
                }
                b"numId" if has(&ctx, Ctx::NumPr) => {
                    // numId 0 means "remove list formatting" — not a real list item.
                    if get_attr(e.attributes(), b"val").as_deref() != Some("0") {
                        para_is_list = true;
                    }
                }
                b"tab" if has(&ctx, Ctx::Run) && !has(&ctx, Ctx::Del) => {
                    if table_depth(&ctx) == 0 {
                        para_text.push('\t');
                    } else if has(&ctx, Ctx::Cell) {
                        cell_text.push('\t');
                    }
                }
                b"br" if has(&ctx, Ctx::Run) && !has(&ctx, Ctx::Del) => {
                    if table_depth(&ctx) == 0 {
                        para_text.push('\n');
                    } else if has(&ctx, Ctx::Cell) {
                        cell_text.push(' ');
                    }
                }
                _ => {}
            },

            Ok(Event::End(ref e)) => match e.local_name().as_ref() {
                b"body" => pop_tag(&mut ctx, Ctx::Body),
                b"pPr" => pop_tag(&mut ctx, Ctx::ParaProps),
                b"numPr" => pop_tag(&mut ctx, Ctx::NumPr),
                b"t" => pop_tag(&mut ctx, Ctx::Text),
                b"r" => pop_tag(&mut ctx, Ctx::Run),
                b"del" => pop_tag(&mut ctx, Ctx::Del),

                b"p" if has(&ctx, Ctx::Body) && has(&ctx, Ctx::Para) => {
                    let td = table_depth(&ctx);
                    pop_tag(&mut ctx, Ctx::Para);
                    if td == 0 {
                        let text = para_text.trim().to_string();
                        if !text.is_empty() {
                            if para_is_list {
                                sections.push(Section::ListItem {
                                    level: para_list_level,
                                    text,
                                });
                            } else {
                                // Try English style name first, then outlineLvl fallback.
                                let level = para_style
                                    .as_deref()
                                    .and_then(heading_level)
                                    .or_else(|| para_outline_level.map(|l| l + 1));
                                match level {
                                    Some(level) => sections.push(Section::Heading { level, text }),
                                    None => sections.push(Section::Paragraph { text }),
                                }
                            }
                        }
                        para_style = None;
                        para_outline_level = None;
                        para_text.clear();
                        para_is_list = false;
                        para_list_level = 0;
                    }
                }

                b"tc" if table_depth(&ctx) == 1 && has(&ctx, Ctx::Row) => {
                    pop_tag(&mut ctx, Ctx::Cell);
                    current_row.push(cell_text.trim().to_string());
                    cell_text.clear();
                }
                b"tr" if table_depth(&ctx) == 1 => {
                    pop_tag(&mut ctx, Ctx::Row);
                    if !current_row.is_empty() {
                        current_table.push(std::mem::take(&mut current_row));
                    }
                }
                b"tbl" => {
                    let td = table_depth(&ctx);
                    pop_tag(&mut ctx, Ctx::Tbl);
                    if td == 1 && !current_table.is_empty() {
                        sections.push(Section::Table {
                            rows: std::mem::take(&mut current_table),
                        });
                    }
                }

                b"txbxContent" if has(&ctx, Ctx::Body) => {
                    if txbx_depth == 1 {
                        if let Some((saved_text, saved_style, saved_is_list, saved_level)) =
                            saved_outer_para.take()
                        {
                            para_text = saved_text;
                            para_style = saved_style;
                            para_is_list = saved_is_list;
                            para_list_level = saved_level;
                        }
                    }
                    txbx_depth = txbx_depth.saturating_sub(1);
                }

                b"hyperlink" if has(&ctx, Ctx::Body) => {
                    if let Some(url) = hyperlink_url.take() {
                        // Wrap the text accumulated since the hyperlink opened.
                        let buf = if table_depth(&ctx) == 0 {
                            &mut para_text
                        } else {
                            &mut cell_text
                        };
                        let link_text = buf[hyperlink_text_start..].to_string();
                        if !link_text.is_empty() {
                            buf.truncate(hyperlink_text_start);
                            buf.push_str(&format!("[{link_text}]({url})"));
                        }
                    }
                }

                _ => {}
            },

            Ok(Event::Text(ref e)) if has(&ctx, Ctx::Text) => {
                let text = e.unescape().unwrap_or_default();
                if table_depth(&ctx) == 0 {
                    para_text.push_str(&text);
                } else {
                    cell_text.push_str(&text);
                }
            }

            Ok(Event::Eof) => break,
            Err(e) => return Err(ExtractError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(sections)
}

// ── Image extraction ───────────────────────────────────────────────────────────

fn extract_images(
    archive: &mut ZipArchive<std::fs::File>,
    rels: &HashMap<String, String>,
) -> Result<Vec<Image>, ExtractError> {
    let mut images = Vec::new();

    // Collect paths first to avoid borrow-while-iterating issues
    let entries: Vec<(String, String)> = rels
        .iter()
        .filter_map(|(_id, path)| {
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let mime = match ext.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                "tiff" | "tif" => "image/tiff",
                "webp" => "image/webp",
                _ => {
                    eprintln!("warning: skipping unsupported image format: {path}");
                    return None;
                }
            };
            Some((path.clone(), mime.to_string()))
        })
        .collect();

    for (path, mime_type) in entries {
        // Reject paths that could escape the archive root.
        if path.contains("..") || path.starts_with('/') || path.starts_with('\\') {
            eprintln!("warning: skipping image with unsafe path: {path}");
            continue;
        }

        const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

        let mut bytes = Vec::new();
        match archive.by_name(&path) {
            Ok(mut f) => {
                if f.size() > MAX_IMAGE_BYTES {
                    eprintln!(
                        "warning: skipping image larger than 10 MB: {path} ({} bytes)",
                        f.size()
                    );
                    continue;
                }
                f.read_to_end(&mut bytes)?;
            }
            Err(_) => {
                eprintln!("warning: image not found in archive: {path}");
                continue;
            }
        };

        let id = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path)
            .to_string();

        images.push(Image {
            id,
            mime_type,
            base64: BASE64.encode(&bytes),
        });
    }

    Ok(images)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn get_attr(attrs: Attributes<'_>, key: &[u8]) -> Option<String> {
    attrs.flatten().find_map(|attr| {
        if attr.key.local_name().as_ref() == key {
            Some(lossy(&attr.value))
        } else {
            None
        }
    })
}

fn lossy(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            eprintln!("warning: non-UTF-8 bytes in XML attribute; replacement characters inserted");
            String::from_utf8_lossy(bytes).into_owned()
        }
    }
}

fn heading_level(style: &str) -> Option<u8> {
    let lower = style.trim().to_lowercase();
    lower
        .strip_prefix("heading")
        .and_then(|rest| {
            rest.trim_start_matches(|c: char| !c.is_ascii_digit())
                .parse::<u8>()
                .ok()
        })
        .filter(|&l| (1..=9).contains(&l))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // heading_level unit tests

    #[test]
    fn heading_level_valid_range() {
        for i in 1u8..=9 {
            assert_eq!(heading_level(&format!("Heading{i}")), Some(i));
            assert_eq!(heading_level(&format!("heading{i}")), Some(i));
            assert_eq!(heading_level(&format!("HEADING{i}")), Some(i));
        }
    }

    #[test]
    fn heading_level_zero_rejected() {
        assert_eq!(heading_level("Heading0"), None);
    }

    #[test]
    fn heading_level_ten_rejected() {
        assert_eq!(heading_level("Heading10"), None);
    }

    #[test]
    fn heading_level_non_heading_styles() {
        assert_eq!(heading_level("Normal"), None);
        assert_eq!(heading_level(""), None);
        assert_eq!(heading_level("Body Text"), None);
        assert_eq!(heading_level("Title"), None);
    }

    #[test]
    fn heading_level_trims_whitespace() {
        assert_eq!(heading_level("  Heading1  "), Some(1));
    }

    // lossy unit tests

    #[test]
    fn lossy_valid_utf8_is_unchanged() {
        assert_eq!(lossy(b"hello world"), "hello world");
    }

    #[test]
    fn lossy_invalid_utf8_returns_replacement() {
        let result = lossy(&[0xFF, 0xFE, 0x41]); // 0xFF 0xFE are invalid, 'A' is valid
        assert!(result.contains('\u{FFFD}'));
        assert!(result.ends_with('A'));
    }

    #[test]
    fn lossy_empty_is_empty() {
        assert_eq!(lossy(b""), "");
    }
}
