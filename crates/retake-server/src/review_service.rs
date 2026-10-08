use futures::future::join_all;
use retake_core::{
    Annotation, Generation, RenderError, ReviewResult, RuntimePaths, Selection, SnapshotDraft,
    StoredSnapshot, Target,
};
use retake_renderers::{
    AdbRenderer, CodeRenderer, ImageRenderer, MacosWindowRenderer, PdfRenderer, PencilRenderer,
    TextRenderer, VideoRenderer, WebRenderer,
};
use retake_store::{ReviewFeedback, ReviewMessage, ReviewRevision, ReviewStore, StoreError};
use serde_json::json;
use std::sync::Arc;
use std::{collections::HashMap, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyOutcome {
    Pending,
    Replied(String),
    Chat,
}

fn chat_handoff(review_id: &str) -> ReviewResult {
    ReviewResult {
        status: "chat".into(),
        review_id: review_id.into(),
        annotations: None,
        generation: None,
    }
}

pub struct ReviewService {
    store: Arc<ReviewStore>,
    registry: retake_core::RendererRegistry,
    worker_path: Option<String>,
    review_windows: Mutex<HashMap<String, Child>>,
    capture_timeout: Duration,
}

impl ReviewService {
    pub fn new(store: Arc<ReviewStore>, worker_path: Option<String>) -> Self {
        let mut reg = retake_core::RendererRegistry::new();
        reg.register(Box::new(ImageRenderer::new()));
        reg.register(Box::new(MacosWindowRenderer::new()));
        reg.register(Box::new(TextRenderer::new()));
        reg.register(Box::new(AdbRenderer::new()));
        reg.register(Box::new(CodeRenderer::new(worker_path.clone())));
        reg.register(Box::new(PdfRenderer::new(worker_path.clone())));
        reg.register(Box::new(PencilRenderer::new()));
        reg.register(Box::new(VideoRenderer::new()));
        if let Some(wp) = &worker_path {
            reg.register(Box::new(WebRenderer::new(wp.clone())));
        }
        Self {
            store,
            registry: reg,
            worker_path,
            review_windows: Mutex::new(HashMap::new()),
            capture_timeout: Duration::from_secs(60),
        }
    }

    /// Open an owned review window for this review only. A configured Tauri
    /// desktop host is preferred; the Playwright browser remains the fallback.
    pub async fn open_browser(&self, review_id: &str, url: &str) -> anyhow::Result<()> {
        let runtime = RuntimePaths::discover();
        let mut command = if let Some(desktop) = runtime.desktop_executable() {
            let mut command = Command::new(desktop);
            command.arg("--review-window");
            command
        } else {
            let worker = self
                .worker_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("browser worker not configured"))?;
            let script = Path::new(worker).with_file_name("review-window.js");
            if !script.is_file() {
                anyhow::bail!("review browser worker not found");
            }
            let mut command = Command::new(runtime.node_executable());
            runtime.configure_worker_command(&mut command);
            command.arg(script);
            command
        };
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        // Do not SIGKILL the Node worker when this handle is dropped. Playwright
        // launches Chromium in a separate process group on macOS, so killing
        // only Node leaves an orphaned app in the Dock. Closing stdin lets the
        // worker shut Chromium down itself even if the MCP host disappears.
        command.kill_on_drop(false);
        let mut child = command.spawn()?;
        let request = serde_json::json!({ "url": url }).to_string() + "\n";
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(request.as_bytes())
            .await?;

        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        match timeout(Duration::from_secs(20), lines.next_line()).await {
            Ok(Ok(Some(line))) if line == "ready" => {
                // The worker only writes a single readiness line to stdout.
                if let Some(old) = self
                    .review_windows
                    .lock()
                    .await
                    .insert(review_id.to_owned(), child)
                {
                    Self::shutdown_browser_child(old).await;
                }
                Ok(())
            }
            _ => {
                Self::shutdown_browser_child(child).await;
                anyhow::bail!("could not start dedicated review browser");
            }
        }
    }

    pub async fn close_browser(&self, review_id: &str) {
        let child = self.review_windows.lock().await.remove(review_id);
        if let Some(child) = child {
            Self::shutdown_browser_child(child).await;
        }
    }

    /// Ask the dedicated review browser to bring its page to the foreground.
    /// This is best-effort because desktop window managers may reject focus.
    pub async fn focus_browser(&self, review_id: &str) -> bool {
        let mut windows = self.review_windows.lock().await;
        let Some(child) = windows.get_mut(review_id) else {
            return false;
        };
        let Some(input) = child.stdin.as_mut() else {
            return false;
        };
        input.write_all(b"focus\n").await.is_ok()
    }

    /// Closing the review window pauses the transient update presentation.
    /// Feedback remains submitted, and a later update_review call recreates
    /// the marker before capturing the next immutable revision.
    pub fn pause_update(&self, review_id: &str) -> Result<(), StoreError> {
        self.store.abort_update(review_id)
    }

    pub async fn close_all_browsers(&self) {
        let children = {
            let mut windows = self.review_windows.lock().await;
            windows.drain().map(|(_, child)| child).collect::<Vec<_>>()
        };
        join_all(children.into_iter().map(Self::shutdown_browser_child)).await;
    }

    async fn shutdown_browser_child(mut child: Child) {
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(b"close\n").await;
            let _ = input.shutdown().await;
        }
        if timeout(Duration::from_secs(3), child.wait()).await.is_ok() {
            return;
        }

        // SIGTERM is important here: Playwright handles it by closing the
        // detached Chromium process group. tokio's Child::kill uses SIGKILL on
        // Unix, which bypasses that cleanup and causes the macOS orphan.
        #[cfg(unix)]
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
        #[cfg(not(unix))]
        let _ = child.start_kill();

        if timeout(Duration::from_secs(3), child.wait()).await.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    pub async fn open_review(
        &self,
        title: Option<String>,
        targets: Vec<Target>,
    ) -> Result<(String, String), anyhow::Error> {
        let stored_snaps = self.capture_targets(targets).await?;
        let token = self.store.generate_token();
        let review_id = self
            .store
            .create_review(title, stored_snaps, token.clone())?;
        Ok((review_id, token))
    }

    pub async fn update_review(
        &self,
        review_id: &str,
        targets: Vec<Target>,
    ) -> anyhow::Result<i64> {
        self.store.begin_update(review_id)?;
        let result = async {
            let stored_snaps = self.capture_targets(targets).await?;
            Ok::<_, anyhow::Error>(self.store.append_revision(review_id, stored_snaps)?)
        }
        .await;
        if result.is_err() {
            let _ = self.store.abort_update(review_id);
        }
        result
    }

    async fn capture_targets(
        &self,
        targets: Vec<Target>,
    ) -> anyhow::Result<Vec<(StoredSnapshot, Vec<u8>)>> {
        if targets.is_empty() {
            anyhow::bail!("at least one target is required");
        }

        // (target, zoom level, draft). Code is one seamless zoomable drawing
        // (concept graph zooms out, source inside its boxes zooms in), so it
        // captures as a single snapshot like every other target.
        let mut captured: Vec<(Target, u32, SnapshotDraft)> = vec![];
        for (group, t) in targets.iter().enumerate() {
            let limit = if t.kind == retake_core::TargetKind::Video {
                Duration::from_secs(240)
            } else if matches!(
                t.kind,
                retake_core::TargetKind::Code | retake_core::TargetKind::Pdf
            ) {
                Duration::from_secs(120)
            } else {
                self.capture_timeout
            };
            let levels: Vec<(Target, u32)> = vec![(t.clone(), 0)];
            for (sub, level) in levels {
                let renderer = self
                    .registry
                    .find(&sub)
                    .ok_or_else(|| anyhow::anyhow!("no renderer for target"))?;
                let mut draft = timeout(limit, renderer.capture(&sub))
                    .await
                    .map_err(|_| anyhow::anyhow!("capture timeout"))?
                    .map_err(|e| anyhow::anyhow!("capture error: {}", e))?;
                if let Some(mapping) = draft.mapping.as_object_mut() {
                    mapping.insert("zoom_group".into(), json!(group));
                    mapping.insert("zoom_level".into(), json!(level));
                }
                captured.push((sub, level, draft));
            }
        }

        let mut stored_snaps = vec![];
        for (i, (target, _level, draft)) in captured.into_iter().enumerate() {
            let snap_id = uuid::Uuid::new_v4().to_string();
            let stored = StoredSnapshot {
                id: snap_id,
                review_id: "".to_string(), // filled in store
                position: i as i32,
                renderer_id: self.registry.find(&target).unwrap().id().to_string(),
                mapping_version: 1,
                width: draft.width,
                height: draft.height,
                mime_type: draft.mime_type,
                asset_path: "".to_string(),
                source_label: draft.source_label,
                mapping_json: draft.mapping.to_string(),
            };
            stored_snaps.push((stored, draft.asset_bytes));
        }

        Ok(stored_snaps)
    }

    pub async fn wait_review(
        &self,
        review_id: &str,
        timeout_ms: u64,
    ) -> Result<ReviewResult, StoreError> {
        // Subscribe before checking state so a submission between the check and
        // the wait cannot be missed.
        let mut updates = self.store.subscribe(review_id)?;
        let initial = self.store.get_review_state(review_id)?;
        if initial.status != "pending" {
            return Ok(initial);
        }
        if self.store.has_chat_handoff(review_id)? {
            return Ok(chat_handoff(review_id));
        }

        let duration = Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
        match timeout(duration, async {
            loop {
                updates
                    .changed()
                    .await
                    .map_err(|_| StoreError::Other("review notifications closed".into()))?;
                let result = self.store.get_review_state(review_id)?;
                if result.status != "pending" {
                    return Ok(result);
                }
                if self.store.has_chat_handoff(review_id)? {
                    return Ok(chat_handoff(review_id));
                }
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                let result = self.store.get_review_state(review_id)?;
                if result.status == "pending" && self.store.has_chat_handoff(review_id)? {
                    Ok(chat_handoff(review_id))
                } else {
                    Ok(result)
                }
            }
        }
    }

    pub async fn wait_reply(
        &self,
        review_id: &str,
        message_id: &str,
        timeout_ms: u64,
    ) -> Result<ReplyOutcome, StoreError> {
        let mut updates = self.store.subscribe(review_id)?;
        let current = self.question_outcome(review_id, message_id)?;
        if current != ReplyOutcome::Pending {
            return Ok(current);
        }
        let wait = timeout(
            Duration::from_millis(timeout_ms.clamp(1_000, 600_000)),
            async {
                loop {
                    updates
                        .changed()
                        .await
                        .map_err(|_| StoreError::Other("review notifications closed".into()))?;
                    let current = self.question_outcome(review_id, message_id)?;
                    if current != ReplyOutcome::Pending {
                        return Ok(current);
                    }
                }
            },
        )
        .await;
        match wait {
            Ok(result) => result,
            Err(_) => self.question_outcome(review_id, message_id),
        }
    }

    fn question_outcome(
        &self,
        review_id: &str,
        message_id: &str,
    ) -> Result<ReplyOutcome, StoreError> {
        let (reply, handed_off) = self.store.get_message_response(review_id, message_id)?;
        Ok(if let Some(text) = reply {
            ReplyOutcome::Replied(text)
        } else if handed_off {
            ReplyOutcome::Chat
        } else {
            ReplyOutcome::Pending
        })
    }

    pub fn add_message(
        &self,
        review_id: &str,
        kind: &str,
        text: &str,
    ) -> Result<String, StoreError> {
        self.store.add_message(review_id, kind, text)
    }

    pub fn get_messages(&self, review_id: &str) -> Result<Vec<ReviewMessage>, StoreError> {
        self.store.list_messages(review_id)
    }

    pub fn reply_message(
        &self,
        review_id: &str,
        message_id: &str,
        text: &str,
    ) -> Result<(), StoreError> {
        self.store.reply_message(review_id, message_id, text)
    }

    pub fn handoff_message(&self, review_id: &str, message_id: &str) -> Result<(), StoreError> {
        self.store.handoff_message(review_id, message_id)
    }

    pub async fn resolve_annotations(
        &self,
        review_id: &str,
        raw_annotations: Vec<RawAnnotation>,
    ) -> Result<Vec<Annotation>, RenderError> {
        let snaps = self
            .store
            .list_revisions(review_id)
            .map_err(|_| RenderError::Other("store".into()))?
            .pop()
            .ok_or(RenderError::InvalidSelection)?
            .snapshots;
        let mut out = vec![];
        for ra in raw_annotations {
            let snap = snaps
                .iter()
                .find(|s| s.id == ra.snapshot_id)
                .ok_or(RenderError::InvalidSelection)?;
            let renderer = self
                .registry
                .find_by_id(&snap.renderer_id)
                .ok_or(RenderError::Unsupported)?;
            let resolved = renderer.resolve(snap, &ra.selection)?;
            let ann = Annotation {
                snapshot_id: ra.snapshot_id,
                target_label: snap.source_label.clone(),
                selection: ra.selection,
                comment: ra.comment,
                reference: resolved.description,
            };
            out.push(ann);
        }
        Ok(out)
    }

    pub fn submit(
        &self,
        review_id: &str,
        annotations: Vec<Annotation>,
        generation: Generation,
    ) -> Result<ReviewResult, StoreError> {
        self.store.submit_review(review_id, annotations, generation)
    }

    pub fn cancel(&self, review_id: &str) -> Result<ReviewResult, StoreError> {
        self.store.cancel_review(review_id)
    }

    pub fn get_snapshots(&self, review_id: &str) -> Result<Vec<StoredSnapshot>, StoreError> {
        self.store.list_snapshots(review_id)
    }

    pub fn get_revisions(&self, review_id: &str) -> Result<Vec<ReviewRevision>, StoreError> {
        self.store.list_revisions(review_id)
    }

    pub fn get_feedback(&self, review_id: &str) -> Result<Vec<ReviewFeedback>, StoreError> {
        self.store.list_feedback(review_id)
    }

    pub fn is_updating(&self, review_id: &str) -> Result<bool, StoreError> {
        self.store.is_updating(review_id)
    }

    pub fn get_asset(&self, review_id: &str, snap_id: &str) -> Result<Vec<u8>, StoreError> {
        self.store.get_asset_bytes(review_id, snap_id)
    }

    /// Scene data for dynamic panes (code). The stored mapping minus anything
    /// that reveals the original target path.
    pub fn get_scene(
        &self,
        review_id: &str,
        snap_id: &str,
    ) -> Result<serde_json::Value, StoreError> {
        let snaps = self.store.list_revisions(review_id)?;
        let snap = snaps
            .iter()
            .flat_map(|revision| &revision.snapshots)
            .find(|s| s.id == snap_id)
            .ok_or(StoreError::NotFound)?;
        let mut mapping: serde_json::Value = serde_json::from_str(&snap.mapping_json)
            .map_err(|e| StoreError::Other(e.to_string()))?;
        if let Some(obj) = mapping.as_object_mut() {
            obj.remove("path");
        }
        Ok(mapping)
    }

    pub fn validate_access(&self, review_id: &str, token: &str) -> bool {
        self.store.validate_token(review_id, token).unwrap_or(false)
    }

    pub fn get_state(&self, review_id: &str) -> Result<ReviewResult, StoreError> {
        self.store.get_review_state(review_id)
    }
}

// helper for input
#[derive(Clone, Debug)]
pub struct RawAnnotation {
    pub snapshot_id: String,
    pub selection: Selection,
    pub comment: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_review_returns_when_submitted() {
        let store = Arc::new(ReviewStore::new());
        let id = store
            .create_review(None, vec![], store.generate_token())
            .unwrap();
        let service = ReviewService::new(store.clone(), None);
        let submitted = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            store
                .submit_review(
                    &id,
                    vec![],
                    Generation {
                        variants_enabled: false,
                        count: 1,
                        guides: vec![],
                    },
                )
                .unwrap();
        };
        let (result, ()) = tokio::join!(service.wait_review(&id, 2_000), submitted);
        assert_eq!(result.unwrap().status, "submitted");
    }

    #[tokio::test]
    async fn wait_review_returns_pending_at_timeout() {
        let store = Arc::new(ReviewStore::new());
        let id = store
            .create_review(None, vec![], store.generate_token())
            .unwrap();
        let service = ReviewService::new(store, None);
        assert_eq!(service.wait_review(&id, 1).await.unwrap().status, "pending");
    }

    #[tokio::test]
    async fn resolves_macos_window_annotations() {
        let store = Arc::new(ReviewStore::new());
        let snapshot = StoredSnapshot {
            id: "window-snapshot".into(),
            review_id: String::new(),
            position: 0,
            renderer_id: "macos_window".into(),
            mapping_version: 1,
            width: 820,
            height: 680,
            mime_type: "image/png".into(),
            asset_path: String::new(),
            source_label: "Retake — Retake".into(),
            mapping_json: json!({
                "version": 1,
                "renderer": "macos_window",
                "window_id": 42,
                "owner": "Retake",
                "title": "Retake"
            })
            .to_string(),
        };
        let id = store
            .create_review(None, vec![(snapshot, Vec::new())], store.generate_token())
            .unwrap();
        let service = ReviewService::new(store, None);
        let annotations = service
            .resolve_annotations(
                &id,
                vec![RawAnnotation {
                    snapshot_id: "window-snapshot".into(),
                    selection: Selection::Point { x: 30.0, y: 40.0 },
                    comment: "Remove this".into(),
                }],
            )
            .await
            .unwrap();
        assert!(annotations[0].reference.contains("macOS window position"));
    }

    #[test]
    fn pausing_window_clears_transient_update_but_keeps_feedback() {
        let store = Arc::new(ReviewStore::new());
        let id = store
            .create_review(None, vec![], store.generate_token())
            .unwrap();
        store
            .submit_review(
                &id,
                vec![],
                Generation {
                    variants_enabled: false,
                    count: 1,
                    guides: vec![],
                },
            )
            .unwrap();
        assert!(store.is_updating(&id).unwrap());

        ReviewService::new(store.clone(), None)
            .pause_update(&id)
            .unwrap();

        assert!(!store.is_updating(&id).unwrap());
        assert_eq!(store.get_review_state(&id).unwrap().status, "submitted");
        assert_eq!(store.list_feedback(&id).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn wait_reply_returns_after_user_answers() {
        let store = Arc::new(ReviewStore::new());
        let id = store
            .create_review(None, vec![], store.generate_token())
            .unwrap();
        let question = store.add_message(&id, "question", "Which color?").unwrap();
        let service = ReviewService::new(store.clone(), None);
        let respond = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            store.reply_message(&id, &question, "Green").unwrap();
        };
        let (answer, ()) = tokio::join!(service.wait_reply(&id, &question, 2_000), respond);
        assert_eq!(answer.unwrap(), ReplyOutcome::Replied("Green".into()));
    }

    #[tokio::test]
    async fn chat_handoff_finishes_both_waits_without_submitting() {
        let store = Arc::new(ReviewStore::new());
        let id = store
            .create_review(None, vec![], store.generate_token())
            .unwrap();
        let question = store.add_message(&id, "question", "Which color?").unwrap();
        let service = ReviewService::new(store.clone(), None);
        let handoff = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            store.handoff_message(&id, &question).unwrap();
        };
        let (result, answer, ()) = tokio::join!(
            service.wait_review(&id, 2_000),
            service.wait_reply(&id, &question, 2_000),
            handoff,
        );
        assert_eq!(result.unwrap().status, "chat");
        assert_eq!(answer.unwrap(), ReplyOutcome::Chat);
        assert_eq!(store.get_review_state(&id).unwrap().status, "pending");
        assert!(store.reply_message(&id, &question, "Green").is_err());
        assert!(store.list_messages(&id).unwrap()[0].handed_off_at.is_some());
    }
}
