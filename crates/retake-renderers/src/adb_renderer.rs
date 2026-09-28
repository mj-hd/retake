use image::{GenericImageView, ImageFormat, ImageReader};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::io::Cursor;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Android screen via `adb`. Capture is a PNG of the current display plus the
/// UIAutomator node tree from that moment. Comments resolve against the stored
/// tree; the device is not queried again at submit time.
pub struct AdbRenderer {
    adb: String,
}

impl AdbRenderer {
    pub fn new() -> Self {
        Self {
            adb: std::env::var("RETAKE_ADB").unwrap_or_else(|_| "adb".into()),
        }
    }
}

impl Default for AdbRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Renderer for AdbRenderer {
    fn id(&self) -> &'static str {
        "adb"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Adb {
            return Err(RenderError::Unsupported);
        }
        let serial = target
            .path
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| RenderError::Capture("adb serial required in path".into()))?;
        if !is_safe_serial(serial) {
            return Err(RenderError::Capture("invalid adb serial".into()));
        }

        let png = adb(&self.adb, serial, &["exec-out", "screencap", "-p"]).await?;
        let img = ImageReader::new(Cursor::new(&png))
            .with_guessed_format()
            .map_err(|e| RenderError::Capture(e.to_string()))?
            .decode()
            .map_err(|e| RenderError::Capture(e.to_string()))?;
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return Err(RenderError::Capture("empty screencap".into()));
        }

        let dump = adb(
            &self.adb,
            serial,
            &["exec-out", "uiautomator", "dump", "/dev/tty"],
        )
        .await
        .unwrap_or_default();
        let xml = extract_xml(&dump);
        let nodes = parse_nodes(xml);

        let mut asset_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut asset_bytes), ImageFormat::Png)?;

        let label = target
            .label
            .clone()
            .unwrap_or_else(|| format!("Android {}", serial));
        let mapping = serde_json::json!({
            "version": 1,
            "renderer": "adb",
            "serial": serial,
            "nodes": nodes,
        });

        Ok(SnapshotDraft {
            width,
            height,
            mime_type: "image/png".to_string(),
            asset_bytes,
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
        let nodes = mapping["nodes"].as_array().cloned().unwrap_or_default();
        let (sx, sy, sw, sh) = match selection {
            Selection::Point { x, y } => (*x, *y, 0.0, 0.0),
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => (*x, *y, *width, *height),
        };

        let mut hits: Vec<&serde_json::Value> = nodes
            .iter()
            .filter(|n| {
                let (rx, ry, rw, rh) = match rect_of(n) {
                    Some(r) => r,
                    None => return false,
                };
                if sw == 0.0 && sh == 0.0 {
                    sx >= rx && sx <= rx + rw && sy >= ry && sy <= ry + rh
                } else {
                    !(rx + rw < sx || rx > sx + sw || ry + rh < sy || ry > sy + sh)
                }
            })
            .collect();
        // Smaller nodes are the more specific UI elements.
        hits.sort_by(|a, b| {
            let area =
                |n: &serde_json::Value| rect_of(n).map(|(_, _, w, h)| w * h).unwrap_or(f64::MAX);
            area(a).total_cmp(&area(b))
        });

        let label = &snapshot.source_label;
        let (description, node) = if let Some(n) = hits.first() {
            let text = n["text"].as_str().unwrap_or("");
            let class = n["class"].as_str().unwrap_or("node");
            let id = n["resource_id"].as_str().unwrap_or("");
            let desc = if text.is_empty() && id.is_empty() {
                format!("{}: {}", label, class)
            } else if id.is_empty() {
                format!("{}: \"{}\" ({})", label, text, class)
            } else {
                format!("{}: \"{}\" {}", label, text, id)
            };
            (desc, Some((*n).clone()))
        } else {
            (
                format!("{}: screen position ({:.0},{:.0})", label, sx, sy),
                None,
            )
        };

        Ok(ResolvedReference {
            description,
            anchor: serde_json::json!({
                "kind": "adb_node",
                "serial": mapping["serial"],
                "selection": selection,
                "node": node,
            }),
        })
    }
}

fn is_safe_serial(serial: &str) -> bool {
    !serial.is_empty()
        && serial.len() <= 64
        && serial
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == ':' || c == '-' || c == '_')
}

fn extract_xml(dump: &[u8]) -> &str {
    let text = String::from_utf8_lossy(dump);
    let start = text.find('<').unwrap_or(0);
    let end = text.rfind('>').map(|i| i + 1).unwrap_or(text.len());
    // from_utf8_lossy may have replaced bytes; re-slice the original only when it is valid.
    std::str::from_utf8(&dump[start..end]).unwrap_or("")
}

fn parse_nodes(xml: &str) -> Vec<serde_json::Value> {
    let mut nodes = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<node ") {
        rest = &rest[start..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if let Some(bounds) = attr(tag, "bounds") {
            if let Some((x, y, x2, y2)) = parse_bounds(&bounds) {
                nodes.push(serde_json::json!({
                    "class": attr(tag, "class").unwrap_or_default(),
                    "text": attr(tag, "text").unwrap_or_default(),
                    "resource_id": attr(tag, "resource-id").unwrap_or_default(),
                    "content_desc": attr(tag, "content-desc").unwrap_or_default(),
                    "rect": { "x": x, "y": y, "width": (x2 - x).max(0.0), "height": (y2 - y).max(0.0) },
                }));
            }
        }
        rest = &rest[end.min(rest.len())..];
        if nodes.len() >= 2000 {
            break;
        }
    }
    nodes
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{}=\"", name);
    let start = tag.find(&key)? + key.len();
    let value = &tag[start..];
    let end = value.find('"')?;
    Some(unescape(&value[..end]))
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn parse_bounds(bounds: &str) -> Option<(f64, f64, f64, f64)> {
    // [x,y][x2,y2]
    let parts: Vec<&str> = bounds
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 4 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
        parts[3].parse().ok()?,
    ))
}

fn rect_of(node: &serde_json::Value) -> Option<(f64, f64, f64, f64)> {
    Some((
        node["rect"]["x"].as_f64()?,
        node["rect"]["y"].as_f64()?,
        node["rect"]["width"].as_f64()?,
        node["rect"]["height"].as_f64()?,
    ))
}

async fn adb(bin: &str, serial: &str, args: &[&str]) -> Result<Vec<u8>, RenderError> {
    if Path::new(bin).is_absolute() && !Path::new(bin).is_file() {
        return Err(RenderError::Capture(format!("adb not found: {}", bin)));
    }
    let mut cmd = Command::new(bin);
    cmd.arg("-s")
        .arg(serial)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| RenderError::Capture(format!("spawn adb: {}", e)))?;
    let wait = timeout(Duration::from_secs(20), child.wait())
        .await
        .map_err(|_| RenderError::Capture("adb timed out".into()))?
        .map_err(|e| RenderError::Capture(e.to_string()))?;
    let mut stdout = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use tokio::io::AsyncReadExt;
        let _ = out.read_to_end(&mut stdout).await;
    }
    if !wait.success() {
        let mut err = Vec::new();
        if let Some(mut e) = child.stderr.take() {
            use tokio::io::AsyncReadExt;
            let _ = e.read_to_end(&mut err).await;
        }
        let msg = String::from_utf8_lossy(&err);
        return Err(RenderError::Capture(format!("adb failed: {}", msg.trim())));
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uiautomator_bounds() {
        let xml = r#"<?xml version="1.0"?><hierarchy><node class="android.widget.TextView" text="設定" resource-id="com.example:id/title" bounds="[10,20][110,70]" /></hierarchy>"#;
        let nodes = parse_nodes(xml);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["text"], "設定");
        assert_eq!(nodes[0]["rect"]["width"], 100.0);
        assert_eq!(nodes[0]["resource_id"], "com.example:id/title");
    }

    #[tokio::test]
    async fn capture_and_resolve_with_fake_adb() {
        let dir = tempfile::tempdir().unwrap();
        let adb = dir.path().join("adb");
        let png = {
            let img = image::RgbImage::from_pixel(40, 30, image::Rgb([20, 40, 60]));
            let mut bytes = Vec::new();
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                .unwrap();
            bytes
        };
        let png_path = dir.path().join("screen.png");
        std::fs::write(&png_path, &png).unwrap();
        let script = format!(
            "#!/bin/sh\ncase \"$*\" in\n  *screencap*) cat \"{}\" ;;\n  *uiautomator*) printf '%s' '<hierarchy><node class=\"android.widget.Button\" text=\"OK\" resource-id=\"app:id/ok\" bounds=\"[4,6][24,18]\"/></hierarchy>' ;;\n  *) exit 1 ;;\nesac\n",
            png_path.display()
        );
        std::fs::write(&adb, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        std::env::set_var("RETAKE_ADB", &adb);
        let renderer = AdbRenderer::new();
        let target = Target {
            kind: TargetKind::Adb,
            path: Some("emulator-5554".into()),
            url: None,
            label: Some("test device".into()),
            viewport: None,
            metadata: None,
        };
        let draft = renderer.capture(&target).await.unwrap();
        assert_eq!((draft.width, draft.height), (40, 30));
        assert_eq!(draft.mapping["nodes"][0]["text"], "OK");

        let snap = StoredSnapshot {
            id: "s".into(),
            review_id: "r".into(),
            position: 0,
            renderer_id: "adb".into(),
            mapping_version: 1,
            width: draft.width,
            height: draft.height,
            mime_type: draft.mime_type,
            asset_path: String::new(),
            source_label: draft.source_label,
            mapping_json: draft.mapping.to_string(),
        };
        let hit = renderer
            .resolve(&snap, &Selection::Point { x: 10.0, y: 10.0 })
            .unwrap();
        assert!(hit.description.contains("OK"), "{}", hit.description);
        assert_eq!(hit.anchor["node"]["resource_id"], "app:id/ok");
    }
}
