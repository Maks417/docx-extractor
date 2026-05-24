use serde::Serialize;

#[derive(Serialize, Debug, Default)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Document {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    pub sections: Vec<Section>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderFooter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footers: Vec<HeaderFooter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<Note>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub endnotes: Vec<Note>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    pub images: Vec<Image>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Section {
    Heading {
        level: u8,
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        images: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        footnote_refs: Vec<u32>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        endnote_refs: Vec<u32>,
    },
    Paragraph {
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        images: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        footnote_refs: Vec<u32>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        endnote_refs: Vec<u32>,
    },
    #[serde(rename = "list_item")]
    ListItem {
        level: u8,
        text: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        images: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        footnote_refs: Vec<u32>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        endnote_refs: Vec<u32>,
    },
    Table {
        rows: Vec<Vec<TableCell>>,
    },
}

#[derive(Serialize, Debug, Default, Clone)]
pub struct TableCell {
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub images: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct HeaderFooter {
    #[serde(rename = "type")]
    pub kind: String,
    pub sections: Vec<Section>,
}

#[derive(Serialize, Debug)]
pub struct Note {
    pub id: u32,
    pub sections: Vec<Section>,
}

#[derive(Serialize, Debug)]
pub struct Comment {
    pub id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    pub sections: Vec<Section>,
}

#[derive(Serialize, Debug)]
pub struct Revision {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    pub text: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct Anchor {
    pub section_index: usize,
    pub char_start: usize,
    pub char_end: usize,
}

#[derive(Serialize, Debug)]
pub struct Image {
    pub id: String,
    pub mime_type: String,
    pub base64: String,
}
