use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::fs;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

#[derive(Default)]
pub struct WebRenderer {
    worker_path: String, // path to node script e.g. workers/web-capture/index.js
}

impl WebRenderer {
    pub fn new(worker_path: impl Into<String>) -> Self {
        Self {
            worker_path: worker_path.into(),
        }
    }
}

#[async_trait::async_trait]
impl Renderer for WebRenderer {
    fn id(&self) -> &'static str {
        "web"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Web
            && target.kind != TargetKind::Html
            && target.kind != TargetKind::Markdown
        {
            return Err(RenderError::Unsupported);
        }
        let is_web = target.kind == TargetKind::Web;
        let is_markdown = target.kind == TargetKind::Markdown;
        let url_or_path = if is_web {
            target
                .url
                .clone()
                .ok_or_else(|| RenderError::Capture("no url".into()))?
        } else {
            target
                .path
                .clone()
                .ok_or_else(|| RenderError::Capture("no path".into()))?
        };
        let viewport = target.viewport.clone().unwrap_or(retake_core::Viewport {
            width: 1280,
            height: 800,
        });

        let tmp = TempDir::new()?;
        let out_dir = tmp.path().to_path_buf();
        let req = serde_json::json!({
            "version": 1,
            "kind": if is_web { "web" } else if is_markdown { "markdown" } else { "html" },
            "url": if is_web { url_or_path.clone() } else { format!("file://{}", url_or_path) },
            "path": if !is_web { Some(url_or_path.clone()) } else { None },
            "viewport": { "width": viewport.width, "height": viewport.height },
            "output_dir": out_dir.to_string_lossy().to_string(),
        });

        let req_path = out_dir.join("request.json");
        fs::write(&req_path, serde_json::to_string(&req).unwrap())?;

        // spawn node worker (async)
        let mut child = Command::new("node")
            .arg(&self.worker_path)
            .arg(req_path.to_string_lossy().to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RenderError::Capture(format!("spawn worker: {}", e)))?;

        // wait with inner timeout so we can kill on expiry
        let wait_res = timeout(Duration::from_secs(30), child.wait()).await;
        let status = match wait_res {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(RenderError::Capture(e.to_string())),
            Err(_) => {
                // timeout, kill
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(RenderError::Capture(
                    "web capture timeout, process killed".into(),
                ));
            }
        };
        if !status.success() {
            let err = if let Some(mut stderr) = child.stderr.take() {
                let mut b = vec![];
                let _ = stderr.read_to_end(&mut b).await;
                String::from_utf8_lossy(&b).to_string()
            } else {
                String::new()
            };
            return Err(RenderError::Capture(format!("worker failed: {}", err)));
        }

        let resp_path = out_dir.join("response.json");
        let resp: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&resp_path).unwrap_or_default())
                .map_err(|e| RenderError::Capture(format!("bad resp: {}", e)))?;

        let png_path = resp["png_path"]
            .as_str()
            .ok_or_else(|| RenderError::Capture("no png".into()))?
            .to_string();
        let png_bytes = fs::read(&png_path)?;
        let w = resp["width"].as_u64().unwrap_or(1280) as u32;
        let h = resp["height"].as_u64().unwrap_or(800) as u32;
        let final_url = resp["final_url"]
            .as_str()
            .unwrap_or(&url_or_path)
            .to_string();
        let title = resp["title"].as_str().unwrap_or("").to_string();

        let nodes = resp.get("nodes").cloned().unwrap_or(serde_json::json!([]));

        let mapping = serde_json::json!({
            "version": 1,
            "renderer": "web",
            "kind": if is_markdown { "markdown" } else if is_web { "web" } else { "html" },
            "final_url": final_url,
            "title": title,
            "viewport": {"width": w, "height": h},
            "nodes": nodes,
        });

        let label = target.label.clone().unwrap_or_else(|| {
            if is_markdown {
                std::path::Path::new(&url_or_path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "markdown".to_owned())
            } else {
                final_url.clone()
            }
        });

        Ok(SnapshotDraft {
            width: w,
            height: h,
            mime_type: "image/png".to_string(),
            asset_bytes: png_bytes,
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
        let label = &snapshot.source_label;

        // find hit candidates
        let (sx, sy, sw, sh) = match selection {
            Selection::Point { x, y } => (*x, *y, 0.0, 0.0),
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => (*x, *y, *width, *height),
        };
        let mut candidates: Vec<serde_json::Value> = vec![];
        for n in &nodes {
            if let (Some(rx), Some(ry), Some(rw), Some(rh)) = (
                n["rect"]["x"].as_f64(),
                n["rect"]["y"].as_f64(),
                n["rect"]["width"].as_f64(),
                n["rect"]["height"].as_f64(),
            ) {
                let intersects = if sw == 0.0 && sh == 0.0 {
                    sx >= rx && sx <= rx + rw && sy >= ry && sy <= ry + rh
                } else {
                    !(rx + rw < sx || rx > sx + sw || ry + rh < sy || ry > sy + sh)
                };
                if intersects {
                    candidates.push(n.clone());
                }
            }
        }
        candidates.sort_by_key(|c| c["depth"].as_i64().unwrap_or(999));

        let markdown_line = if mapping["kind"] == "markdown" {
            candidates
                .iter()
                .filter_map(|candidate| candidate["attributes"]["data-source-line"].as_str())
                .filter_map(|line| line.parse::<usize>().ok())
                .min()
                .or_else(|| {
                    // A click on whitespace between blocks still refers to
                    // the closest rendered Markdown block.
                    nodes
                        .iter()
                        .filter_map(|node| {
                            let line = node["attributes"]["data-source-line"]
                                .as_str()?
                                .parse::<usize>()
                                .ok()?;
                            let x = node["rect"]["x"].as_f64()?;
                            let y = node["rect"]["y"].as_f64()?;
                            let w = node["rect"]["width"].as_f64()?;
                            let h = node["rect"]["height"].as_f64()?;
                            let dx = if sx < x {
                                x - sx
                            } else if sx > x + w {
                                sx - x - w
                            } else {
                                0.0
                            };
                            let dy = if sy < y {
                                y - sy
                            } else if sy > y + h {
                                sy - y - h
                            } else {
                                0.0
                            };
                            Some((dx * dx + dy * dy, line))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, line)| line)
                })
        } else {
            None
        };
        let desc = if let Some(line) = markdown_line {
            format!("{}: Markdown near line {}", label, line)
        } else if candidates.is_empty() {
            format!("{}: viewport area ({:.0},{:.0})", label, sx, sy)
        } else if candidates.len() == 1 {
            let c = &candidates[0];
            let tag = c["tag"].as_str().unwrap_or("element");
            let text = c["text"].as_str().unwrap_or("");
            let xpath = c["xpath"].as_str().unwrap_or("");
            format!("{}: {} ({}) {}", label, tag, text, xpath)
        } else {
            let top = &candidates[0];
            format!(
                "{}: multiple elements in region (frontmost: {})",
                label,
                top["tag"].as_str().unwrap_or("")
            )
        };

        Ok(ResolvedReference {
            description: desc,
            anchor: serde_json::json!({ "kind": "web_dom", "selection": selection, "candidates": candidates.iter().take(3).collect::<Vec<_>>() }),
        })
    }
}
