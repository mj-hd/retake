pub mod registry;
pub mod runtime;

pub use registry::RendererRegistry;
pub use runtime::RuntimePaths;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Selection {
    Point {
        x: f64,
        y: f64,
    },
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    #[serde(rename = "type")]
    pub kind: TargetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Image,
    Text,
    Markdown,
    Html,
    Web,
    Adb,
    Code,
    Pdf,
    Pencil,
    Video,
    MacosWindow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotDraft {
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub asset_bytes: Vec<u8>,
    pub mapping: serde_json::Value,
    pub source_label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedReference {
    pub description: String,
    pub anchor: serde_json::Value,
}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("unsupported target")]
    Unsupported,
    #[error("capture failed: {0}")]
    Capture(String),
    #[error("resolve failed: {0}")]
    Resolve(String),
    #[error("invalid selection")]
    InvalidSelection,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("other: {0}")]
    Other(String),
}

#[async_trait::async_trait]
pub trait Renderer: Send + Sync {
    fn id(&self) -> &'static str;
    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError>;
    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError>;
    fn variation_guide(&self, _snapshot: &StoredSnapshot, _count: u8) -> Option<VariationGuide> {
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredSnapshot {
    pub id: String,
    pub review_id: String,
    pub position: i32,
    pub renderer_id: String,
    pub mapping_version: i32,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub asset_path: String,
    pub source_label: String,
    pub mapping_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariationGuide {
    pub instructions: String,
    pub identification: String,
    pub recapture: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewResult {
    pub status: String, // "submitted" | "cancelled" | "pending"
    pub review_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<Annotation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<Generation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Annotation {
    pub snapshot_id: String,
    pub target_label: String,
    pub selection: Selection,
    pub comment: String,
    pub reference: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Generation {
    pub variants_enabled: bool,
    pub count: u8,
    pub guides: Vec<String>,
}
