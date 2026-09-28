use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{ImageFormat, Rgb, RgbImage};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::io::Cursor;

const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 16.0;
const LINE_H: f32 = 18.0;
const MAX_CHARS_PER_LINE: usize = 120;
const PADDING: u32 = 8;
// Keep selections/mappings in logical pixels while rendering a sharp 2x PNG.
const RASTER_SCALE: u32 = 2;

pub struct TextRenderer;

impl TextRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Renderer for TextRenderer {
    fn id(&self) -> &'static str {
        "text"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Text {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_ref()
            .ok_or_else(|| RenderError::Capture("no path".into()))?;
        let content = std::fs::read_to_string(path)?;
        let _bytes = content.as_bytes();

        // load a font (try common system paths for DejaVu which supports many chars)
        let font_data: Vec<u8> = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/freefont/FreeSans.ttf"))
            .or_else(|_| std::fs::read("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"))
            .unwrap_or_default();

        let mut char_positions: Vec<(usize, usize, f32, f32, f32, f32)> = vec![];
        let mut img_w = 0u32;

        if !font_data.is_empty() {
            if let Ok(font) = FontRef::try_from_slice(&font_data) {
                let scale = PxScale::from(14.0);
                let scaled = font.as_scaled(scale);

                let max_width = MAX_CHARS_PER_LINE as f32 * CHAR_W; // keep similar visual width

                let mut current_y = PADDING as f32;
                let mut byte_offset = 0usize;

                for line_content in content.lines() {
                    let mut x = PADDING as f32;
                    let mut line_start_y = current_y;

                    for ch in line_content.chars() {
                        let ch_bytes = ch.len_utf8();
                        let start = byte_offset;
                        byte_offset += ch_bytes;

                        if ch == '\t' {
                            let tab_w = CHAR_W * 4.0;
                            char_positions.push((
                                start,
                                byte_offset,
                                x,
                                line_start_y,
                                tab_w,
                                CHAR_H,
                            ));
                            x += tab_w;
                            continue;
                        }

                        let glyph_id = font.glyph_id(ch);
                        let advance = scaled.h_advance(glyph_id);
                        let glyph = glyph_id.with_scale(scale);

                        let (w, h) = if let Some(outlined) = scaled.outline_glyph(glyph) {
                            let b = outlined.px_bounds();
                            (b.width().max(advance), b.height().max(CHAR_H))
                        } else {
                            (advance, CHAR_H)
                        };

                        // wrap if needed
                        if x + advance > PADDING as f32 + max_width && x > PADDING as f32 {
                            current_y += LINE_H;
                            x = PADDING as f32;
                            line_start_y = current_y;
                        }

                        char_positions.push((start, byte_offset, x, line_start_y, w, h));
                        x += advance;
                    }
                    // newline
                    byte_offset += 1;
                    current_y += LINE_H;
                    img_w = img_w.max((x + PADDING as f32) as u32);
                }

                let img_h = (current_y + PADDING as f32) as u32;

                let logical_w = img_w.max(20);
                let logical_h = img_h.max(20);
                let mut img = RgbImage::new(logical_w * RASTER_SCALE, logical_h * RASTER_SCALE);
                for p in img.pixels_mut() {
                    *p = Rgb([250, 250, 250]);
                }

                // redraw with actual glyphs using the positions we just computed
                let raster_scale = PxScale::from(14.0 * RASTER_SCALE as f32);
                let raster_font = font.as_scaled(raster_scale);
                let mut pos_idx = 0;
                let mut _y = PADDING as f32;
                for line in content.lines() {
                    let mut _x = PADDING as f32;
                    for ch in line.chars() {
                        if ch == '\t' {
                            _x += CHAR_W * 4.0;
                            pos_idx += 1;
                            continue;
                        }
                        if pos_idx < char_positions.len() {
                            let (_s, _e, px, py, _pw, _ph) = char_positions[pos_idx];
                            let glyph_id = font.glyph_id(ch);
                            let glyph = glyph_id.with_scale(raster_scale);
                            if let Some(outlined) = raster_font.outline_glyph(glyph) {
                                let bounds = outlined.px_bounds();
                                let gx = px * RASTER_SCALE as f32 + bounds.min.x;
                                let gy = py * RASTER_SCALE as f32 + bounds.min.y;
                                outlined.draw(|dx, dy, c| {
                                    let pxx = (gx + dx as f32) as u32;
                                    let pyy = (gy + dy as f32) as u32;
                                    if pxx < img.width() && pyy < img.height() {
                                        let old = img.get_pixel(pxx, pyy).0[0] as f32;
                                        let newv = (old * (1.0 - c) + 20.0 * c) as u8;
                                        img.put_pixel(pxx, pyy, Rgb([newv, newv, newv]));
                                    }
                                });
                            }
                            _x += scaled.h_advance(glyph_id);
                        }
                        pos_idx += 1;
                    }
                    _y += LINE_H;
                }

                let mut out = Vec::new();
                image::DynamicImage::ImageRgb8(img)
                    .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;

                let mapping = serde_json::json!({
                    "version": 1,
                    "renderer": "text",
                    "char_rects": char_positions.iter().map(|(s,e,x,y,w,h)| serde_json::json!({"start":s,"end":e,"x":x,"y":y,"width":w,"height":h})).collect::<Vec<_>>()
                });

                let label = target.label.clone().unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or("text".into())
                });

                return Ok(SnapshotDraft {
                    width: logical_w,
                    height: logical_h,
                    mime_type: "image/png".to_string(),
                    asset_bytes: out,
                    mapping,
                    source_label: label,
                });
            }
        }

        // fallback (no font): original fixed width logic + boxes
        let mut char_positions: Vec<(usize, usize, f32, f32, f32, f32)> = vec![];
        let mut current_y = PADDING as f32;
        let mut x = PADDING as f32;
        let mut byte_offset = 0usize;

        for line_content in content.lines() {
            let mut col = 0usize;
            for ch in line_content.chars() {
                if col >= MAX_CHARS_PER_LINE {
                    current_y += LINE_H;
                    x = PADDING as f32;
                    col = 0;
                }
                let ch_bytes = ch.len_utf8();
                let start = byte_offset;
                byte_offset += ch_bytes;
                char_positions.push((start, byte_offset, x, current_y, CHAR_W, CHAR_H));
                x += CHAR_W;
                col += 1;
            }
            byte_offset += 1;
            current_y += LINE_H;
            x = PADDING as f32;
        }

        let img_w = (PADDING as f32 + MAX_CHARS_PER_LINE as f32 * CHAR_W) as u32;
        let img_h = (current_y + PADDING as f32) as u32;
        let logical_h = img_h.max(20);
        let mut img = RgbImage::new(img_w * RASTER_SCALE, logical_h * RASTER_SCALE);
        for p in img.pixels_mut() {
            *p = Rgb([250, 250, 250]);
        }

        for (_s, _e, px, py, pw, ph) in &char_positions {
            let ix = (*px as u32) * RASTER_SCALE;
            let iy = (*py as u32) * RASTER_SCALE;
            let iw = ((*pw as u32).max(1)) * RASTER_SCALE;
            let ih = ((*ph as u32).max(1)) * RASTER_SCALE;
            for dy in 0..ih {
                for dx in 0..iw {
                    if ix + dx < img.width() && iy + dy < img.height() {
                        img.put_pixel(ix + dx, iy + dy, Rgb([40, 40, 40]));
                    }
                }
            }
        }

        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;

        let mapping = serde_json::json!({
            "version": 1,
            "renderer": "text",
            "char_rects": char_positions.iter().map(|(s,e,x,y,w,h)| serde_json::json!({"start":s,"end":e,"x":x,"y":y,"width":w,"height":h})).collect::<Vec<_>>()
        });

        let label = target.label.clone().unwrap_or_else(|| {
            std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or("text".into())
        });

        Ok(SnapshotDraft {
            width: img_w,
            height: logical_h,
            mime_type: "image/png".to_string(),
            asset_bytes: out,
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
        let rects = mapping["char_rects"]
            .as_array()
            .ok_or_else(|| RenderError::Resolve("bad map".into()))?;
        let label = &snapshot.source_label;

        match selection {
            Selection::Point { x, y } => {
                let mut best = None;
                let mut bd = f64::MAX;
                for r in rects {
                    let cx = r["x"].as_f64().unwrap_or(0.);
                    let cy = r["y"].as_f64().unwrap_or(0.);
                    let d = ((cx - *x).powi(2) + (cy - *y).powi(2)).sqrt();
                    if d < bd {
                        bd = d;
                        best = Some(r);
                    }
                }
                let r = best.ok_or(RenderError::InvalidSelection)?;
                let s = r["start"].as_u64().unwrap_or(0) as usize;
                let e = r["end"].as_u64().unwrap_or(0) as usize;
                Ok(ResolvedReference {
                    description: format!("{}: near byte offsets {}–{}", label, s, e),
                    anchor: serde_json::json!({"kind":"text_byte","range":[s,e]}),
                })
            }
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => {
                let x2 = x + width;
                let y2 = y + height;
                let mut min_s = usize::MAX;
                let mut max_e = 0;
                for r in rects {
                    let cx = r["x"].as_f64().unwrap_or(0.);
                    let cy = r["y"].as_f64().unwrap_or(0.);
                    let cw = r["width"].as_f64().unwrap_or(0.);
                    let ch = r["height"].as_f64().unwrap_or(0.);
                    if cx + cw >= *x && cx <= x2 && cy + ch >= *y && cy <= y2 {
                        let s = r["start"].as_u64().unwrap_or(0) as usize;
                        let e = r["end"].as_u64().unwrap_or(0) as usize;
                        if s < min_s {
                            min_s = s;
                        }
                        if e > max_e {
                            max_e = e;
                        }
                    }
                }
                if min_s == usize::MAX {
                    return Err(RenderError::InvalidSelection);
                }
                Ok(ResolvedReference {
                    description: format!("{}: byte offset range {}–{}", label, min_s, max_e),
                    anchor: serde_json::json!({"kind":"text_byte","range":[min_s,max_e]}),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn text_png_is_twice_logical_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "retake test").unwrap();
        let target = Target {
            kind: TargetKind::Text,
            path: Some(path.to_string_lossy().into_owned()),
            url: None,
            label: None,
            viewport: None,
            metadata: None,
        };
        let draft = TextRenderer::new().capture(&target).await.unwrap();
        let png = image::load_from_memory(&draft.asset_bytes).unwrap();
        assert_eq!(png.width(), draft.width * RASTER_SCALE);
        assert_eq!(png.height(), draft.height * RASTER_SCALE);
        assert!(!draft.mapping["char_rects"].as_array().unwrap().is_empty());
    }
}
