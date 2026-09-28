use image::{ImageFormat, ImageReader};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use serde_json::Value;
use std::{io::Cursor, path::Path};

/// Consumes a PNG and node map exported via Pencil MCP. Never reads .pen files.
pub struct PencilRenderer;

impl PencilRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PencilRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Renderer for PencilRenderer {
    fn id(&self) -> &'static str {
        "pencil"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Pencil {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_deref()
            .ok_or_else(|| RenderError::Capture("Pencil export path required".into()))?;
        let map: Value = if let Some(map) = &target.metadata {
            if Path::new(path).extension().and_then(|s| s.to_str()) != Some("png") {
                return Err(RenderError::Capture(
                    "Pencil metadata requires an exported PNG path".into(),
                ));
            }
            map.clone()
        } else {
            if Path::new(path).extension().and_then(|s| s.to_str()) != Some("json") {
                return Err(RenderError::Capture(
                    "Export via Pencil MCP first; do not pass a .pen file".into(),
                ));
            }
            let bytes = std::fs::read(path)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(RenderError::Capture("Pencil manifest exceeds 4 MiB".into()));
            }
            serde_json::from_slice(&bytes)
                .map_err(|e| RenderError::Capture(format!("invalid Pencil manifest: {e}")))?
        };
        if serde_json::to_vec(&map).map_or(true, |data| data.len() > 4 * 1024 * 1024) {
            return Err(RenderError::Capture("Pencil metadata exceeds 4 MiB".into()));
        }
        let nodes = map["nodes"]
            .as_array()
            .filter(|n| n.len() <= 10_000)
            .ok_or_else(|| RenderError::Capture("invalid Pencil nodes".into()))?;
        if map["version"] != 1 {
            return Err(RenderError::Capture(
                "unknown Pencil manifest version".into(),
            ));
        }
        let width = map["width"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 10_000)
            .ok_or_else(|| RenderError::Capture("invalid Pencil width".into()))?
            as u32;
        let height = map["height"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 10_000)
            .ok_or_else(|| RenderError::Capture("invalid Pencil height".into()))?
            as u32;
        if width as u64 * height as u64 > 40_000_000 {
            return Err(RenderError::Capture(
                "Pencil frame exceeds 40 megapixels".into(),
            ));
        }
        let scale = map["scale"]
            .as_u64()
            .filter(|n| (1..=4).contains(n))
            .ok_or_else(|| RenderError::Capture("Pencil scale must be 1–4".into()))?
            as u32;
        let png_path = (if target.metadata.is_some() {
            Some(path)
        } else {
            map["png_path"].as_str()
        })
        .map(Path::new)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| RenderError::Capture("absolute Pencil PNG path required".into()))?;
        let png_bytes = std::fs::read(png_path)?;
        if png_bytes.len() > 80 * 1024 * 1024 {
            return Err(RenderError::Capture("Pencil PNG exceeds 80 MiB".into()));
        }
        let image = ImageReader::new(Cursor::new(&png_bytes))
            .with_guessed_format()?
            .decode()?;
        if image.width() != width * scale || image.height() != height * scale {
            return Err(RenderError::Capture(
                "Pencil PNG dimensions do not match manifest".into(),
            ));
        }
        let frame = map["frame_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| RenderError::Capture("Pencil frame_id required".into()))?;
        let frame_name = map["frame_name"].as_str().unwrap_or("Frame");
        let document = map["document"].as_str().unwrap_or("Pencil");
        let mut asset_bytes = Vec::new();
        image.write_to(&mut Cursor::new(&mut asset_bytes), ImageFormat::Png)?;
        Ok(SnapshotDraft {
            width,
            height,
            mime_type: "image/png".into(),
            asset_bytes,
            mapping: serde_json::json!({"version":1,"renderer":"pencil","document":document,"frame_id":frame,"frame_name":frame_name,"nodes":nodes}),
            source_label: target
                .label
                .clone()
                .unwrap_or_else(|| format!("{document} / {frame_name}")),
        })
    }

    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError> {
        let map: Value = serde_json::from_str(&snapshot.mapping_json)
            .map_err(|e| RenderError::Resolve(e.to_string()))?;
        let (x, y) = match selection {
            Selection::Point { x, y } => (*x, *y),
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => (*x + width / 2.0, *y + height / 2.0),
        };
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > snapshot.width as f64
            || y > snapshot.height as f64
        {
            return Err(RenderError::InvalidSelection);
        }
        let hit = map["nodes"].as_array().and_then(|nodes| {
            let bounds = |n: &Value| {
                let r = &n["rect"];
                let (nx, ny, w, h) = (
                    r["x"].as_f64()?,
                    r["y"].as_f64()?,
                    r["width"].as_f64()?,
                    r["height"].as_f64()?,
                );
                if ![nx, ny, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
                    return None;
                }
                Some((nx, ny, w, h))
            };
            let rank = |a: &&Value, b: &&Value| {
                let depth = |n: &Value| n["path"].as_array().map_or(0, Vec::len);
                let area = |n: &Value| {
                    n["rect"]["width"].as_f64().unwrap_or(f64::MAX)
                        * n["rect"]["height"].as_f64().unwrap_or(f64::MAX)
                };
                depth(a)
                    .cmp(&depth(b))
                    .then_with(|| area(b).total_cmp(&area(a)))
            };
            // A drag across several children refers to their smallest shared
            // containing frame, not whichever child happens to be at its center.
            let enclosed_text = match selection {
                Selection::Rect {
                    x,
                    y,
                    width,
                    height,
                } if *width > 0.0 && *height > 0.0 => nodes
                    .iter()
                    .filter(|n| {
                        n["type"] == "text"
                            && bounds(n).is_some_and(|(nx, ny, w, h)| {
                                // A rectangle drawn just outside a label should
                                // resolve to the label, not its containing frame.
                                nx >= *x
                                    && ny >= *y
                                    && nx + w <= *x + *width + 1.0
                                    && ny + h <= *y + *height + 1.0
                                    && w * h >= *width * *height * 0.55
                            })
                    })
                    .max_by(rank),
                _ => None,
            };
            let enclosing = match selection {
                Selection::Rect {
                    x,
                    y,
                    width,
                    height,
                } if *width > 0.0 && *height > 0.0 => nodes
                    .iter()
                    .filter(|n| {
                        bounds(n).is_some_and(|(nx, ny, w, h)| {
                            *x >= nx
                                && *y >= ny
                                && *x + *width <= nx + w + 1.0
                                && *y + *height <= ny + h + 1.0
                        })
                    })
                    .max_by(rank),
                _ => None,
            };
            enclosed_text.or(enclosing).or_else(|| {
                nodes
                    .iter()
                    .filter(|n| {
                        bounds(n).is_some_and(|(nx, ny, w, h)| {
                            x >= nx && x <= nx + w && y >= ny && y <= ny + h
                        })
                    })
                    .max_by(rank)
            })
        });
        let doc = map["document"].as_str().unwrap_or("Pencil");
        let frame = map["frame_name"].as_str().unwrap_or("Frame");
        let frame_id = map["frame_id"].as_str().unwrap_or("");
        if let Some(node) = hit {
            let id = node["id"].as_str().unwrap_or(frame_id);
            let name = node["name"].as_str().unwrap_or("Layer");
            let kind = node["type"].as_str().unwrap_or("node");
            let text = node["text"].as_str().unwrap_or("");
            let path = node["path"]
                .as_array()
                .map(|p| {
                    p.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" / ")
                })
                .unwrap_or_else(|| name.to_owned());
            let suffix = if text.is_empty() {
                String::new()
            } else {
                format!(" \"{}\"", text.chars().take(60).collect::<String>())
            };
            return Ok(ResolvedReference {
                description: format!("{doc}: {frame} / {path} ({kind}, node {id}){suffix}"),
                anchor: serde_json::json!({"kind":"pencil_node","document":doc,"frame_id":frame_id,"node_id":id,"path":node["path"],"selection":selection}),
            });
        }
        Ok(ResolvedReference {
            description: format!("{doc}: {frame} (node {frame_id}) at ({x:.0}, {y:.0})"),
            anchor: serde_json::json!({"kind":"pencil_frame","document":doc,"frame_id":frame_id,"selection":selection}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_and_resolves_deepest_node() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("frame.png");
        image::RgbImage::new(200, 120).save(&png).unwrap();
        let manifest = tmp.path().join("frame.json");
        std::fs::write(&manifest,serde_json::json!({
            "version":1,"document":"test.pen","frame_id":"root","frame_name":"Screen",
            "width":100,"height":60,"scale":2,"png_path":png,
            "nodes":[
                {"id":"container","name":"Card","type":"frame","path":["Card"],"rect":{"x":5,"y":5,"width":90,"height":50}},
                {"id":"title","name":"Title","type":"text","text":"Hello","path":["Card","Title"],"rect":{"x":12,"y":12,"width":30,"height":20}}
            ]
        }).to_string()).unwrap();
        let renderer = PencilRenderer::new();
        let draft = renderer
            .capture(&Target {
                kind: TargetKind::Pencil,
                path: Some(manifest.to_string_lossy().into()),
                url: None,
                label: None,
                viewport: None,
                metadata: None,
            })
            .await
            .unwrap();
        let mut inline: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        inline.as_object_mut().unwrap().remove("png_path");
        let inline_draft = renderer
            .capture(&Target {
                kind: TargetKind::Pencil,
                path: Some(png.to_string_lossy().into()),
                url: None,
                label: None,
                viewport: None,
                metadata: Some(inline),
            })
            .await
            .unwrap();
        assert_eq!(inline_draft.width, draft.width);
        assert_eq!(inline_draft.mapping, draft.mapping);
        let snap = StoredSnapshot {
            id: "s".into(),
            review_id: "r".into(),
            position: 0,
            renderer_id: "pencil".into(),
            mapping_version: 1,
            width: draft.width,
            height: draft.height,
            mime_type: draft.mime_type,
            asset_path: "".into(),
            source_label: draft.source_label,
            mapping_json: draft.mapping.to_string(),
        };
        assert_eq!(
            renderer
                .resolve(&snap, &Selection::Point { x: 20.0, y: 20.0 })
                .unwrap()
                .anchor["node_id"],
            "title"
        );
        assert_eq!(
            renderer
                .resolve(
                    &snap,
                    &Selection::Rect {
                        x: 6.0,
                        y: 6.0,
                        width: 85.0,
                        height: 45.0,
                    }
                )
                .unwrap()
                .anchor["node_id"],
            "container"
        );
        assert_eq!(
            renderer
                .resolve(
                    &snap,
                    &Selection::Rect {
                        x: 10.0,
                        y: 10.0,
                        width: 34.0,
                        height: 24.0,
                    }
                )
                .unwrap()
                .anchor["node_id"],
            "title"
        );
        assert_eq!(
            renderer
                .resolve(&snap, &Selection::Point { x: 99.0, y: 59.0 })
                .unwrap()
                .anchor["kind"],
            "pencil_frame"
        );
    }
}
