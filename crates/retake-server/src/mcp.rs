use crate::review_service::ReviewService;
use retake_core::{RuntimePaths, Target, TargetKind, Viewport};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

const TARGET_TYPES: &str =
    "image, text, markdown, web, html, adb, code, pdf, pencil, video, macos_window";

fn target_schema() -> Value {
    json!({
        "type": "object",
        "description": "A review target. Choose the type from the artifact format; do not substitute a different file. Use text for UTF-8 plain text, including .diff and .patch files. Code is limited to .rs, .ts, .tsx, .js, .jsx, .mjs, .mts, and .cts files.",
        "properties": {
            "type": {
                "type": "string",
                "enum": ["image", "text", "markdown", "web", "html", "adb", "code", "pdf", "pencil", "video", "macos_window"],
                "description": "Renderer to use. Use text for .txt, .diff, .patch, logs, and other UTF-8 plain text; markdown for .md/.markdown; image for PNG/JPEG/WebP; code only for supported Rust/TypeScript/JavaScript extensions."
            },
            "path": {
                "type": "string",
                "description": "Absolute local path. Required except for web. For adb, use the device serial; for macos_window, use an app name or absolute .app path."
            },
            "url": {
                "type": "string",
                "description": "HTTP(S) URL. Required for web targets."
            },
            "label": {
                "type": "string",
                "description": "Optional display label."
            },
            "viewport": {
                "type": "object",
                "description": "Optional capture viewport for web, html, or markdown (default 1280x800).",
                "properties": {
                    "width": {"type": "integer", "minimum": 1},
                    "height": {"type": "integer", "minimum": 1}
                },
                "required": ["width", "height"],
                "additionalProperties": false
            },
            "metadata": {
                "type": "object",
                "description": "Optional renderer-specific metadata, such as Pencil node data, video time range, or macOS window_title."
            }
        },
        "required": ["type"],
        "additionalProperties": false
    })
}

pub async fn run_mcp() -> anyhow::Result<()> {
    configure_stdio()?;
    let store = Arc::new(retake_store::ReviewStore::new());
    let runtime = RuntimePaths::discover();
    let worker = runtime
        .web_worker()
        .or_else(|| find_runtime_path("workers/web-capture/index.js", Path::is_file))
        .map(|p| p.to_string_lossy().to_string());
    let service = Arc::new(ReviewService::new(store.clone(), worker));

    // start http server on random port
    let ui_dir = std::env::var("RETAKE_UI_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| {
            runtime
                .root()
                .map(|root| root.join("ui"))
                .filter(|path| path.join("dist/index.html").is_file())
        })
        .or_else(|| find_runtime_path("ui", |p| p.join("dist/index.html").is_file()))
        .unwrap_or_else(|| PathBuf::from("ui"));
    let http_server = crate::http_server::start_http(service.clone(), 0, ui_dir).await?;
    let base_url = format!("http://127.0.0.1:{}", http_server.port());

    let result = tokio::select! {
        result = serve_mcp_requests(service.clone(), &base_url) => result,
        signal = shutdown_signal() => {
            signal
                .map(|name| info!("received {}; shutting down", name))
                .map_err(anyhow::Error::from)
        }
    };

    // Stop accepting HTTP work before closing browser children. This runs for
    // stdin EOF, broken stdio, SIGINT and SIGTERM alike.
    http_server.shutdown().await;
    service.close_all_browsers().await;
    result
}

// OpenCode V2 connects local MCP servers with non-blocking Unix sockets.
// std::io's line reader treats EAGAIN as a fatal read error, so restore the
// blocking semantics expected by the MCP stdio loop. Each socket endpoint has
// its own file status flags; this does not change the client's endpoint.
#[cfg(unix)]
fn configure_stdio() -> io::Result<()> {
    set_fd_blocking(libc::STDIN_FILENO)?;
    set_fd_blocking(libc::STDOUT_FILENO)
}

#[cfg(unix)]
fn set_fd_blocking(fd: libc::c_int) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::O_NONBLOCK != 0
        && unsafe { libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) } == -1
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn configure_stdio() -> io::Result<()> {
    Ok(())
}

async fn serve_mcp_requests(service: Arc<ReviewService>, base_url: &str) -> anyhow::Result<()> {
    let mut lines = stdin_lines();

    let mut stdout = io::stdout();
    while let Some(line) = lines.recv().await {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = req.get("id").cloned();
        let method = req["method"].as_str().unwrap_or("").to_string();
        let params = req.get("params").cloned().unwrap_or(json!({}));

        let resp = match method.as_str() {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "retake", "version": "0.1.0" }
            }),
            "tools/list" => json!({
                "tools": [
                    {
                        "name": "open_review",
                        "description": "Open a review session. Supported targets: image (PNG/JPEG/WebP), text (UTF-8, including .diff/.patch), markdown, web, html, adb, code (.rs/.ts/.tsx/.js/.jsx/.mjs/.mts/.cts only), pdf, pencil, video, and macos_window. In the same assistant turn, show the returned URL and immediately call wait_review; never stop after open_review.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string"},
                                "targets": {
                                    "type": "array",
                                    "description": "Artifacts to review. For a git diff, write it to a .diff or .patch file and use type text.",
                                    "items": target_schema(),
                                    "minItems": 1
                                },
                                "open_browser": {"type": "boolean", "description": "Open an owned browser window. It stays open after submit for revisions/diffs and closes on cancel or explicit close (default true)"}
                            },
                            "required": ["targets"]
                        }
                    },
                    {
                        "name": "wait_review",
                        "description": "Wait for review submission, cancellation, chat handoff (status chat), or timeout",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "review_id": {"type": "string"},
                                "timeout_ms": {"type": "integer"}
                            },
                            "required": ["review_id"]
                        }
                    },
                    {
                        "name": "update_review",
                        "description": "Capture revised targets after acting on feedback. Appends an immutable revision to the submitted review, asks the owned review browser to come to the foreground, and shows the before/after diff. Pass the same targets in the same order as open_review. In the same assistant turn, immediately call wait_review with timeout_ms: 600000 and repeat while pending; never stop after update_review.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "review_id": {"type": "string"},
                                "targets": {
                                    "type": "array",
                                    "description": "The same target definitions, in the same order, as open_review.",
                                    "items": target_schema(),
                                    "minItems": 1
                                }
                            },
                            "required": ["review_id", "targets"]
                        }
                    },
                    {
                        "name": "review_message",
                        "description": "Post a progress update or a question to the open review window. Progress must be one line, at most 60 characters, and must not end in a period. For questions, use wait_reply to receive the user's response.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "review_id": {"type": "string"},
                                "kind": {"type": "string", "enum": ["progress", "question"]},
                                "text": {"type": "string"}
                            },
                            "required": ["review_id", "kind", "text"]
                        }
                    },
                    {
                        "name": "wait_reply",
                        "description": "Wait for a question reply, or status chat if the user chooses to answer in the original chat",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "review_id": {"type": "string"},
                                "message_id": {"type": "string"},
                                "timeout_ms": {"type": "integer"}
                            },
                            "required": ["review_id", "message_id"]
                        }
                    }
                ]
            }),
            "tools/call" => {
                let tool = params["name"].as_str().unwrap_or("");
                let args = &params["arguments"];
                match tool {
                    "open_review" => {
                        match handle_open_review(service.clone(), args, base_url).await {
                            Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                            Err(e) => {
                                json!({"isError": true, "content": [{"type":"text", "text": e.to_string()}] })
                            }
                        }
                    }
                    "wait_review" => match handle_wait_review(service.clone(), args).await {
                        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                        Err(e) => {
                            json!({"isError": true, "content": [{"type":"text", "text": e.to_string()}] })
                        }
                    },
                    "update_review" => match handle_update_review(service.clone(), args).await {
                        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                        Err(e) => json!({"isError": true, "content": [{"type":"text", "text": e}]}),
                    },
                    "review_message" => match handle_review_message(service.clone(), args) {
                        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                        Err(e) => json!({"isError": true, "content": [{"type":"text", "text": e}]}),
                    },
                    "wait_reply" => match handle_wait_reply(service.clone(), args).await {
                        Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                        Err(e) => json!({"isError": true, "content": [{"type":"text", "text": e}]}),
                    },
                    _ => {
                        json!({"isError": true, "content": [{"type":"text","text":"unknown tool"}]})
                    }
                }
            }
            _ => json!({}),
        };

        let response = if let Some(i) = id {
            json!({ "jsonrpc": "2.0", "id": i, "result": resp })
        } else {
            json!({ "jsonrpc": "2.0", "result": resp })
        };
        let out = serde_json::to_string(&response)? + "\n";
        stdout.write_all(out.as_bytes())?;
        stdout.flush()?;
    }
    Ok(())
}

fn stdin_lines() -> mpsc::UnboundedReceiver<io::Result<String>> {
    let (sender, receiver) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        forward_lines(stdin.lock(), sender);
    });
    receiver
}

fn forward_lines(reader: impl BufRead, sender: mpsc::UnboundedSender<io::Result<String>>) {
    for line in reader.lines() {
        let failed = line.is_err();
        if sender.send(line).is_err() || failed {
            break;
        }
    }
}

#[cfg(unix)]
async fn shutdown_signal() -> io::Result<&'static str> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            Ok("SIGINT")
        }
        _ = terminate.recv() => Ok("SIGTERM"),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> io::Result<&'static str> {
    tokio::signal::ctrl_c().await?;
    Ok("interrupt")
}

/// Locate assets next to an installed binary (`bin/retake` plus sibling asset
/// directories) or above a source build (`target/{debug,release}/retake`).
fn find_runtime_path(relative: &str, is_match: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable = executable.canonicalize().unwrap_or(executable);
    find_runtime_path_from(&executable, relative, is_match)
}

fn find_runtime_path_from(
    executable: &Path,
    relative: &str,
    is_match: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    executable
        .parent()?
        .ancestors()
        .take(4)
        .map(|directory| directory.join(relative))
        .find(|candidate| is_match(candidate))
}

async fn handle_open_review(
    service: Arc<ReviewService>,
    args: &Value,
    base_url: &str,
) -> Result<Value, String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let targets = parse_targets(args)?;

    let (review_id, token) = service
        .open_review(title, targets)
        .await
        .map_err(|e| e.to_string())?;
    let url = format!("{}/review/{}#token={}", base_url, review_id, token);
    let open_browser = args
        .get("open_browser")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let browser_opened =
        if open_browser && std::env::var("RETAKE_OPEN_BROWSER").as_deref() != Ok("0") {
            match service.open_browser(&review_id, &url).await {
                Ok(()) => true,
                Err(err) => {
                    // The review remains usable through the URL when no GUI is available.
                    tracing::warn!("could not open dedicated browser: {}", err);
                    false
                }
            }
        } else {
            false
        };
    Ok(json!({
        "review_id": review_id,
        "review_url": url,
        "browser_opened": browser_opened,
        "next_action": "Immediately call wait_review for this review_id in the same assistant turn (timeout_ms: 600000); repeat while pending."
    }))
}

fn parse_targets(args: &Value) -> Result<Vec<Target>, String> {
    let targets_in = args
        .get("targets")
        .and_then(|v| v.as_array())
        .ok_or("targets required")?;
    let mut targets = vec![];
    for t in targets_in {
        let kind_str = t
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or("type required")?;
        let kind = match kind_str {
            "image" => TargetKind::Image,
            "text" => TargetKind::Text,
            "markdown" => TargetKind::Markdown,
            "html" => TargetKind::Html,
            "web" => TargetKind::Web,
            "adb" => TargetKind::Adb,
            "code" => TargetKind::Code,
            "pdf" => TargetKind::Pdf,
            "pencil" => TargetKind::Pencil,
            "video" => TargetKind::Video,
            "macos_window" | "macos-window" => TargetKind::MacosWindow,
            _ => {
                return Err(format!(
                    "unknown target type '{kind_str}'; supported types: {TARGET_TYPES}"
                ))
            }
        };
        let path = t
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let url = t.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());
        let label = t
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let viewport = t.get("viewport").map(|v| Viewport {
            width: v.get("width").and_then(|x| x.as_u64()).unwrap_or(1280) as u32,
            height: v.get("height").and_then(|x| x.as_u64()).unwrap_or(800) as u32,
        });
        targets.push(Target {
            kind,
            path,
            url,
            label,
            viewport,
            metadata: t.get("metadata").cloned(),
        });
    }

    Ok(targets)
}

async fn handle_update_review(service: Arc<ReviewService>, args: &Value) -> Result<Value, String> {
    let review_id = args
        .get("review_id")
        .and_then(Value::as_str)
        .ok_or("review_id required")?;
    let targets = parse_targets(args)?;
    let revision = service
        .update_review(review_id, targets)
        .await
        .map_err(|e| e.to_string())?;
    let browser_focus_requested = service.focus_browser(review_id).await;
    Ok(json!({
        "review_id": review_id,
        "revision": revision,
        "browser_focus_requested": browser_focus_requested,
        "next_action": "Immediately call wait_review for this review_id in the same assistant turn (timeout_ms: 600000); repeat while pending. Never finish the turn after update_review."
    }))
}

fn handle_review_message(service: Arc<ReviewService>, args: &Value) -> Result<Value, String> {
    let review_id = args
        .get("review_id")
        .and_then(Value::as_str)
        .ok_or("review_id required")?;
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("kind required")?;
    let text = args
        .get("text")
        .and_then(Value::as_str)
        .ok_or("text required")?;
    let message_id = service
        .add_message(review_id, kind, text)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "review_id": review_id, "message_id": message_id }))
}

async fn handle_wait_reply(service: Arc<ReviewService>, args: &Value) -> Result<Value, String> {
    let review_id = args
        .get("review_id")
        .and_then(Value::as_str)
        .ok_or("review_id required")?;
    let message_id = args
        .get("message_id")
        .and_then(Value::as_str)
        .ok_or("message_id required")?;
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(20_000);
    let reply = service
        .wait_reply(review_id, message_id, timeout_ms)
        .await
        .map_err(|e| e.to_string())?;
    let (status, text) = match reply {
        crate::review_service::ReplyOutcome::Pending => ("pending", None),
        crate::review_service::ReplyOutcome::Chat => ("chat", None),
        crate::review_service::ReplyOutcome::Replied(text) => ("replied", Some(text)),
    };
    Ok(json!({ "review_id": review_id, "message_id": message_id, "status": status, "reply": text }))
}

async fn handle_wait_review(service: Arc<ReviewService>, args: &Value) -> Result<Value, String> {
    let rid = args
        .get("review_id")
        .and_then(|v| v.as_str())
        .ok_or("review_id required")?
        .to_string();
    let to = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(20000);
    let res = service
        .wait_review(&rid, to)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(&res).unwrap())
}

#[cfg(test)]
mod tests {
    use super::{find_runtime_path_from, forward_lines, parse_targets, target_schema};
    use retake_core::TargetKind;
    use std::{fs, io::Cursor};
    use tokio::sync::mpsc;

    #[test]
    fn finds_assets_beside_a_packaged_bin_directory() {
        let root = std::env::temp_dir().join(format!("retake-runtime-{}", uuid::Uuid::new_v4()));
        let executable = root.join("bin/retake");
        let index = root.join("ui/dist/index.html");
        fs::create_dir_all(index.parent().unwrap()).unwrap();
        fs::write(&index, "").unwrap();

        let result = find_runtime_path_from(&executable, "ui", |path| {
            path.join("dist/index.html").is_file()
        });

        assert_eq!(result, Some(root.join("ui")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_macos_window_target_aliases() {
        for kind in ["macos_window", "macos-window"] {
            let targets = parse_targets(&serde_json::json!({
                "targets": [{ "type": kind, "path": "Retake" }]
            }))
            .unwrap();
            assert_eq!(targets[0].kind, TargetKind::MacosWindow);
        }
    }

    #[test]
    fn target_schema_exposes_formats_and_fields() {
        let schema = target_schema();
        let types = schema["properties"]["type"]["enum"]
            .as_array()
            .expect("target type enum");
        assert!(types.iter().any(|kind| kind == "text"));
        assert!(types.iter().any(|kind| kind == "code"));
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["url"].is_object());
        assert!(schema["description"].as_str().unwrap().contains(".diff"));
    }

    #[test]
    fn unknown_target_error_lists_supported_types() {
        let error = parse_targets(&serde_json::json!({
            "targets": [{ "type": "diff", "path": "/tmp/change.diff" }]
        }))
        .unwrap_err();
        assert!(error.contains("unknown target type 'diff'"));
        assert!(error.contains("text"));
    }

    #[tokio::test]
    async fn stdin_forwarder_closes_channel_at_eof() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        forward_lines(Cursor::new("first\nsecond\n"), sender);

        assert_eq!(receiver.recv().await.unwrap().unwrap(), "first");
        assert_eq!(receiver.recv().await.unwrap().unwrap(), "second");
        assert!(receiver.recv().await.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn converts_nonblocking_stdio_socket_to_blocking() {
        use super::set_fd_blocking;
        use std::os::{fd::AsRawFd, unix::net::UnixStream};

        let (socket, _peer) = UnixStream::pair().unwrap();
        socket.set_nonblocking(true).unwrap();
        set_fd_blocking(socket.as_raw_fd()).unwrap();

        let flags = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_GETFL) };
        assert_eq!(flags & libc::O_NONBLOCK, 0);
    }
}
