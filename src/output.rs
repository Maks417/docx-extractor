use serde::Serialize;

/// Document-level properties from `docProps/core.xml`. All fields are optional;
/// absent properties are omitted from the JSON output.
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

/// Top-level output structure serialized to JSON.
#[derive(Serialize, Debug)]
pub struct Document {
    /// Filename of the source DOCX (not the full path).
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Ordered content sections extracted from the document body.
    pub sections: Vec<Section>,
    /// Embedded images, base64-encoded.
    pub images: Vec<Image>,
}

/// A single content block from the document body.
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Section {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    #[serde(rename = "list_item")]
    ListItem {
        level: u8,
        text: String,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
}

/// An embedded image extracted from the DOCX archive.
#[derive(Serialize, Debug)]
pub struct Image {
    /// Filename of the image as stored in the archive (e.g. `image1.png`).
    pub id: String,
    pub mime_type: String,
    pub base64: String,
}
