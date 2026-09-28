use image::{ImageFormat, ImageReader};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::io::Cursor;

pub struct ImageRenderer;

impl ImageRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ImageRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Renderer for ImageRenderer {
    fn id(&self) -> &'static str {
        "image"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Image {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_ref()
            .ok_or_else(|| RenderError::Capture("no path".into()))?;
        let bytes = std::fs::read(path)?;

        let img = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()?
            .decode()?;
        let width = img.width();
        let height = img.height();
        if width == 0 || height == 0 || (width as u64 * height as u64) > 100_000_000 {
            return Err(RenderError::Capture("invalid dimensions".into()));
        }

        let mut out_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut out_bytes);
        img.write_to(&mut cursor, ImageFormat::Png)?;

        let mapping = serde_json::json!({
            "version": 1,
            "renderer": "image",
            "original_path": path,
            "original_width": width,
            "original_height": height,
        });

        let label = target.label.clone().unwrap_or_else(|| {
            std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or("image".into())
        });

        Ok(SnapshotDraft {
            width,
            height,
            mime_type: "image/png".to_string(),
            asset_bytes: out_bytes,
            mapping,
            source_label: label,
        })
    }

    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError> {
        let mapping: serde_json::Value = serde_json::from_str(&snapshot.mapping_json)
            .map_err(|e| RenderError::Resolve(e.to_string()))?;
        let orig_w: u32 = mapping["original_width"]
            .as_u64()
            .unwrap_or(snapshot.width as u64) as u32;
        let orig_h: u32 = mapping["original_height"]
            .as_u64()
            .unwrap_or(snapshot.height as u64) as u32;
        let label = &snapshot.source_label;

        let desc = match selection {
            Selection::Point { x, y } => {
                if *x < 0.0 || *y < 0.0 || *x > snapshot.width as f64 || *y > snapshot.height as f64
                {
                    return Err(RenderError::InvalidSelection);
                }
                let ix = (*x as i32).clamp(0, orig_w as i32 - 1);
                let iy = (*y as i32).clamp(0, orig_h as i32 - 1);
                format!(
                    "{}: near original image coordinates ({}, {})",
                    label, ix, iy
                )
            }
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => {
                if *x < 0.0
                    || *y < 0.0
                    || *width <= 0.0
                    || *height <= 0.0
                    || *x + *width > snapshot.width as f64 + 0.1
                    || *y + *height > snapshot.height as f64 + 0.1
                {
                    return Err(RenderError::InvalidSelection);
                }
                let x0 = (*x as i32).clamp(0, orig_w as i32);
                let y0 = (*y as i32).clamp(0, orig_h as i32);
                let x1 = ((*x + *width) as i32).clamp(0, orig_w as i32);
                let y1 = ((*y + *height) as i32).clamp(0, orig_h as i32);
                format!(
                    "{}: rectangle in original image coordinates ({}, {})–({}, {})",
                    label, x0, y0, x1, y1
                )
            }
        };

        Ok(ResolvedReference {
            description: desc,
            anchor: serde_json::json!({
                "kind": "image_pixel",
                "snapshot_id": snapshot.id,
                "selection": selection,
                "original_size": [orig_w, orig_h]
            }),
        })
    }
}
