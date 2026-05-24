use std::collections::HashMap;
use std::io::Read;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use quick_xml::events::attributes::Attributes;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use thiserror::Error;
use zip::ZipArchive;

use crate::output::{
    Anchor, Comment, Document, HeaderFooter, Image, Metadata, Note, Revision, Section, TableCell,
};

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

pub fn extract(path: &str) -> Result<Document, ExtractError> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;

    if archive.by_name("word/document.xml").is_err() {
        return Err(ExtractError::NotDocx);
    }

    let rels = parse_relationships(&mut archive)?;

    // Parse the main document body.
    let mut doc_xml = String::new();
    archive
        .by_name("word/document.xml")?
        .read_to_string(&mut doc_xml)?;
    let body = parse_body(&doc_xml, &rels);

    // Headers & footers — collected via references found while parsing the body.
    let headers = parse_header_footer_parts(&mut archive, &rels, &body.header_refs, "hdr")?;
    let footers = parse_header_footer_parts(&mut archive, &rels, &body.footer_refs, "ftr")?;

    // Footnotes & endnotes.
    let footnotes = parse_notes_file(&mut archive, "word/footnotes.xml", b"footnote", &rels)?;
    let endnotes = parse_notes_file(&mut archive, "word/endnotes.xml", b"endnote", &rels)?;

    // Comments — bodies from comments.xml, anchors from the main body parse.
    let comments = parse_comments_file(&mut archive, &rels, &body.comment_anchors)?;

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
        sections: body.sections,
        headers,
        footers,
        footnotes,
        endnotes,
        comments,
        revisions: body.revisions,
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

    let has_any = meta.title.is_some()
        || meta.author.is_some()
        || meta.last_modified_by.is_some()
        || meta.created.is_some()
        || meta.modified.is_some();

    Ok(if has_any { Some(meta) } else { None })
}

// ── Relationships ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Rels {
    pub images: HashMap<String, String>,
    pub hyperlinks: HashMap<String, String>,
    pub headers: HashMap<String, String>, // rId → "word/header1.xml"
    pub footers: HashMap<String, String>,
}

fn parse_relationships(archive: &mut ZipArchive<std::fs::File>) -> Result<Rels, ExtractError> {
    let mut rels = Rels::default();

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
                        rels.images.insert(id, format!("word/{target}"));
                    } else if rel_type.contains("hyperlink") {
                        rels.hyperlinks.insert(id, target);
                    } else if rel_type.ends_with("/header") {
                        rels.headers.insert(id, format!("word/{target}"));
                    } else if rel_type.ends_with("/footer") {
                        rels.footers.insert(id, format!("word/{target}"));
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

// ── Body parser ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
enum Ctx {
    Body,
    Para,
    ParaProps,
    NumPr,
    Run,
    Text,
    Ins,
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

#[derive(Debug, Default)]
pub struct ParsedBody {
    pub sections: Vec<Section>,
    pub revisions: Vec<Revision>,
    pub comment_anchors: HashMap<u32, Anchor>,
    pub header_refs: Vec<(String, String)>, // (type, rId)
    pub footer_refs: Vec<(String, String)>,
}

struct SavedPara {
    text: String,
    style: Option<String>,
    is_list: bool,
    list_level: u8,
    images: Vec<String>,
    footnote_refs: Vec<u32>,
    endnote_refs: Vec<u32>,
}

struct InsState {
    author: Option<String>,
    date: Option<String>,
    start_pos: usize,
    in_table: bool,
}

struct DelState {
    author: Option<String>,
    date: Option<String>,
    buf: String,
    in_table: bool,
}

struct CommentStart {
    section_index: usize, // prospective index for current pending paragraph
    char_start: usize,
    in_table: bool,
    finalized_end: Option<usize>, // set when the start paragraph is emitted, if range still open
}

/// Parse a "body" XML chunk — recognizes `<w:body>`, `<w:hdr>`, `<w:ftr>`,
/// `<w:footnote>`, `<w:endnote>`, or `<w:comment>` as the body container.
pub fn parse_body(xml: &str, rels: &Rels) -> ParsedBody {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    let mut out = ParsedBody::default();
    let mut ctx: Vec<Ctx> = Vec::new();

    // Paragraph state.
    let mut para_style: Option<String> = None;
    let mut para_outline_level: Option<u8> = None;
    let mut para_text = String::new();
    let mut para_images: Vec<String> = Vec::new();
    let mut para_footnote_refs: Vec<u32> = Vec::new();
    let mut para_endnote_refs: Vec<u32> = Vec::new();
    let mut para_is_list = false;
    let mut para_list_level: u8 = 0;

    // Cell state.
    let mut cell_text = String::new();
    let mut cell_images: Vec<String> = Vec::new();
    let mut current_row: Vec<TableCell> = Vec::new();
    let mut current_table: Vec<Vec<TableCell>> = Vec::new();

    // Text-box state.
    let mut txbx_depth: usize = 0;
    let mut saved_outer_para: Option<SavedPara> = None;

    // Hyperlink state.
    let mut hyperlink_url: Option<String> = None;
    let mut hyperlink_text_start: usize = 0;

    // Tracked-changes state.
    let mut ins_stack: Vec<InsState> = Vec::new();
    let mut del_stack: Vec<DelState> = Vec::new();

    // Comment ranges state.
    let mut open_comment_starts: HashMap<u32, CommentStart> = HashMap::new();

    // Nested-table flatten state — content captured at table_depth >= 2.
    let mut nested_cell = String::new();
    let mut nested_row: Vec<String> = Vec::new();
    let mut nested_rows: Vec<Vec<String>> = Vec::new();

    while let Ok(event) = reader.read_event_into(&mut buf) {
        match event {
            Event::Start(ref e) => match e.local_name().as_ref() {
                b"body" | b"hdr" | b"ftr" | b"footnote" | b"endnote" | b"comment"
                    if !has(&ctx, Ctx::Body) =>
                {
                    ctx.push(Ctx::Body);
                }

                b"p" if has(&ctx, Ctx::Body) => {
                    let td = table_depth(&ctx);
                    let in_cell = has(&ctx, Ctx::Cell);
                    ctx.push(Ctx::Para);
                    if td == 0 {
                        para_style = None;
                        para_outline_level = None;
                        para_text.clear();
                        para_images.clear();
                        para_footnote_refs.clear();
                        para_endnote_refs.clear();
                        para_is_list = false;
                        para_list_level = 0;
                    } else if td == 1 && in_cell && !cell_text.is_empty() {
                        cell_text.push('\n');
                    } else if td >= 2 {
                        // Nested paragraph — append newline separator to current nested cell.
                        if !nested_cell.is_empty() && !nested_cell.ends_with('\n') {
                            nested_cell.push('\n');
                        }
                    }
                }

                b"pPr" if has(&ctx, Ctx::Para) || has(&ctx, Ctx::Cell) => {
                    ctx.push(Ctx::ParaProps);
                }
                b"numPr" if has(&ctx, Ctx::ParaProps) => ctx.push(Ctx::NumPr),

                b"r" if (table_depth(&ctx) == 0 && has(&ctx, Ctx::Para)
                    || table_depth(&ctx) == 1 && has(&ctx, Ctx::Cell)
                    || table_depth(&ctx) >= 2 && has(&ctx, Ctx::Body)
                    || has(&ctx, Ctx::Del)) =>
                {
                    ctx.push(Ctx::Run);
                }
                b"t" if has(&ctx, Ctx::Run) => ctx.push(Ctx::Text),
                b"delText" if has(&ctx, Ctx::Run) && has(&ctx, Ctx::Del) => ctx.push(Ctx::Text),

                b"tbl" if has(&ctx, Ctx::Body) => {
                    ctx.push(Ctx::Tbl);
                    let depth = table_depth(&ctx);
                    if depth == 1 {
                        current_table.clear();
                    } else if depth == 2 {
                        nested_cell.clear();
                        nested_row.clear();
                        nested_rows.clear();
                    }
                }
                b"tr" => {
                    let depth = table_depth(&ctx);
                    if depth == 1 {
                        ctx.push(Ctx::Row);
                        current_row.clear();
                    } else if depth >= 2 {
                        ctx.push(Ctx::Row);
                    }
                }
                b"tc" => {
                    let depth = table_depth(&ctx);
                    if depth == 1 && has(&ctx, Ctx::Row) {
                        ctx.push(Ctx::Cell);
                        cell_text.clear();
                        cell_images.clear();
                    } else if depth >= 2 && has(&ctx, Ctx::Row) {
                        ctx.push(Ctx::Cell);
                        nested_cell.clear();
                    }
                }

                b"ins" => {
                    ctx.push(Ctx::Ins);
                    let in_table = table_depth(&ctx) >= 1;
                    let start_pos = if in_table {
                        cell_text.len()
                    } else {
                        para_text.len()
                    };
                    ins_stack.push(InsState {
                        author: get_attr(e.attributes(), b"author"),
                        date: get_attr(e.attributes(), b"date"),
                        start_pos,
                        in_table,
                    });
                }
                b"del" => {
                    ctx.push(Ctx::Del);
                    let in_table = table_depth(&ctx) >= 1;
                    del_stack.push(DelState {
                        author: get_attr(e.attributes(), b"author"),
                        date: get_attr(e.attributes(), b"date"),
                        buf: String::new(),
                        in_table,
                    });
                }

                b"txbxContent" if has(&ctx, Ctx::Body) => {
                    txbx_depth += 1;
                    if txbx_depth == 1 {
                        saved_outer_para = Some(SavedPara {
                            text: std::mem::take(&mut para_text),
                            style: para_style.take(),
                            is_list: para_is_list,
                            list_level: para_list_level,
                            images: std::mem::take(&mut para_images),
                            footnote_refs: std::mem::take(&mut para_footnote_refs),
                            endnote_refs: std::mem::take(&mut para_endnote_refs),
                        });
                        para_is_list = false;
                        para_list_level = 0;
                    }
                }

                b"hyperlink" if has(&ctx, Ctx::Body) => {
                    let url = e.attributes().flatten().find_map(|attr| {
                        if attr.key.as_ref() == b"r:id" {
                            rels.hyperlinks.get(lossy(&attr.value).as_str()).cloned()
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

            Event::Empty(ref e) => match e.local_name().as_ref() {
                b"pStyle" if has(&ctx, Ctx::ParaProps) => {
                    para_style = get_attr(e.attributes(), b"val");
                }
                b"outlineLvl" if has(&ctx, Ctx::ParaProps) => {
                    if let Some(v) = get_attr(e.attributes(), b"val") {
                        para_outline_level = v.parse::<u8>().ok().filter(|&l| l <= 8);
                    }
                }
                b"ilvl" if has(&ctx, Ctx::NumPr) => {
                    if let Some(v) = get_attr(e.attributes(), b"val") {
                        para_list_level = v.parse().unwrap_or(0);
                    }
                }
                b"numId"
                    if has(&ctx, Ctx::NumPr)
                        && get_attr(e.attributes(), b"val").as_deref() != Some("0") =>
                {
                    para_is_list = true;
                }
                b"tab" if has(&ctx, Ctx::Run) => {
                    push_text(
                        "\t",
                        &ctx,
                        &mut para_text,
                        &mut cell_text,
                        &mut nested_cell,
                        &mut del_stack,
                    );
                }
                b"br" if has(&ctx, Ctx::Run) => {
                    let s = if table_depth(&ctx) == 0 { "\n" } else { " " };
                    push_text(
                        s,
                        &ctx,
                        &mut para_text,
                        &mut cell_text,
                        &mut nested_cell,
                        &mut del_stack,
                    );
                }

                b"blip" if has(&ctx, Ctx::Run) => {
                    if let Some(name) = image_filename_for(e, rels) {
                        add_image_ref(&ctx, &mut para_images, &mut cell_images, name);
                    }
                }
                b"imagedata" if has(&ctx, Ctx::Run) => {
                    if let Some(name) = image_filename_for(e, rels) {
                        add_image_ref(&ctx, &mut para_images, &mut cell_images, name);
                    }
                }

                b"footnoteReference" if has(&ctx, Ctx::Run) && table_depth(&ctx) == 0 => {
                    if let Some(id) = get_attr(e.attributes(), b"id").and_then(|v| v.parse().ok()) {
                        para_footnote_refs.push(id);
                    }
                }
                b"endnoteReference" if has(&ctx, Ctx::Run) && table_depth(&ctx) == 0 => {
                    if let Some(id) = get_attr(e.attributes(), b"id").and_then(|v| v.parse().ok()) {
                        para_endnote_refs.push(id);
                    }
                }

                b"commentRangeStart" if has(&ctx, Ctx::Body) => {
                    if let Some(id) = get_attr(e.attributes(), b"id").and_then(|v| v.parse().ok()) {
                        let in_table = table_depth(&ctx) >= 1;
                        let (char_start, section_index) = if in_table {
                            (cell_text.len(), out.sections.len())
                        } else {
                            (para_text.len(), out.sections.len())
                        };
                        open_comment_starts.insert(
                            id,
                            CommentStart {
                                section_index,
                                char_start,
                                in_table,
                                finalized_end: None,
                            },
                        );
                    }
                }
                b"commentRangeEnd" if has(&ctx, Ctx::Body) => {
                    if let Some(id) = get_attr(e.attributes(), b"id").and_then(|v| v.parse().ok()) {
                        if let Some(start) = open_comment_starts.remove(&id) {
                            let char_end = if let Some(end) = start.finalized_end {
                                end
                            } else if start.in_table {
                                cell_text.len()
                            } else {
                                para_text.len()
                            };
                            out.comment_anchors.insert(
                                id,
                                Anchor {
                                    section_index: start.section_index,
                                    char_start: start.char_start,
                                    char_end,
                                },
                            );
                        }
                    }
                }

                b"headerReference" if has(&ctx, Ctx::Body) => {
                    let kind =
                        get_attr(e.attributes(), b"type").unwrap_or_else(|| "default".into());
                    if let Some(id) = e.attributes().flatten().find_map(|a| {
                        if a.key.as_ref() == b"r:id" {
                            Some(lossy(&a.value))
                        } else {
                            None
                        }
                    }) {
                        out.header_refs.push((kind, id));
                    }
                }
                b"footerReference" if has(&ctx, Ctx::Body) => {
                    let kind =
                        get_attr(e.attributes(), b"type").unwrap_or_else(|| "default".into());
                    if let Some(id) = e.attributes().flatten().find_map(|a| {
                        if a.key.as_ref() == b"r:id" {
                            Some(lossy(&a.value))
                        } else {
                            None
                        }
                    }) {
                        out.footer_refs.push((kind, id));
                    }
                }

                _ => {}
            },

            Event::End(ref e) => match e.local_name().as_ref() {
                b"body" | b"hdr" | b"ftr" | b"footnote" | b"endnote" | b"comment" => {
                    pop_tag(&mut ctx, Ctx::Body);
                }
                b"pPr" => pop_tag(&mut ctx, Ctx::ParaProps),
                b"numPr" => pop_tag(&mut ctx, Ctx::NumPr),
                b"t" | b"delText" => pop_tag(&mut ctx, Ctx::Text),
                b"r" => pop_tag(&mut ctx, Ctx::Run),

                b"ins" => {
                    if let Some(state) = ins_stack.pop() {
                        let end_pos = if state.in_table {
                            cell_text.len()
                        } else {
                            para_text.len()
                        };
                        let buf = if state.in_table {
                            &cell_text
                        } else {
                            &para_text
                        };
                        let text = buf[state.start_pos..end_pos.min(buf.len())].to_string();
                        if !text.is_empty() {
                            out.revisions.push(Revision {
                                kind: "insert".into(),
                                author: state.author,
                                date: state.date,
                                anchor: None, // anchor finalized on paragraph emit
                                text,
                            });
                            // Mark for anchor wiring once the paragraph is emitted.
                            // We store as the last revision; anchor set on para emit if section idx still unknown.
                            // To keep this simple: anchor = current pending section index + char positions.
                            let idx = out.revisions.len() - 1;
                            out.revisions[idx].anchor = Some(Anchor {
                                section_index: out.sections.len(),
                                char_start: state.start_pos,
                                char_end: end_pos,
                            });
                        }
                    }
                    pop_tag(&mut ctx, Ctx::Ins);
                }
                b"del" => {
                    if let Some(state) = del_stack.pop() {
                        if !state.buf.is_empty() {
                            let pos = if state.in_table {
                                cell_text.len()
                            } else {
                                para_text.len()
                            };
                            out.revisions.push(Revision {
                                kind: "delete".into(),
                                author: state.author,
                                date: state.date,
                                anchor: Some(Anchor {
                                    section_index: out.sections.len(),
                                    char_start: pos,
                                    char_end: pos,
                                }),
                                text: state.buf,
                            });
                        }
                    }
                    pop_tag(&mut ctx, Ctx::Del);
                }

                b"p" if has(&ctx, Ctx::Body) && has(&ctx, Ctx::Para) => {
                    let td = table_depth(&ctx);
                    pop_tag(&mut ctx, Ctx::Para);

                    if td == 0 {
                        let raw_len = para_text.len();
                        let trimmed = para_text.trim();
                        if !trimmed.is_empty() || !para_images.is_empty() {
                            let text = trimmed.to_string();
                            let leading = para_text.len() - para_text.trim_start().len();

                            // Adjust char positions of any open comments started in this paragraph
                            // and any revisions targeting this section.
                            for cs in open_comment_starts.values_mut() {
                                if cs.section_index == out.sections.len()
                                    && cs.finalized_end.is_none()
                                {
                                    cs.finalized_end = Some(raw_len.saturating_sub(leading));
                                    cs.char_start = cs.char_start.saturating_sub(leading);
                                }
                            }
                            for rev in out.revisions.iter_mut() {
                                if let Some(anchor) = rev.anchor.as_mut() {
                                    if anchor.section_index == out.sections.len() {
                                        anchor.char_start =
                                            anchor.char_start.saturating_sub(leading);
                                        anchor.char_end = anchor.char_end.saturating_sub(leading);
                                    }
                                }
                            }

                            let images = std::mem::take(&mut para_images);
                            let footnote_refs = std::mem::take(&mut para_footnote_refs);
                            let endnote_refs = std::mem::take(&mut para_endnote_refs);

                            if para_is_list {
                                out.sections.push(Section::ListItem {
                                    level: para_list_level,
                                    text,
                                    images,
                                    footnote_refs,
                                    endnote_refs,
                                });
                            } else {
                                let level = para_style
                                    .as_deref()
                                    .and_then(heading_level)
                                    .or_else(|| para_outline_level.map(|l| l + 1));
                                match level {
                                    Some(level) => out.sections.push(Section::Heading {
                                        level,
                                        text,
                                        images,
                                        footnote_refs,
                                        endnote_refs,
                                    }),
                                    None => out.sections.push(Section::Paragraph {
                                        text,
                                        images,
                                        footnote_refs,
                                        endnote_refs,
                                    }),
                                }
                            }
                        }
                        para_style = None;
                        para_outline_level = None;
                        para_text.clear();
                        para_images.clear();
                        para_footnote_refs.clear();
                        para_endnote_refs.clear();
                        para_is_list = false;
                        para_list_level = 0;
                    }
                }

                b"tc" => {
                    let depth = table_depth(&ctx);
                    if depth == 1 && has(&ctx, Ctx::Row) {
                        pop_tag(&mut ctx, Ctx::Cell);
                        current_row.push(TableCell {
                            text: cell_text.trim().to_string(),
                            images: std::mem::take(&mut cell_images),
                        });
                        cell_text.clear();
                    } else if depth >= 2 && has(&ctx, Ctx::Row) {
                        pop_tag(&mut ctx, Ctx::Cell);
                        nested_row.push(nested_cell.trim().to_string());
                        nested_cell.clear();
                    }
                }
                b"tr" => {
                    let depth = table_depth(&ctx);
                    if depth == 1 {
                        pop_tag(&mut ctx, Ctx::Row);
                        if !current_row.is_empty() {
                            current_table.push(std::mem::take(&mut current_row));
                        }
                    } else if depth >= 2 {
                        pop_tag(&mut ctx, Ctx::Row);
                        if !nested_row.is_empty() {
                            nested_rows.push(std::mem::take(&mut nested_row));
                        }
                    }
                }
                b"tbl" => {
                    let depth = table_depth(&ctx);
                    pop_tag(&mut ctx, Ctx::Tbl);
                    if depth == 1 && !current_table.is_empty() {
                        out.sections.push(Section::Table {
                            rows: std::mem::take(&mut current_table),
                        });
                    } else if depth == 2 {
                        let rendered = nested_rows
                            .iter()
                            .map(|row| row.join(" | "))
                            .collect::<Vec<_>>()
                            .join("\n");
                        nested_rows.clear();
                        if !rendered.is_empty() {
                            if !cell_text.is_empty() && !cell_text.ends_with('\n') {
                                cell_text.push('\n');
                            }
                            cell_text.push_str(&rendered);
                        }
                    }
                }

                b"txbxContent" if has(&ctx, Ctx::Body) => {
                    if txbx_depth == 1 {
                        if let Some(saved) = saved_outer_para.take() {
                            para_text = saved.text;
                            para_style = saved.style;
                            para_is_list = saved.is_list;
                            para_list_level = saved.list_level;
                            para_images = saved.images;
                            para_footnote_refs = saved.footnote_refs;
                            para_endnote_refs = saved.endnote_refs;
                        }
                    }
                    txbx_depth = txbx_depth.saturating_sub(1);
                }

                b"hyperlink" if has(&ctx, Ctx::Body) => {
                    if let Some(url) = hyperlink_url.take() {
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

            Event::Text(ref e) if has(&ctx, Ctx::Text) => {
                let text = e.unescape().unwrap_or_default().to_string();
                push_text(
                    &text,
                    &ctx,
                    &mut para_text,
                    &mut cell_text,
                    &mut nested_cell,
                    &mut del_stack,
                );
            }

            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

// Route text to the right buffer based on current context. Deletions are
// captured into the active <w:del>'s buffer instead of the visible text.
fn push_text(
    s: &str,
    ctx: &[Ctx],
    para_text: &mut String,
    cell_text: &mut String,
    nested_cell: &mut String,
    del_stack: &mut [DelState],
) {
    if !del_stack.is_empty() {
        if let Some(last) = del_stack.last_mut() {
            last.buf.push_str(s);
        }
        return;
    }
    let depth = table_depth(ctx);
    if depth == 0 {
        para_text.push_str(s);
    } else if depth == 1 && has(ctx, Ctx::Cell) {
        cell_text.push_str(s);
    } else if depth >= 2 {
        nested_cell.push_str(s);
    }
}

fn add_image_ref(
    ctx: &[Ctx],
    para_images: &mut Vec<String>,
    cell_images: &mut Vec<String>,
    name: String,
) {
    let depth = table_depth(ctx);
    let target = if depth == 0 {
        para_images
    } else if depth >= 1 && has(ctx, Ctx::Cell) {
        cell_images
    } else {
        return;
    };
    if !target.contains(&name) {
        target.push(name);
    }
}

fn image_filename_for(e: &BytesStart, rels: &Rels) -> Option<String> {
    // a:blip uses r:embed; v:imagedata uses r:id.
    for attr in e.attributes().flatten() {
        let key = attr.key.as_ref();
        if key == b"r:embed" || key == b"r:id" {
            let rid = lossy(&attr.value);
            if let Some(path) = rels.images.get(&rid) {
                return std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
            }
        }
    }
    None
}

// ── Header/footer & notes & comments file parsers ──────────────────────────────

fn parse_header_footer_parts(
    archive: &mut ZipArchive<std::fs::File>,
    rels: &Rels,
    refs: &[(String, String)],
    _kind_tag: &str,
) -> Result<Vec<HeaderFooter>, ExtractError> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let lookup = if _kind_tag == "hdr" {
        &rels.headers
    } else {
        &rels.footers
    };

    for (kind, rid) in refs {
        if !seen.insert(rid.clone()) {
            continue;
        }
        let Some(path) = lookup.get(rid) else {
            continue;
        };
        let mut xml = String::new();
        match archive.by_name(path) {
            Ok(mut f) => f.read_to_string(&mut xml)?,
            Err(_) => continue,
        };
        let body = parse_body(&xml, rels);
        if !body.sections.is_empty() {
            out.push(HeaderFooter {
                kind: kind.clone(),
                sections: body.sections,
            });
        }
    }
    Ok(out)
}

fn parse_notes_file(
    archive: &mut ZipArchive<std::fs::File>,
    path: &str,
    note_tag: &[u8],
    rels: &Rels,
) -> Result<Vec<Note>, ExtractError> {
    let mut xml = String::new();
    match archive.by_name(path) {
        Ok(mut f) => f.read_to_string(&mut xml)?,
        Err(_) => return Ok(Vec::new()),
    };

    let tag_str = std::str::from_utf8(note_tag).unwrap_or("");
    let chunks = split_by_tag(&xml, tag_str);
    let mut out = Vec::new();
    for chunk in chunks {
        // Skip separator notes (w:type="separator" or "continuationSeparator").
        if let Some(t) = find_attr_in_open_tag(&chunk.opening, b"type") {
            if t == "separator" || t == "continuationSeparator" {
                continue;
            }
        }
        let Some(id) =
            find_attr_in_open_tag(&chunk.opening, b"id").and_then(|v| v.parse::<i64>().ok())
        else {
            continue;
        };
        if id < 1 {
            continue;
        }
        let body = parse_body(&chunk.full, rels);
        if !body.sections.is_empty() {
            out.push(Note {
                id: id as u32,
                sections: body.sections,
            });
        }
    }
    Ok(out)
}

fn parse_comments_file(
    archive: &mut ZipArchive<std::fs::File>,
    rels: &Rels,
    anchors: &HashMap<u32, Anchor>,
) -> Result<Vec<Comment>, ExtractError> {
    let mut xml = String::new();
    match archive.by_name("word/comments.xml") {
        Ok(mut f) => f.read_to_string(&mut xml)?,
        Err(_) => return Ok(Vec::new()),
    };

    let chunks = split_by_tag(&xml, "comment");
    let mut out = Vec::new();
    for chunk in chunks {
        let id = match find_attr_in_open_tag(&chunk.opening, b"id").and_then(|v| v.parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        let author = find_attr_in_open_tag(&chunk.opening, b"author");
        let date = find_attr_in_open_tag(&chunk.opening, b"date");
        let body = parse_body(&chunk.full, rels);
        out.push(Comment {
            id,
            author,
            date,
            anchor: anchors.get(&id).cloned(),
            sections: body.sections,
        });
    }
    out.sort_by_key(|c| c.id);
    Ok(out)
}

struct Chunk {
    opening: String, // the opening tag like `<w:footnote w:id="1">`
    full: String,    // full element including opening and closing tags
}

/// Split an XML document into chunks bounded by `<w:{tag} ...>` ... `</w:{tag}>`.
/// Used to break footnotes.xml / endnotes.xml / comments.xml into entries.
fn split_by_tag(xml: &str, tag: &str) -> Vec<Chunk> {
    let open_prefix = format!("<w:{tag}");
    let close_tag = format!("</w:{tag}>");
    let bytes = xml.as_bytes();

    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel) = xml[cursor..].find(&open_prefix) {
        let start = cursor + rel;
        // Reject false matches like `<w:footnoteReference` when looking for `<w:footnote`.
        let after = bytes.get(start + open_prefix.len()).copied();
        if !matches!(
            after,
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'>') | Some(b'/')
        ) {
            cursor = start + open_prefix.len();
            continue;
        }

        // Find end of the opening tag.
        let close_of_open = match xml[start..].find('>') {
            Some(i) => start + i + 1,
            None => break,
        };
        let opening = xml[start..close_of_open].to_string();

        // Self-closing form `<w:tag .../>` — emit chunk with empty body.
        if opening.trim_end().ends_with("/>") {
            out.push(Chunk {
                opening: opening.clone(),
                full: opening,
            });
            cursor = close_of_open;
            continue;
        }

        // Find matching closing tag.
        let end_rel = match xml[close_of_open..].find(&close_tag) {
            Some(i) => i,
            None => break,
        };
        let end = close_of_open + end_rel + close_tag.len();
        out.push(Chunk {
            opening,
            full: xml[start..end].to_string(),
        });
        cursor = end;
    }
    out
}

fn find_attr_in_open_tag(opening: &str, key: &[u8]) -> Option<String> {
    // Parse the opening tag with quick-xml to extract a single attribute robustly.
    let mut reader = Reader::from_str(opening);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                return get_attr(e.attributes(), key);
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
    }
}

// ── Image extraction ───────────────────────────────────────────────────────────

fn extract_images(
    archive: &mut ZipArchive<std::fs::File>,
    rels: &HashMap<String, String>,
) -> Result<Vec<Image>, ExtractError> {
    let mut images = Vec::new();

    let entries: Vec<(String, String)> = rels
        .values()
        .filter_map(|path| {
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
        if path.contains("..") || path.starts_with('/') || path.starts_with('\\') {
            eprintln!("warning: skipping image with unsafe path: {path}");
            continue;
        }

        const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

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

    #[test]
    fn lossy_valid_utf8_is_unchanged() {
        assert_eq!(lossy(b"hello world"), "hello world");
    }

    #[test]
    fn lossy_invalid_utf8_returns_replacement() {
        let result = lossy(&[0xFF, 0xFE, 0x41]);
        assert!(result.contains('\u{FFFD}'));
        assert!(result.ends_with('A'));
    }

    #[test]
    fn lossy_empty_is_empty() {
        assert_eq!(lossy(b""), "");
    }
}
