use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{ImageFormat, Rgb, RgbImage};
use retake_core::{
    RenderError, Renderer, ResolvedReference, RuntimePaths, Selection, SnapshotDraft,
    StoredSnapshot, Target, TargetKind,
};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Source files as one infinite, zoomable field (Tree-sitter backed).
///
/// The review UI draws this dynamically from the stored mapping: far away it
/// is a concept graph (the file frame wrapping its types/functions, neighbor
/// modules, dependencies), zooming in reveals the real source inside each box.
/// The PNG is only a small thumbnail of the same layout.
pub struct CodeRenderer {
    worker: Option<String>,
}

impl CodeRenderer {
    pub fn new(worker: Option<String>) -> Self {
        Self { worker }
    }
}

const CHAR_W: f64 = 6.4;
const LINE_H: f64 = 13.5;
const HEADER_H: f64 = 26.0;
const THUMB_W: u32 = 1200;
const THUMB_H: u32 = 3000;
const SCALE: u32 = 2;
const MAX_LINES: usize = 8000;
const MAX_SYMBOLS: usize = 120;
const MAX_IMPORTS: usize = 60;
const MAX_GROUPS: usize = 6;
const SECONDARY_BODY: usize = 12;

#[derive(Clone, Copy)]
enum CodeLanguage {
    Rust,
    TypeScript,
    Tsx,
}

impl CodeLanguage {
    fn for_path(path: &str) -> Option<Self> {
        match Path::new(path)
            .extension()?
            .to_str()?
            .to_ascii_lowercase()
            .as_str()
        {
            "rs" => Some(Self::Rust),
            "ts" | "js" | "mjs" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" | "jsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    fn worker_name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
        }
    }
}

#[derive(Clone)]
struct SymbolBox {
    name: String,
    kind: String,
    group: String,
    start_line: usize,
    end_line: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    code_x: f64,
    code_top: f64,
    lines_shown: usize,
}

#[derive(Clone)]
struct ImportBox {
    module: String,
    line: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone)]
struct FrameBox {
    file_name: String,
    primary: bool,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Layout {
    width: f64,
    height: f64,
    frames: Vec<FrameBox>,
    imports: Vec<ImportBox>,
    symbols: Vec<SymbolBox>,
}

struct DocData {
    file_name: String,
    map: serde_json::Value,
    source: Vec<String>,
}

#[async_trait::async_trait]
impl Renderer for CodeRenderer {
    fn id(&self) -> &'static str {
        "code"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::Code {
            return Err(RenderError::Unsupported);
        }
        let path = target
            .path
            .as_deref()
            .ok_or_else(|| RenderError::Capture("code path required".into()))?;
        if CodeLanguage::for_path(path).is_none() {
            return Err(RenderError::Unsupported);
        }
        let worker = self
            .worker
            .as_deref()
            .ok_or_else(|| RenderError::Capture("code worker not configured".into()))?;

        let file_name = display_name(path);

        // Primary file plus its directly imported local modules.
        let mut docs = vec![self.load_doc(worker, path, MAX_LINES).await?];
        let local = resolve_local_modules(&docs[0].map, path);
        for neighbor in local.iter().take(MAX_GROUPS) {
            if let Ok(doc) = self.load_doc(worker, neighbor, 1200).await {
                docs.push(doc);
            }
        }

        // Call graph inside the primary file: symbol A mentions symbol B.
        let calls = call_edges(&docs[0].map);

        let layout = plan(&docs);
        let thumb = render_thumbnail(&layout);

        let label = target.label.clone().unwrap_or_else(|| file_name.clone());
        let mut symbol_json: Vec<serde_json::Value> = vec![];
        let mut import_json: Vec<serde_json::Value> = vec![];
        let mut identifier_json: Vec<serde_json::Value> = vec![];
        let mut sources = serde_json::Map::new();
        let mut tokens = serde_json::Map::new();
        for doc in &docs {
            sources.insert(doc.file_name.clone(), serde_json::json!(doc.source));
            if let Some(value) = doc.map.get("tokens") {
                tokens.insert(doc.file_name.clone(), value.clone());
            }
            if let Some(ids) = doc.map["identifiers"].as_array() {
                for id in ids {
                    let mut id = id.clone();
                    id["group"] = serde_json::json!(doc.file_name);
                    identifier_json.push(id);
                }
            }
        }
        for b in &layout.symbols {
            symbol_json.push(serde_json::json!({
                "name": b.name, "kind": b.kind, "group": b.group,
                "line": b.start_line, "end_line": b.end_line,
                "x": b.x, "y": b.y, "width": b.w, "height": b.h,
                "code_x": b.code_x, "code_top": b.code_top, "lines_shown": b.lines_shown,
            }));
        }
        for m in &layout.imports {
            import_json.push(serde_json::json!({
                "module": m.module, "line": m.line,
                "x": m.x, "y": m.y, "width": m.w, "height": m.h,
            }));
        }
        Ok(SnapshotDraft {
            width: layout.width.ceil() as u32,
            height: layout.height.ceil() as u32,
            mime_type: "image/png".to_string(),
            asset_bytes: thumb,
            mapping: serde_json::json!({
                "version": 3,
                "renderer": "code",
                "view": "unified",
                "file_name": file_name,
                "char_width": CHAR_W,
                "line_height": LINE_H,
                "header_height": HEADER_H,
                "canvas": { "width": layout.width, "height": layout.height },
                "frames": layout.frames.iter().map(|f| serde_json::json!({
                    "file_name": f.file_name, "primary": f.primary,
                    "x": f.x, "y": f.y, "width": f.w, "height": f.h,
                })).collect::<Vec<_>>(),
                "imports": import_json,
                "symbols": symbol_json,
                "identifiers": identifier_json,
                "calls": calls,
                "sources": sources,
                "tokens": tokens,
            }),
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
        let (x, y) = match selection {
            Selection::Point { x, y } => (*x, *y),
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => (*x + width / 2.0, *y + height / 2.0),
        };
        let label = &snapshot.source_label;

        if let Some(sym) = hit_box(mapping["symbols"].as_array(), x, y) {
            let name = sym["name"].as_str().unwrap_or("");
            let kind = sym["kind"].as_str().unwrap_or("symbol");
            let group = sym["group"].as_str().unwrap_or("");
            let start = sym["line"].as_u64().unwrap_or(0);
            let end = sym["end_line"].as_u64().unwrap_or(start);
            let code_top = sym["code_top"].as_f64().unwrap_or(0.0);
            let line_h = mapping["line_height"].as_f64().unwrap_or(LINE_H);
            let char_w = mapping["char_width"].as_f64().unwrap_or(CHAR_W);
            let code_x = sym["code_x"].as_f64().unwrap_or(0.0);
            let where_ = if group.is_empty() {
                label.clone()
            } else {
                format!("{} / {}", label, group)
            };
            if y <= code_top {
                return Ok(ResolvedReference {
                    description: format!(
                        "{}: {} \"{}\" (lines {}–{})",
                        where_, kind, name, start, end
                    ),
                    anchor: serde_json::json!({ "kind": "code_symbol", "name": name, "group": group, "range": [start, end] }),
                });
            }
            let row = ((y - code_top) / line_h).round() as i64;
            let line = (start as i64 + row).clamp(start as i64, end as i64) as u64;
            let id = mapping["identifiers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|id| {
                    id["line"].as_u64() == Some(line) && id["group"].as_str().unwrap_or("") == group
                })
                .filter_map(|id| {
                    let col = id["column"].as_f64()?;
                    let text = id["text"].as_str()?;
                    let ix = code_x + col * char_w;
                    let iw = text.chars().count() as f64 * char_w;
                    let dist = if x < ix {
                        ix - x
                    } else if x > ix + iw {
                        x - ix - iw
                    } else {
                        0.0
                    };
                    Some((dist, id))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let description = match id {
                Some((dist, id)) if dist < char_w * 2.0 => format!(
                    "{}: identifier \"{}\" (line {}, column {}, in {})",
                    where_,
                    id["text"].as_str().unwrap_or(""),
                    line,
                    id["column"].as_u64().unwrap_or(0) + 1,
                    name
                ),
                _ => format!("{}: {} \"{}\" near line {}", where_, kind, name, line),
            };
            return Ok(ResolvedReference {
                description,
                anchor: serde_json::json!({ "kind": "code_line", "group": group, "symbol": name, "line": line }),
            });
        }

        if let Some(dep) = hit_box(mapping["imports"].as_array(), x, y) {
            let module = dep["module"].as_str().unwrap_or("");
            let line = dep["line"].as_u64().unwrap_or(0);
            return Ok(ResolvedReference {
                description: format!("{}: dependency \"{}\" (line {})", label, module, line),
                anchor: serde_json::json!({ "kind": "code_import", "module": module, "line": line }),
            });
        }

        Ok(ResolvedReference {
            description: format!("{}: diagram area ({:.0},{:.0})", label, x, y),
            anchor: serde_json::json!({ "kind": "code", "selection": selection }),
        })
    }
}

impl CodeRenderer {
    async fn load_doc(
        &self,
        worker: &str,
        path: &str,
        max_lines: usize,
    ) -> Result<DocData, RenderError> {
        let script = Path::new(worker).with_file_name("code-map.js");
        let language = CodeLanguage::for_path(path).ok_or(RenderError::Unsupported)?;
        let runtime = RuntimePaths::discover();
        let mut command = Command::new(runtime.node_executable());
        runtime.configure_worker_command(&mut command);
        let mut child = command
            .arg(&script)
            .arg(language.worker_name())
            .arg(path)
            .arg(max_lines.to_string())
            .current_dir(script.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RenderError::Capture(format!("spawn code map: {}", e)))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let finished = timeout(Duration::from_secs(60), async {
            use tokio::io::AsyncReadExt;
            if let Some(mut out) = child.stdout.take() {
                let _ = out.read_to_end(&mut stdout).await;
            }
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_end(&mut stderr).await;
            }
            child.wait().await
        })
        .await
        .map_err(|_| RenderError::Capture("code map timed out".into()))?
        .map_err(|e| RenderError::Capture(e.to_string()))?;
        if !finished.success() {
            return Err(RenderError::Capture(format!(
                "tree-sitter map failed: {}",
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        let map: serde_json::Value = serde_json::from_slice(&stdout)
            .map_err(|e| RenderError::Capture(format!("bad code map: {}", e)))?;
        let source: Vec<String> = std::fs::read_to_string(path)
            .map_err(|e| RenderError::Capture(e.to_string()))?
            .lines()
            .take(max_lines)
            .map(|l| l.to_string())
            .collect();
        Ok(DocData {
            file_name: display_name(path),
            map,
            source,
        })
    }
}

/// Unique, human-readable name: `parent/file` for ubiquitous names like
/// `lib.rs`, plain file name otherwise.
fn display_name(path: &str) -> String {
    let p = Path::new(path);
    let base = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    match (p.parent().and_then(|d| d.file_name()), base.as_str()) {
        (Some(dir), "lib.rs" | "main.rs" | "mod.rs" | "index.ts" | "index.tsx" | "index.js") => {
            format!("{}/{}", dir.to_string_lossy(), base)
        }
        _ => base,
    }
}

/// Calls between symbols of the primary file, from AST references.
fn call_edges(map: &serde_json::Value) -> Vec<serde_json::Value> {
    let empty = vec![];
    let symbols = map["symbols"].as_array().unwrap_or(&empty);
    let identifiers = map["identifiers"].as_array().unwrap_or(&empty);
    let mut edges = vec![];
    for sym in symbols {
        let name = sym["name"].as_str().unwrap_or("");
        let start = sym["line"].as_u64().unwrap_or(0);
        let end = sym["end_line"].as_u64().unwrap_or(start);
        for other in symbols {
            let other_name = other["name"].as_str().unwrap_or("");
            if other_name == name {
                continue;
            }
            let hits = identifiers
                .iter()
                .filter(|id| {
                    id["text"].as_str() == Some(other_name)
                        && id["line"]
                            .as_u64()
                            .map(|l| l >= start && l <= end)
                            .unwrap_or(false)
                })
                .count();
            if hits > 0
                && !edges
                    .iter()
                    .any(|e: &serde_json::Value| e["from"] == name && e["to"] == other_name)
            {
                edges.push(serde_json::json!({ "from": name, "to": other_name }));
            }
        }
    }
    edges
}

/// Import specifiers that resolve to local source files next to `from`.
fn resolve_local_modules(map: &serde_json::Value, from: &str) -> Vec<String> {
    let from_path = Path::new(from);
    let dir = from_path.parent().unwrap_or(Path::new("."));
    // Cargo workspace root: prefer the ancestor whose Cargo.toml declares
    // [workspace]; fall back to the topmost Cargo.toml found.
    let mut root = dir.to_path_buf();
    let mut fallback = None;
    for _ in 0..8 {
        let manifest = root.join("Cargo.toml");
        if manifest.is_file() {
            fallback = Some(root.clone());
            if std::fs::read_to_string(&manifest)
                .map(|s| s.contains("[workspace]"))
                .unwrap_or(false)
            {
                break;
            }
        }
        if !root.pop() {
            break;
        }
    }
    let root = fallback.unwrap_or(root);
    let is_rust = from.ends_with(".rs");
    let mut out: Vec<String> = vec![];
    for imp in map["imports"].as_array().into_iter().flatten() {
        if out.len() >= MAX_GROUPS {
            break;
        }
        let module = imp["module"].as_str().unwrap_or("");
        let mut candidates: Vec<PathBuf> = if module.starts_with('.') {
            if is_rust {
                continue;
            }
            let base = dir.join(module.trim_start_matches("./"));
            vec![
                base.with_extension("ts"),
                base.with_extension("tsx"),
                base.with_extension("js"),
                base.with_extension("jsx"),
                base.join("index.ts"),
                base.join("index.tsx"),
            ]
        } else if is_rust {
            let rel = module
                .trim_start_matches("crate::")
                .trim_start_matches("self::");
            let rel = match rel.strip_prefix("super::") {
                Some(rest) => PathBuf::from("..").join(rest.replace("::", "/")),
                None => PathBuf::from(rel.replace("::", "/")),
            };
            let base = dir.join(&rel);
            // Sibling crate: crates/<name>/src/lib.rs (hyphenated or not).
            let crate_name = module.split("::").next().unwrap_or(module);
            let hyphenated = crate_name.replace('_', "-");
            let mut c = vec![base.with_extension("rs"), base.join("mod.rs")];
            for name in [crate_name.to_string(), hyphenated] {
                for lib in ["lib.rs", "main.rs"] {
                    c.push(root.join("crates").join(&name).join("src").join(lib));
                    c.push(root.join(&name).join("src").join(lib));
                }
            }
            c
        } else {
            continue;
        };
        candidates.dedup();
        for cand in candidates {
            if cand.is_file() {
                let resolved = cand.to_string_lossy().into_owned();
                if resolved != from && !out.contains(&resolved) {
                    out.push(resolved);
                }
                break;
            }
        }
    }
    out
}

fn hit_box(nodes: Option<&Vec<serde_json::Value>>, x: f64, y: f64) -> Option<&serde_json::Value> {
    nodes?
        .iter()
        .filter(|n| {
            let (bx, by, bw, bh) = (
                n["x"].as_f64().unwrap_or(0.0),
                n["y"].as_f64().unwrap_or(0.0),
                n["width"].as_f64().unwrap_or(0.0),
                n["height"].as_f64().unwrap_or(0.0),
            );
            x >= bx && x <= bx + bw && y >= by && y <= by + bh
        })
        .min_by(|a, b| {
            let area = |n: &serde_json::Value| {
                n["width"].as_f64().unwrap_or(f64::MAX) * n["height"].as_f64().unwrap_or(f64::MAX)
            };
            area(a).total_cmp(&area(b))
        })
}

/// Layout: the primary file frame wraps its symbols (force-placed), neighbor
/// module frames sit in a row below, dependencies live on the outer shell.
fn plan(docs: &[DocData]) -> Layout {
    let primary = &docs[0];
    let total_lines = primary.source.len().max(1);

    #[derive(Clone, Copy)]
    struct SimNode {
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
        vx: f64,
        vy: f64,
    }

    let hub = SimNode {
        cx: 0.0,
        cy: 0.0,
        w: 300.0,
        h: 40.0,
        vx: 0.0,
        vy: 0.0,
    };
    let sym_w = 720.0;
    let imp_w = 300.0;

    let mut sym_meta: Vec<(usize, usize, String, String, String)> = vec![];
    let mut nodes: Vec<SimNode> = vec![];
    let mut symbols: Vec<serde_json::Value> = primary.map["symbols"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    symbols.truncate(MAX_SYMBOLS);
    for sym in &symbols {
        let (start, shown) = body_lines(sym, total_lines, 600);
        sym_meta.push((
            start,
            shown,
            sym["name"].as_str().unwrap_or("").to_string(),
            sym["kind"].as_str().unwrap_or("symbol").to_string(),
            primary.file_name.clone(),
        ));
        nodes.push(SimNode {
            cx: 0.0,
            cy: 0.0,
            w: sym_w,
            h: HEADER_H + 14.0 + shown as f64 * LINE_H,
            vx: 0.0,
            vy: 0.0,
        });
    }
    let imp_start = nodes.len();
    let imports: Vec<serde_json::Value> = primary.map["imports"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for _ in imports.iter().take(MAX_IMPORTS) {
        nodes.push(SimNode {
            cx: 0.0,
            cy: 0.0,
            w: imp_w,
            h: 44.0,
            vx: 0.0,
            vy: 0.0,
        });
    }

    // Ring around the hub in file order; dependencies on an outer shell.
    let area: f64 = nodes.iter().map(|n| (n.w + 120.0) * (n.h + 120.0)).sum();
    let base_r = (area / std::f64::consts::PI).sqrt().max(520.0);
    let count = nodes.len().max(1) as f64;
    for (i, node) in nodes.iter_mut().enumerate() {
        let is_dep = i >= imp_start;
        let angle = -std::f64::consts::FRAC_PI_2 + 2.0 * std::f64::consts::PI * i as f64 / count;
        let radius = if is_dep {
            base_r * 1.7 + node.h * 0.2
        } else {
            base_r + node.h * 0.25
        };
        node.cx = angle.cos() * radius;
        node.cy = angle.sin() * radius * 0.85;
    }

    for _ in 0..400 {
        let mut fx = vec![0.0f64; nodes.len()];
        let mut fy = vec![0.0f64; nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let dx = hub.cx - node.cx;
            let dy = hub.cy - node.cy;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let rest = if i >= imp_start {
                base_r * 1.55 + node.w * 0.5
            } else {
                240.0 + (node.w + node.h) * 0.45
            };
            let spring = (dist - rest) * 0.01;
            fx[i] += dx / dist * spring + dx * 0.0025;
            fy[i] += dy / dist * spring + dy * 0.0025;
        }
        for i in 0..imp_start.saturating_sub(1) {
            let (a, b) = (&nodes[i], &nodes[i + 1]);
            let dx = b.cx - a.cx;
            let dy = b.cy - a.cy;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let rest = (a.w + b.w) * 0.5 + 120.0;
            let spring = (dist - rest) * 0.003;
            fx[i] += dx / dist * spring;
            fy[i] += dy / dist * spring;
            fx[i + 1] -= dx / dist * spring;
            fy[i + 1] -= dy / dist * spring;
        }
        // Rectangle-aware repulsion.
        let gap_margin = 100.0;
        for (i, node) in nodes.iter().enumerate() {
            let dx = hub.cx - node.cx;
            let dy = hub.cy - node.cy;
            let gap_x = dx.abs() - (hub.w + node.w) * 0.5;
            let gap_y = dy.abs() - (hub.h + node.h) * 0.5;
            if gap_x < gap_margin && gap_y < gap_margin {
                if gap_x < gap_y {
                    let push = (gap_margin - gap_x).max(1.0) * 0.12;
                    let dir = if dx >= 0.0 { 1.0 } else { -1.0 };
                    fx[i] -= dir * push;
                } else {
                    let push = (gap_margin - gap_y).max(1.0) * 0.12;
                    let dir = if dy >= 0.0 { 1.0 } else { -1.0 };
                    fy[i] -= dir * push;
                }
            }
        }
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (a, b) = (&nodes[i], &nodes[j]);
                let dx = b.cx - a.cx;
                let dy = b.cy - a.cy;
                let gap_x = dx.abs() - (a.w + b.w) * 0.5;
                let gap_y = dy.abs() - (a.h + b.h) * 0.5;
                if gap_x < gap_margin && gap_y < gap_margin {
                    if gap_x < gap_y {
                        let push = (gap_margin - gap_x).max(1.0) * 0.12;
                        let dir = if dx != 0.0 { dx.signum() } else { 1.0 };
                        fx[i] -= dir * push;
                        fx[j] += dir * push;
                    } else {
                        let push = (gap_margin - gap_y).max(1.0) * 0.12;
                        let dir = if dy != 0.0 { dy.signum() } else { 1.0 };
                        fy[i] -= dir * push;
                        fy[j] += dir * push;
                    }
                }
            }
        }
        for (i, node) in nodes.iter_mut().enumerate() {
            node.vx = (node.vx + fx[i]) * 0.8;
            node.vy = (node.vy + fy[i]) * 0.8;
            node.cx += node.vx;
            node.cy += node.vy;
        }
    }

    // Guaranteed separation: panels never overlap, dependencies stay outside
    // every frame.
    let gap = 40.0;
    for _ in 0..200 {
        let mut moved = false;
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (dx, dy) = (nodes[j].cx - nodes[i].cx, nodes[j].cy - nodes[i].cy);
                let over_x = (nodes[i].w + nodes[j].w) / 2.0 + gap - dx.abs();
                let over_y = (nodes[i].h + nodes[j].h) / 2.0 + gap - dy.abs();
                if over_x > 0.0 && over_y > 0.0 {
                    if over_x < over_y {
                        let dir = if dx != 0.0 { dx.signum() } else { 1.0 };
                        nodes[i].cx -= dir * over_x / 2.0;
                        nodes[j].cx += dir * over_x / 2.0;
                    } else {
                        let dir = if dy != 0.0 { dy.signum() } else { 1.0 };
                        nodes[i].cy -= dir * over_y / 2.0;
                        nodes[j].cy += dir * over_y / 2.0;
                    }
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }

    // Primary frame wraps the symbol group.
    let mut sym_boxes: Vec<SymbolBox> = vec![];
    for (i, (start, shown, name, kind, group)) in sym_meta.iter().enumerate() {
        let node = &nodes[i];
        sym_boxes.push(SymbolBox {
            name: name.clone(),
            kind: kind.clone(),
            group: group.clone(),
            start_line: *start,
            end_line: start + shown - 1,
            x: node.cx - node.w / 2.0,
            y: node.cy - node.h / 2.0,
            w: node.w,
            h: node.h,
            code_x: node.cx - node.w / 2.0 + 52.0,
            code_top: node.cy - node.h / 2.0 + HEADER_H + 7.0,
            lines_shown: *shown,
        });
    }
    let mut imp_boxes: Vec<ImportBox> = vec![];
    for (k, imp) in imports.iter().take(MAX_IMPORTS).enumerate() {
        let node = &nodes[imp_start + k];
        imp_boxes.push(ImportBox {
            module: imp["module"].as_str().unwrap_or("").to_string(),
            line: imp["line"].as_u64().unwrap_or(0) as usize,
            x: node.cx - node.w / 2.0,
            y: node.cy - node.h / 2.0,
            w: node.w,
            h: node.h,
        });
    }

    let mut frames: Vec<FrameBox> = vec![];
    if !sym_boxes.is_empty() {
        let (mut sx0, mut sy0, mut sx1, mut sy1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for s in &sym_boxes {
            sx0 = sx0.min(s.x);
            sy0 = sy0.min(s.y);
            sx1 = sx1.max(s.x + s.w);
            sy1 = sy1.max(s.y + s.h);
        }
        frames.push(FrameBox {
            file_name: primary.file_name.clone(),
            primary: true,
            x: sx0 - 80.0,
            y: sy0 - 120.0,
            w: (sx1 + 80.0) - (sx0 - 80.0),
            h: (sy1 + 80.0) - (sy0 - 120.0),
        });
    } else {
        frames.push(FrameBox {
            file_name: primary.file_name.clone(),
            primary: true,
            x: -300.0,
            y: -150.0,
            w: 600.0,
            h: 300.0,
        });
    }
    let primary_frame = frames[0].clone();

    // Neighbor module frames below the primary frame.
    let mut row_x = primary_frame.x;
    let mut row_y = primary_frame.y + primary_frame.h + 320.0;
    let mut row_max_h = 0.0f64;
    for doc in docs.iter().skip(1) {
        let (mut frame, mut boxes) = group_block(doc, row_x, row_y);
        if row_x > primary_frame.x && row_x + frame.w > primary_frame.x + primary_frame.w + 2600.0 {
            row_x = primary_frame.x;
            row_y += row_max_h + 220.0;
            row_max_h = 0.0;
            let placed = group_block(doc, row_x, row_y);
            frame = placed.0;
            boxes = placed.1;
        }
        row_x += frame.w + 180.0;
        row_max_h = row_max_h.max(frame.h);
        for b in boxes {
            sym_boxes.push(b);
        }
        frames.push(frame);
    }

    // Dependencies stay outside every frame.
    let gap = 40.0;
    for _ in 0..200 {
        let mut moved = false;
        for node in nodes.iter_mut().skip(imp_start) {
            for frame in &frames {
                let (dx, dy) = (
                    node.cx - (frame.x + frame.w / 2.0),
                    node.cy - (frame.y + frame.h / 2.0),
                );
                let over_x = (frame.w + node.w) / 2.0 + gap - dx.abs();
                let over_y = (frame.h + node.h) / 2.0 + gap - dy.abs();
                if over_x > 0.0 && over_y > 0.0 {
                    if over_x < over_y {
                        node.cx += if dx >= 0.0 { over_x } else { -over_x };
                    } else {
                        node.cy += if dy >= 0.0 { over_y } else { -over_y };
                    }
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    for (k, imp) in imp_boxes.iter_mut().enumerate() {
        let node = &nodes[imp_start + k];
        imp.x = node.cx - node.w / 2.0;
        imp.y = node.cy - node.h / 2.0;
    }

    // Normalize everything into positive coordinates.
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut grow = |x: f64, y: f64, w: f64, h: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    };
    for s in &sym_boxes {
        grow(s.x, s.y, s.w, s.h);
    }
    for i in &imp_boxes {
        grow(i.x, i.y, i.w, i.h);
    }
    for f in &frames {
        grow(f.x, f.y, f.w, f.h);
    }
    let margin = 160.0;
    let shift_x = margin - min_x;
    let shift_y = margin - min_y;
    for s in &mut sym_boxes {
        s.x += shift_x;
        s.y += shift_y;
        s.code_x += shift_x;
        s.code_top += shift_y;
    }
    for i in &mut imp_boxes {
        i.x += shift_x;
        i.y += shift_y;
    }
    for f in &mut frames {
        f.x += shift_x;
        f.y += shift_y;
    }

    Layout {
        width: (max_x + shift_x + margin).max(1500.0),
        height: (max_y + shift_y + margin).max(800.0),
        frames,
        imports: imp_boxes,
        symbols: sym_boxes,
    }
}

fn body_lines(sym: &serde_json::Value, total_lines: usize, cap: usize) -> (usize, usize) {
    let start = sym["line"].as_u64().unwrap_or(1).max(1) as usize;
    let end = sym["end_line"]
        .as_u64()
        .unwrap_or(start as u64)
        .max(start as u64) as usize;
    let end = end.min(start + cap - 1);
    let shown = (end - start + 1)
        .min(total_lines.saturating_sub(start - 1))
        .max(1);
    (start, shown)
}

/// Compact frame for a neighbor module: its symbols with short bodies.
fn group_block(doc: &DocData, x: f64, y: f64) -> (FrameBox, Vec<SymbolBox>) {
    let width = 640.0;
    let gap = 46.0;
    let total = doc.source.len().max(1);
    let mut boxes = vec![];
    let mut cy = y + 110.0;
    let mut symbols = doc.map["symbols"].as_array().cloned().unwrap_or_default();
    symbols.truncate(12);
    for sym in &symbols {
        let (start, shown) = body_lines(sym, total, SECONDARY_BODY);
        let h = HEADER_H + 14.0 + shown as f64 * LINE_H;
        boxes.push(SymbolBox {
            name: sym["name"].as_str().unwrap_or("").to_string(),
            kind: sym["kind"].as_str().unwrap_or("symbol").to_string(),
            group: doc.file_name.clone(),
            start_line: start,
            end_line: start + shown - 1,
            x: x + 70.0,
            y: cy,
            w: width,
            h,
            code_x: x + 70.0 + 52.0,
            code_top: cy + HEADER_H + 7.0,
            lines_shown: shown,
        });
        cy += h + gap;
    }
    let content_h = cy - gap - (y + 110.0);
    (
        FrameBox {
            file_name: doc.file_name.clone(),
            primary: false,
            x,
            y,
            w: width + 140.0,
            h: (content_h + 190.0).max(260.0),
        },
        boxes,
    )
}

/// Small raster thumbnail of the same layout.
fn render_thumbnail(layout: &Layout) -> Vec<u8> {
    let fit = (THUMB_W as f64 / layout.width)
        .min(THUMB_H as f64 / layout.height)
        .min(1.0);
    let font = load_font();
    let mut img = RgbImage::from_pixel(THUMB_W * SCALE, THUMB_H * SCALE, Rgb([15, 17, 22]));
    let primary = layout
        .frames
        .iter()
        .find(|f| f.primary)
        .cloned()
        .unwrap_or_else(|| FrameBox {
            file_name: String::new(),
            primary: true,
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        });
    for frame in &layout.frames {
        let (fx, fy, fw, fh) = (frame.x * fit, frame.y * fit, frame.w * fit, frame.h * fit);
        let edge = if frame.primary {
            [198, 252, 93]
        } else {
            [74, 84, 100]
        };
        for (ax, ay, aw, ah) in [
            (fx, fy, fw, 2.0),
            (fx, fy + fh - 2.0, fw, 2.0),
            (fx, fy, 2.0, fh),
            (fx + fw - 2.0, fy, 2.0, fh),
        ] {
            fill(&mut img, ax, ay, aw, ah, edge);
        }
        let label_w = (frame.file_name.chars().count() as f64 * 8.0 + 28.0).min(fw);
        fill(&mut img, fx, fy, label_w, 34.0, edge);
        draw_text(
            &mut img,
            font.as_deref(),
            fx + 12.0,
            fy + 11.0,
            13.0,
            &frame.file_name,
            if frame.primary {
                [20, 26, 12]
            } else {
                [228, 230, 235]
            },
        );
    }
    for sym in &layout.symbols {
        let (x, y, w, h) = (sym.x * fit, sym.y * fit, sym.w * fit, sym.h * fit);
        fill(&mut img, x, y, w, h, [28, 32, 40]);
        fill(&mut img, x, y, w, HEADER_H * fit, [45, 51, 62]);
        if fit > 0.25 {
            draw_text(
                &mut img,
                font.as_deref(),
                x + 12.0,
                y + 6.0 * fit + 4.0,
                12.0,
                &format!("{} {}", sym.kind, sym.name),
                [198, 252, 93],
            );
        }
    }
    for dep in &layout.imports {
        let (x, y, w, h) = (dep.x * fit, dep.y * fit, dep.w * fit, dep.h * fit);
        let (tx, ty) = closest_on_rect(
            primary.x * fit,
            primary.y * fit,
            primary.w * fit,
            primary.h * fit,
            x + w / 2.0,
            y + h / 2.0,
        );
        draw_line(&mut img, tx, ty, x + w / 2.0, y + h / 2.0, [74, 84, 100]);
        fill(&mut img, x, y, w, h, [35, 40, 49]);
        if fit > 0.3 {
            draw_text(
                &mut img,
                font.as_deref(),
                x + 12.0,
                y + 14.0,
                12.0,
                &dep.module,
                [228, 230, 235],
            );
        }
    }
    encode(&img)
}

fn closest_on_rect(x: f64, y: f64, w: f64, h: f64, px: f64, py: f64) -> (f64, f64) {
    (px.clamp(x, x + w), py.clamp(y, y + h))
}

fn load_font() -> Option<Vec<u8>> {
    [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ]
    .iter()
    .find_map(|path| std::fs::read(path).ok())
}

fn fill(img: &mut RgbImage, x: f64, y: f64, w: f64, h: f64, color: [u8; 3]) {
    let x0 = (x * SCALE as f64).max(0.0) as u32;
    let y0 = (y * SCALE as f64).max(0.0) as u32;
    let x1 = ((x + w) * SCALE as f64).ceil().min(img.width() as f64) as u32;
    let y1 = ((y + h) * SCALE as f64).ceil().min(img.height() as f64) as u32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            img.put_pixel(xx, yy, Rgb(color));
        }
    }
}

fn draw_line(img: &mut RgbImage, x0: f64, y0: f64, x1: f64, y1: f64, color: [u8; 3]) {
    let steps = ((x1 - x0).abs().max((y1 - y0).abs())).max(1.0) as u32;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let px = ((x0 + (x1 - x0) * t) * SCALE as f64).round() as u32;
        let py = ((y0 + (y1 - y0) * t) * SCALE as f64).round() as u32;
        for dx in 0..SCALE {
            for dy in 0..SCALE {
                let (xx, yy) = (px + dx, py + dy);
                if xx < img.width() && yy < img.height() {
                    img.put_pixel(xx, yy, Rgb(color));
                }
            }
        }
    }
}

fn draw_text(
    img: &mut RgbImage,
    font: Option<&[u8]>,
    x: f64,
    y: f64,
    size: f32,
    text: &str,
    color: [u8; 3],
) {
    let Some(data) = font else { return };
    let Ok(font) = FontRef::try_from_slice(data) else {
        return;
    };
    let scale = PxScale::from(size * SCALE as f32);
    let scaled = font.as_scaled(scale);
    let mut cursor = (x * SCALE as f64) as f32;
    let base_y = (y * SCALE as f64) as f32;
    for ch in text.chars().take(80) {
        let glyph = font.glyph_id(ch).with_scale(scale);
        if let Some(outlined) = scaled.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|dx, dy, c| {
                let px = (cursor + bounds.min.x + dx as f32) as i32;
                let py = (base_y + bounds.min.y + dy as f32) as i32;
                if px >= 0
                    && py >= 0
                    && (px as u32) < img.width()
                    && (py as u32) < img.height()
                    && c > 0.4
                {
                    img.put_pixel(px as u32, py as u32, Rgb(color));
                }
            });
        }
        cursor += scaled.h_advance(font.glyph_id(ch));
    }
}

fn encode(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = image::DynamicImage::ImageRgb8(img.clone())
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(map: serde_json::Value, name: &str) -> DocData {
        DocData {
            file_name: name.to_string(),
            map,
            source: (1..=80).map(|i| format!("line {i}")).collect(),
        }
    }

    fn sample_docs() -> Vec<DocData> {
        vec![
            doc(
                serde_json::json!({
                    "imports": [{"module": "tokio::io", "line": 5}],
                    "symbols": [
                        {"name": "a", "kind": "function", "line": 1, "end_line": 40},
                        {"name": "b", "kind": "struct", "line": 42, "end_line": 80},
                    ],
                    "identifiers": [],
                }),
                "sample.rs",
            ),
            doc(
                serde_json::json!({
                    "imports": [],
                    "symbols": [{"name": "helper", "kind": "function", "line": 1, "end_line": 30}],
                    "identifiers": [],
                }),
                "helper.rs",
            ),
        ]
    }

    #[test]
    fn plan_keeps_every_symbol_with_full_body() {
        let layout = plan(&sample_docs());
        assert_eq!(layout.symbols.len(), 3);
        assert_eq!(layout.symbols[0].lines_shown, 40);
        assert_eq!(layout.symbols[1].lines_shown, 39);
        // Secondary modules keep a compact body.
        assert_eq!(layout.symbols[2].lines_shown, SECONDARY_BODY);
        // Force layout must not stack boxes into a single column.
        let centers: Vec<f64> = layout.symbols.iter().map(|s| s.x + s.w / 2.0).collect();
        let spread = centers[0].max(centers[1]) - centers[0].min(centers[1]);
        assert!(spread > 100.0, "boxes collapsed into a column: {centers:?}");
    }

    #[test]
    fn plan_never_overlaps_boxes() {
        let layout = plan(&sample_docs());
        let mut rects: Vec<(f64, f64, f64, f64)> = vec![];
        for s in &layout.symbols {
            rects.push((s.x, s.y, s.w, s.h));
        }
        for m in &layout.imports {
            rects.push((m.x, m.y, m.w, m.h));
        }
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let (ax, ay, aw, ah) = rects[i];
                let (bx, by, bw, bh) = rects[j];
                let ox = (aw + bw) / 2.0 - ((ax + aw / 2.0) - (bx + bw / 2.0)).abs();
                let oy = (ah + bh) / 2.0 - ((ay + ah / 2.0) - (by + bh / 2.0)).abs();
                assert!(
                    ox <= 20.0 || oy <= 20.0,
                    "boxes {i} and {j} overlap: ox={ox:.1} oy={oy:.1}"
                );
            }
        }
        // Dependencies never intrude into any frame.
        for frame in &layout.frames {
            for m in &layout.imports {
                let ox =
                    (frame.w + m.w) / 2.0 - ((frame.x + frame.w / 2.0) - (m.x + m.w / 2.0)).abs();
                let oy =
                    (frame.h + m.h) / 2.0 - ((frame.y + frame.h / 2.0) - (m.y + m.h / 2.0)).abs();
                assert!(
                    ox <= 20.0 || oy <= 20.0,
                    "dependency {:?} intrudes into frame {:?}",
                    m.module,
                    frame.file_name
                );
            }
        }
    }

    #[test]
    fn call_edges_link_symbols() {
        let map = serde_json::json!({
            "imports": [],
            "symbols": [
                {"name": "caller", "kind": "function", "line": 1, "end_line": 10},
                {"name": "callee", "kind": "function", "line": 12, "end_line": 20},
            ],
            "identifiers": [
                {"text": "callee", "line": 5, "column": 4},
            ],
        });
        let edges = call_edges(&map);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "caller");
        assert_eq!(edges[0]["to"], "callee");
    }

    #[tokio::test]
    #[ignore = "writes a preview PNG for visual checks"]
    async fn preview_thumbnail() {
        let worker = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../workers/web-capture/index.js");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../retake-server/src/review_service.rs");
        let renderer = CodeRenderer::new(Some(worker.to_string_lossy().into_owned()));
        let target = Target {
            kind: TargetKind::Code,
            path: Some(source.to_string_lossy().into_owned()),
            url: None,
            label: None,
            viewport: None,
            metadata: None,
        };
        let draft = renderer.capture(&target).await.unwrap();
        let out = std::env::temp_dir().join("retake-code-preview.png");
        std::fs::write(&out, &draft.asset_bytes).unwrap();
        eprintln!(
            "scene {}x{} thumb {} bytes; groups={:?} symbols={} calls unavailable in draft",
            draft.width,
            draft.height,
            draft.asset_bytes.len(),
            draft.mapping["frames"].as_array().map(|f| f
                .iter()
                .filter_map(|x| x["file_name"].as_str())
                .collect::<Vec<_>>()),
            draft.mapping["symbols"]
                .as_array()
                .map(|s| s.len())
                .unwrap_or(0),
        );
    }
}
