use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::path::Path;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Rasterizes PDF pages into one scrollable document and maps annotations back
/// to the original page and, when possible, its extracted text.
pub struct PdfRenderer {
    worker: Option<String>,
}

impl PdfRenderer {
    pub fn new(worker: Option<String>) -> Self {
        Self { worker }
    }
}

#[async_trait::async_trait]
impl Renderer for PdfRenderer {
    fn id(&self) -> &'static str {
        "pdf"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Pdf {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_deref()
            .ok_or_else(|| RenderError::Capture("PDF path required".into()))?;
        let worker = self
            .worker
            .as_deref()
            .ok_or_else(|| RenderError::Capture("PDF worker not configured".into()))?;
        let script = Path::new(worker).with_file_name("pdf-map.js");
        if !script.is_file() {
            return Err(RenderError::Capture("PDF worker not found".into()));
        }
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > 30 * 1024 * 1024 {
            return Err(RenderError::Capture("PDF exceeds 30 MiB".into()));
        }
        let tmp = TempDir::new()?;
        let mut command = Command::new("node");
        command
            .arg(script)
            .arg(path)
            .arg(tmp.path())
            .kill_on_drop(true);
        let output = timeout(Duration::from_secs(90), command.output())
            .await
            .map_err(|_| RenderError::Capture("PDF capture timed out".into()))?
            .map_err(|e| RenderError::Capture(format!("spawn PDF worker: {e}")))?;
        if !output.status.success() {
            return Err(RenderError::Capture(format!(
                "PDF worker failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let response: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| RenderError::Capture(format!("invalid PDF map: {e}")))?;
        let width = response["width"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= 960)
            .ok_or_else(|| RenderError::Capture("invalid PDF width".into()))?
            as u32;
        let height = response["height"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= 12_000)
            .ok_or_else(|| RenderError::Capture("invalid PDF height".into()))?
            as u32;
        let pages = response["pages"]
            .as_array()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| RenderError::Capture("PDF has no pages".into()))?;
        // Read only the output in our private directory, not an arbitrary path
        // reported by a worker response.
        let asset_bytes = std::fs::read(tmp.path().join("pages.png"))?;
        let label = target.label.clone().unwrap_or_else(|| {
            Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "PDF".into())
        });
        Ok(SnapshotDraft {
            width,
            height,
            mime_type: "image/png".into(),
            asset_bytes,
            mapping: serde_json::json!({"version": 1, "renderer": "pdf", "kind": "pdf", "pages": pages}),
            source_label: label,
        })
    }

    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError> {
        let map: serde_json::Value = serde_json::from_str(&snapshot.mapping_json)
            .map_err(|e| RenderError::Resolve(e.to_string()))?;
        let pages = map["pages"]
            .as_array()
            .ok_or_else(|| RenderError::Resolve("PDF pages missing".into()))?;
        let (x, y) = match selection {
            Selection::Point { x, y } => (*x, *y),
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => (*x + width / 2.0, *y + height / 2.0),
        };
        let page = pages
            .iter()
            .min_by(|a, b| {
                let dist = |p: &serde_json::Value| {
                    let top = p["y"].as_f64().unwrap_or(0.0);
                    let bottom = top + p["height"].as_f64().unwrap_or(0.0);
                    (top - y).max(0.0).max(y - bottom)
                };
                dist(a).total_cmp(&dist(b))
            })
            .ok_or_else(|| RenderError::Resolve("PDF pages missing".into()))?;
        let number = page["page"].as_u64().unwrap_or(1);
        let nearest = page["text"].as_array().and_then(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let ix = item["x"].as_f64()?;
                    let iy = item["y"].as_f64()?;
                    let right = ix + item["width"].as_f64()?;
                    let bottom = iy + item["height"].as_f64()?;
                    let dx = (ix - x).max(0.0).max(x - right);
                    let dy = (iy - y).max(0.0).max(y - bottom);
                    Some((dx * dx + dy * dy, item))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
        });
        if let Some((distance, item)) = nearest {
            if distance < 28.0 * 28.0 {
                let text = item["text"].as_str().unwrap_or("");
                return Ok(ResolvedReference {
                    description: format!(
                        "{}: PDF page {} near \"{}\"",
                        snapshot.source_label, number, text
                    ),
                    anchor: serde_json::json!({"kind": "pdf_text", "page": number, "text": text, "selection": selection}),
                });
            }
        }
        Ok(ResolvedReference {
            description: format!(
                "{}: PDF page {} at ({:.0}, {:.0})",
                snapshot.source_label, number, x, y
            ),
            anchor: serde_json::json!({"kind": "pdf_page", "page": number, "selection": selection}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_text_and_page_gap() {
        let snapshot = StoredSnapshot {
            id: "s".into(), review_id: "r".into(), position: 0,
            renderer_id: "pdf".into(), mapping_version: 1, width: 960, height: 1128,
            mime_type: "image/png".into(), asset_path: "".into(),
            source_label: "slides.pdf".into(),
            mapping_json: serde_json::json!({"pages": [
                {"page": 1, "y": 24, "height": 540, "text": [{"text": "Hello", "x": 20, "y": 50, "width": 80, "height": 22}]},
                {"page": 2, "y": 588, "height": 540, "text": []}
            ]}).to_string(),
        };
        let renderer = PdfRenderer::new(None);
        let hit = renderer
            .resolve(&snapshot, &Selection::Point { x: 40.0, y: 60.0 })
            .unwrap();
        assert_eq!(hit.anchor["page"], 1);
        assert_eq!(hit.anchor["kind"], "pdf_text");
        assert!(hit.description.contains("Hello"));
        let blank = renderer
            .resolve(&snapshot, &Selection::Point { x: 40.0, y: 800.0 })
            .unwrap();
        assert_eq!(blank.anchor["page"], 2);
        assert_eq!(blank.anchor["kind"], "pdf_page");
    }
}
