use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{imageops, ImageFormat, Rgba, RgbaImage};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use serde_json::{json, Value};
use std::{io::Cursor, path::Path, process::Stdio};
use tempfile::TempDir;
use tokio::{
    process::Command,
    time::{timeout, Duration},
};

const TILE_W: u32 = 400;
const TILE_H: u32 = 225;
const LABEL_H: u32 = 28;
const GAP: u32 = 16;
const SCALE: u32 = 2;
const COLS: u32 = 3;

/// A fixed storyboard of sampled frames. Annotations refer to timecodes and
/// source-video coordinates; no video stream needs to be served to the UI.
pub struct VideoRenderer;

impl VideoRenderer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for VideoRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Renderer for VideoRenderer {
    fn id(&self) -> &'static str {
        "video"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Video {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_deref()
            .ok_or_else(|| RenderError::Capture("video path required".into()))?;
        let meta = std::fs::metadata(path)?;
        if !meta.is_file() || meta.len() > 500 * 1024 * 1024 {
            return Err(RenderError::Capture(
                "video must be a file of at most 500 MiB".into(),
            ));
        }
        let probe_bin = std::env::var("RETAKE_FFPROBE").unwrap_or_else(|_| "ffprobe".into());
        let output = run(
            Command::new(probe_bin).args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
                path,
            ]),
            Duration::from_secs(15),
        )
        .await?;
        let probe: Value = serde_json::from_slice(&output)
            .map_err(|e| RenderError::Capture(format!("invalid ffprobe response: {e}")))?;
        let stream = probe["streams"]
            .as_array()
            .and_then(|s| s.iter().find(|v| v["codec_type"] == "video"))
            .ok_or_else(|| RenderError::Capture("no video stream".into()))?;
        let src_w = stream["width"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 8192)
            .ok_or_else(|| RenderError::Capture("invalid video width".into()))?
            as u32;
        let src_h = stream["height"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 8192)
            .ok_or_else(|| RenderError::Capture("invalid video height".into()))?
            as u32;
        let duration = probe["format"]["duration"]
            .as_str()
            .or_else(|| stream["duration"].as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|d| d.is_finite() && *d > 0.0 && *d <= 7200.0)
            .ok_or_else(|| RenderError::Capture("video duration must be 0–120 minutes".into()))?;

        let start = target
            .metadata
            .as_ref()
            .and_then(|m| m["start_seconds"].as_f64())
            .unwrap_or(0.0);
        let end = target
            .metadata
            .as_ref()
            .and_then(|m| m["end_seconds"].as_f64())
            .unwrap_or(duration);
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end > duration || end <= start {
            return Err(RenderError::Capture(
                "invalid video review time range".into(),
            ));
        }
        let window = end - start;
        // One frame per second for clips up to one minute; longer clips are
        // evenly sampled. Review a specific segment for denser long-form notes.
        let count = (window.ceil() as u32).clamp(4, 64);
        let rows = count.div_ceil(COLS);
        let width = GAP + COLS * (TILE_W + GAP);
        let height = GAP + rows * (TILE_H + LABEL_H + GAP);
        let mut sheet =
            RgbaImage::from_pixel(width * SCALE, height * SCALE, Rgba([17, 21, 28, 255]));
        let tmp = TempDir::new()?;
        let ffmpeg_bin = std::env::var("RETAKE_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
        let font = [
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok());
        let mut frames = Vec::new();
        for i in 0..count {
            let seconds = start + window * (i as f64 + 0.5) / count as f64;
            let t_ms = (seconds * 1000.0).round() as u64;
            let out = tmp.path().join(format!("frame-{i:02}.png"));
            let seek = format!("{seconds:.3}");
            let filter = format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=0x11151c", TILE_W*SCALE, TILE_H*SCALE, TILE_W*SCALE, TILE_H*SCALE);
            run(
                Command::new(&ffmpeg_bin)
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-nostdin",
                        "-ss",
                        &seek,
                        "-i",
                        path,
                        "-frames:v",
                        "1",
                        "-vf",
                        &filter,
                        "-y",
                    ])
                    .arg(&out),
                Duration::from_secs(20),
            )
            .await?;
            let frame = image::open(&out)?.into_rgba8();
            if frame.width() != TILE_W * SCALE || frame.height() != TILE_H * SCALE {
                return Err(RenderError::Capture(
                    "unexpected extracted frame dimensions".into(),
                ));
            }
            let x = GAP + (i % COLS) * (TILE_W + GAP);
            let y = GAP + (i / COLS) * (TILE_H + LABEL_H + GAP);
            imageops::overlay(
                &mut sheet,
                &frame,
                (x * SCALE) as i64,
                ((y + LABEL_H) * SCALE) as i64,
            );
            draw_label(
                &mut sheet,
                font.as_deref(),
                x,
                y,
                &format!(
                    "{:02}:{:02}.{:01}  /  {:02}",
                    t_ms / 60_000,
                    (t_ms / 1000) % 60,
                    (t_ms % 1000) / 100,
                    i + 1
                ),
            );
            frames.push(json!({"index":i,"time_ms":t_ms,"x":x,"y":y,"width":TILE_W,"height":TILE_H+LABEL_H}));
        }
        let mut asset_bytes = Vec::new();
        sheet.write_to(&mut Cursor::new(&mut asset_bytes), ImageFormat::Png)?;
        let label = target.label.clone().unwrap_or_else(|| {
            Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        Ok(SnapshotDraft {
            width,
            height,
            mime_type: "image/png".into(),
            asset_bytes,
            mapping: json!({"version":1,"renderer":"video","kind":"video","duration_ms":(duration*1000.0).round() as u64,"start_ms":(start*1000.0).round() as u64,"end_ms":(end*1000.0).round() as u64,"source_width":src_w,"source_height":src_h,"label_height":LABEL_H,"frames":frames}),
            source_label: label,
        })
    }

    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError> {
        let map: Value = serde_json::from_str(&snapshot.mapping_json)
            .map_err(|e| RenderError::Resolve(e.to_string()))?;
        let frames = map["frames"]
            .as_array()
            .ok_or_else(|| RenderError::Resolve("video frames missing".into()))?;
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
        let hits: Vec<&Value> = frames
            .iter()
            .filter(|f| {
                let (fx, fy, fw, fh) = (
                    f["x"].as_f64().unwrap_or(0.0),
                    f["y"].as_f64().unwrap_or(0.0),
                    f["width"].as_f64().unwrap_or(0.0),
                    f["height"].as_f64().unwrap_or(0.0),
                );
                match selection {
                    Selection::Point { .. } => x >= fx && x <= fx + fw && y >= fy && y <= fy + fh,
                    Selection::Rect {
                        x,
                        y,
                        width,
                        height,
                    } => *x < fx + fw && *x + *width > fx && *y < fy + fh && *y + *height > fy,
                }
            })
            .collect();
        if hits.len() > 1 {
            let start = hits
                .iter()
                .filter_map(|f| f["time_ms"].as_u64())
                .min()
                .unwrap_or(0);
            let end = hits
                .iter()
                .filter_map(|f| f["time_ms"].as_u64())
                .max()
                .unwrap_or(start);
            return Ok(ResolvedReference {
                description: format!(
                    "{}: video {}–{} (selection spans {} frames)",
                    snapshot.source_label,
                    timecode(start),
                    timecode(end),
                    hits.len()
                ),
                anchor: json!({"kind":"video_range","start_ms":start,"end_ms":end,"selection":selection}),
            });
        }
        let frame = hits
            .first()
            .copied()
            .or_else(|| {
                frames.iter().min_by(|a, b| {
                    let dist = |f: &Value| {
                        let fx = f["x"].as_f64().unwrap_or(0.0);
                        let fy = f["y"].as_f64().unwrap_or(0.0);
                        let fw = f["width"].as_f64().unwrap_or(0.0);
                        let fh = f["height"].as_f64().unwrap_or(0.0);
                        (fx - x).max(0.0).max(x - fx - fw).powi(2)
                            + (fy - y).max(0.0).max(y - fy - fh).powi(2)
                    };
                    dist(a).total_cmp(&dist(b))
                })
            })
            .ok_or_else(|| RenderError::Resolve("video has no sampled frames".into()))?;
        let ms = frame["time_ms"].as_u64().unwrap_or(0);
        let fx = frame["x"].as_f64().unwrap_or(0.0);
        let fy = frame["y"].as_f64().unwrap_or(0.0)
            + map["label_height"].as_f64().unwrap_or(LABEL_H as f64);
        let sw = map["source_width"].as_f64().unwrap_or(TILE_W as f64);
        let sh = map["source_height"].as_f64().unwrap_or(TILE_H as f64);
        let fit = (TILE_W as f64 / sw).min(TILE_H as f64 / sh);
        let (ox, oy) = (
            (TILE_W as f64 - sw * fit) / 2.0,
            (TILE_H as f64 - sh * fit) / 2.0,
        );
        let px = ((x - fx - ox) / fit).clamp(0.0, sw);
        let py = ((y - fy - oy) / fit).clamp(0.0, sh);
        Ok(ResolvedReference {
            description: format!(
                "{}: video frame at {} ({:.0}, {:.0})",
                snapshot.source_label,
                timecode(ms),
                px,
                py
            ),
            anchor: json!({"kind":"video_frame","time_ms":ms,"index":frame["index"],"source_point":{"x":px,"y":py},"selection":selection}),
        })
    }
}

async fn run(command: &mut Command, limit: Duration) -> Result<Vec<u8>, RenderError> {
    command.stdin(Stdio::null()).kill_on_drop(true);
    let output = timeout(limit, command.output())
        .await
        .map_err(|_| RenderError::Capture("video command timed out".into()))?
        .map_err(|e| RenderError::Capture(format!("ffmpeg/ffprobe unavailable: {e}")))?;
    if !output.status.success() {
        return Err(RenderError::Capture(format!(
            "video command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn timecode(ms: u64) -> String {
    format!(
        "{:02}:{:02}.{:01}",
        ms / 60_000,
        (ms / 1000) % 60,
        (ms % 1000) / 100
    )
}

fn draw_label(img: &mut RgbaImage, font: Option<&[u8]>, x: u32, y: u32, text: &str) {
    let Some(font) = font.and_then(|b| FontRef::try_from_slice(b).ok()) else {
        return;
    };
    let scale = PxScale::from(17.0 * SCALE as f32);
    let scaled = font.as_scaled(scale);
    let mut pen = (x + 4) * SCALE;
    let baseline = (y + 22) * SCALE;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen as f32, baseline as f32));
        if let Some(outline) = scaled.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|dx, dy, alpha| {
                let ix = bounds.min.x as i32 + dx as i32;
                let iy = bounds.min.y as i32 + dy as i32;
                if ix >= 0 && iy >= 0 && (ix as u32) < img.width() && (iy as u32) < img.height() {
                    let p = img.get_pixel_mut(ix as u32, iy as u32);
                    for c in 0..3 {
                        p.0[c] = (p.0[c] as f32 * (1.0 - alpha) + 230.0 * alpha) as u8;
                    }
                }
            });
        }
        pen += (scaled.h_advance(id).ceil() as u32).max(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> StoredSnapshot {
        StoredSnapshot{id:"s".into(),review_id:"r".into(),position:0,renderer_id:"video".into(),mapping_version:1,width:1264,height:285,mime_type:"image/png".into(),asset_path:"".into(),source_label:"clip.mp4".into(),mapping_json:json!({
            "label_height":28,"source_width":1120,"source_height":630,
            "frames":[{"index":0,"time_ms":1000,"x":16,"y":16,"width":400,"height":253},{"index":1,"time_ms":3000,"x":432,"y":16,"width":400,"height":253}]
        }).to_string()}
    }

    #[test]
    fn resolves_single_frame_and_range() {
        let s = snap();
        let r = VideoRenderer::new();
        let point = r
            .resolve(&s, &Selection::Point { x: 216.0, y: 156.5 })
            .unwrap();
        assert_eq!(point.anchor["time_ms"], 1000);
        assert!((point.anchor["source_point"]["x"].as_f64().unwrap() - 560.0).abs() < 1.0);
        let range = r
            .resolve(
                &s,
                &Selection::Rect {
                    x: 350.0,
                    y: 100.0,
                    width: 200.0,
                    height: 60.0,
                },
            )
            .unwrap();
        assert_eq!(range.anchor["kind"], "video_range");
        assert_eq!(range.anchor["start_ms"], 1000);
        assert_eq!(range.anchor["end_ms"], 3000);
    }

    #[tokio::test]
    async fn captures_short_clip_when_ffmpeg_available() {
        if Command::new("ffmpeg")
            .arg("-version")
            .output()
            .await
            .is_err()
            || Command::new("ffprobe")
                .arg("-version")
                .output()
                .await
                .is_err()
        {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("sample.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=5:duration=2",
                "-c:v",
                "mpeg4",
                "-y",
            ])
            .arg(&video)
            .status()
            .await
            .unwrap();
        assert!(status.success());
        let draft = VideoRenderer::new()
            .capture(&Target {
                kind: TargetKind::Video,
                path: Some(video.to_string_lossy().into()),
                url: None,
                label: None,
                viewport: None,
                metadata: None,
            })
            .await
            .unwrap();
        assert_eq!(draft.mapping["frames"].as_array().unwrap().len(), 4);
        assert_eq!(draft.mapping["source_width"], 160);
        assert_eq!(draft.mapping["source_height"], 90);
        let img = image::load_from_memory(&draft.asset_bytes).unwrap();
        assert_eq!(img.width(), draft.width * SCALE);
        assert_eq!(img.height(), draft.height * SCALE);
    }
}
