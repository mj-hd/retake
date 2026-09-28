use crate::review_service::{RawAnnotation, ReviewService};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use retake_core::{Generation, ReviewResult, Selection};
use retake_store::StoreError;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<ReviewService>,
    pub http_port: u16,
    pub ui_dir: std::path::PathBuf,
}

pub struct HttpServerHandle {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl HttpServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if timeout(Duration::from_secs(3), &mut self.task)
            .await
            .is_err()
        {
            warn!("http server did not stop gracefully; aborting task");
            self.task.abort();
            let _ = self.task.await;
        }
    }
}

pub async fn start_http(
    service: Arc<ReviewService>,
    port: u16,
    ui_dir: std::path::PathBuf,
) -> anyhow::Result<HttpServerHandle> {
    let state = AppState {
        service,
        http_port: port,
        ui_dir: ui_dir.clone(),
    };

    let app = Router::new()
        .route("/review/:id", get(serve_review_page))
        .route("/api/reviews/:id", get(get_review))
        .route("/api/reviews/:id/assets/:snapshot_id", get(get_asset))
        .route("/api/reviews/:id/scenes/:snapshot_id", get(get_scene))
        .route("/api/reviews/:id/submit", post(submit))
        .route("/api/reviews/:id/cancel", post(cancel))
        .route(
            "/api/reviews/:id/messages/:message_id/reply",
            post(reply_message),
        )
        .route(
            "/api/reviews/:id/messages/:message_id/chat",
            post(handoff_message),
        )
        .route("/api/reviews/:id/close", post(close_browser))
        .route("/api/reviews/:id/session", post(create_session))
        .nest_service("/static", ServeDir::new(ui_dir.join("dist")))
        .nest_service("/assets", ServeDir::new(ui_dir.join("dist/assets")))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();
    info!("http listening on http://127.0.0.1:{}", actual_port);
    let (shutdown, shutdown_requested) = oneshot::channel();
    let task = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_requested.await;
            })
            .await
        {
            warn!("http server failed: {}", error);
        }
    });
    Ok(HttpServerHandle {
        port: actual_port,
        shutdown: Some(shutdown),
        task,
    })
}

async fn serve_review_page(
    Path(_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let index = state.ui_dir.join("dist/index.html");
    if index.exists() {
        match tokio::fs::read_to_string(index).await {
            Ok(html) => axum::response::Html(html).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ui error").into_response(),
        }
    } else {
        (StatusCode::NOT_FOUND, "no ui built").into_response()
    }
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn extract_token(q: &TokenQuery, headers: &HeaderMap) -> Option<String> {
    if let Some(t) = &q.token {
        if !t.is_empty() {
            return Some(t.clone());
        }
    }
    if let Some(cookie_header) = headers.get(axum::http::header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for part in cookies.split(';') {
                let kv: Vec<&str> = part.trim().splitn(2, '=').collect();
                if kv.len() == 2 && kv[0] == "retake_session" {
                    return Some(kv[1].to_string());
                }
            }
        }
    }
    None
}

async fn get_review(
    Path(id): Path<String>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let tok = extract_token(&q, &headers);
    if let Some(t) = tok {
        if !state.service.validate_access(&id, &t) {
            return Err(AppError::Auth);
        }
    } else {
        return Err(AppError::Auth);
    }
    let state_res = state.service.get_state(&id)?;
    let updating = state.service.is_updating(&id)?;
    let revisions = state.service.get_revisions(&id)?;
    let feedback = state.service.get_feedback(&id)?;
    let messages = state.service.get_messages(&id)?;
    let snap_info = |s: &retake_core::StoredSnapshot| {
        let asset_url = format!("/api/reviews/{}/assets/{}", id, s.id);
        let mapping =
            serde_json::from_str::<serde_json::Value>(&s.mapping_json).unwrap_or_default();
        let scene_url = if mapping["renderer"] == "code" {
            Some(format!("/api/reviews/{}/scenes/{}", id, s.id))
        } else {
            None
        };
        let is_document = matches!(mapping["kind"].as_str(), Some("markdown" | "pdf" | "video"));
        let diff_modes = if s.renderer_id == "code" {
            serde_json::json!(["difference"])
        } else {
            serde_json::json!(["split", "slider", "difference"])
        };
        serde_json::json!({
            "id": s.id,
            "label": s.source_label,
            "width": s.width,
            "height": s.height,
            "mime_type": s.mime_type,
            "asset_url": asset_url,
            "scene_url": scene_url,
            "layout": if is_document { "document" } else { "canvas" },
            "diff_modes": diff_modes,
            "zoom_group": mapping["zoom_group"].as_u64(),
            "zoom_level": mapping["zoom_level"].as_u64(),
            "variants_enabled": false, // initial per spec: only if guide exists
        })
    };
    let history: Vec<_> = revisions
        .iter()
        .map(|rev| {
            serde_json::json!({
                "number": rev.number,
                "created_at": rev.created_at,
                "snapshots": rev.snapshots.iter().map(&snap_info).collect::<Vec<_>>()
            })
        })
        .collect();
    let snap_infos = revisions
        .last()
        .map(|r| r.snapshots.iter().map(&snap_info).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "review_id": id,
        "title": "", // store not keep title for get? add later
        "status": if updating { "updating" } else { &state_res.status },
        "snapshots": snap_infos,
        "annotations": state_res.annotations,
        "revisions": history,
        "feedback": feedback.iter().map(|f| serde_json::json!({"number": f.number, "annotations": f.annotations})).collect::<Vec<_>>(),
        "messages": messages,
    })))
}

/// Scene data for dynamic (non-raster) review panes. Local paths are stripped:
/// the UI gets geometry and source text, not the target spec.
async fn get_scene(
    Path((id, sid)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let tok = extract_token(&q, &headers);
    match tok {
        Some(t) if state.service.validate_access(&id, &t) => {}
        _ => return Err(AppError::Auth),
    }
    let scene = state.service.get_scene(&id, &sid)?;
    Ok(Json(scene))
}

async fn get_asset(
    Path((id, sid)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let tok = extract_token(&q, &headers);
    if let Some(t) = tok {
        if !state.service.validate_access(&id, &t) {
            return Err(AppError::Auth);
        }
    } else {
        return Err(AppError::Auth);
    }
    let bytes = state.service.get_asset(&id, &sid)?;
    let mime = "image/png"; // assume
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes))
}

#[derive(Deserialize)]
struct SubmitBody {
    annotations: Vec<RawAnnIn>,
    generation: Option<GenIn>,
}

#[derive(Deserialize)]
struct RawAnnIn {
    snapshot_id: String,
    selection: Selection,
    comment: String,
}

#[derive(Deserialize)]
struct GenIn {
    variants_enabled: bool,
    count: u8,
}

async fn submit(
    Path(id): Path<String>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SubmitBody>,
) -> Result<Json<ReviewResult>, AppError> {
    let tok = extract_token(&q, &headers);
    if let Some(t) = tok {
        if !state.service.validate_access(&id, &t) {
            return Err(AppError::Auth);
        }
    } else {
        return Err(AppError::Auth);
    }
    let mut raw = vec![];
    for a in body.annotations {
        if a.comment.trim().is_empty() {
            continue;
        }
        raw.push(RawAnnotation {
            snapshot_id: a.snapshot_id,
            selection: a.selection,
            comment: a.comment,
        });
    }
    if raw.is_empty() {
        return Err(AppError::BadRequest(
            "at least one annotation required".into(),
        ));
    }
    let anns = state
        .service
        .resolve_annotations(&id, raw)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let gen = body
        .generation
        .map(|g| Generation {
            variants_enabled: g.variants_enabled,
            count: g.count,
            guides: vec![],
        })
        .unwrap_or(Generation {
            variants_enabled: false,
            count: 1,
            guides: vec![],
        });

    // variants not supported in this release for any renderer
    if gen.variants_enabled || gen.count > 1 {
        return Err(AppError::BadRequest(
            "variants not supported (set variants_enabled:false, count:1)".into(),
        ));
    }

    let res = state.service.submit(&id, anns, gen)?;
    Ok(Json(res))
}

async fn close_browser(
    Path(id): Path<String>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    match extract_token(&q, &headers) {
        Some(t) if state.service.validate_access(&id, &t) => {}
        _ => return Err(AppError::Auth),
    }
    state.service.pause_update(&id)?;
    let service = state.service.clone();
    tokio::spawn(async move {
        service.close_browser(&id).await;
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel(
    Path(id): Path<String>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ReviewResult>, AppError> {
    let tok = extract_token(&q, &headers);
    if let Some(t) = tok {
        if !state.service.validate_access(&id, &t) {
            return Err(AppError::Auth);
        }
    } else {
        return Err(AppError::Auth);
    }
    let res = state.service.cancel(&id)?;
    let service = state.service.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        service.close_browser(&id).await;
    });
    Ok(Json(res))
}

#[derive(Deserialize)]
struct ReplyBody {
    text: String,
}

async fn reply_message(
    Path((id, message_id)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReplyBody>,
) -> Result<StatusCode, AppError> {
    match extract_token(&q, &headers) {
        Some(t) if state.service.validate_access(&id, &t) => {}
        _ => return Err(AppError::Auth),
    }
    state
        .service
        .reply_message(&id, &message_id, &body.text)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn handoff_message(
    Path((id, message_id)): Path<(String, String)>,
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    match extract_token(&q, &headers) {
        Some(t) if state.service.validate_access(&id, &t) => {}
        _ => return Err(AppError::Auth),
    }
    state
        .service
        .handoff_message(&id, &message_id)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct SessionBody {
    token: String,
}

async fn create_session(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<SessionBody>,
) -> Result<impl IntoResponse, AppError> {
    if !state.service.validate_access(&id, &body.token) {
        return Err(AppError::Auth);
    }
    // set cookie
    let mut headers = HeaderMap::new();
    // simple cookie, no real secure for local
    headers.insert(
        axum::http::header::SET_COOKIE,
        format!(
            "retake_session={}; HttpOnly; SameSite=Strict; Path=/",
            body.token
        )
        .parse()
        .unwrap(),
    );
    Ok((headers, Json(serde_json::json!({"ok": true}))))
}

#[derive(Debug)]
enum AppError {
    #[allow(dead_code)]
    NotFound,
    Auth,
    BadRequest(String),
    Store(StoreError),
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        AppError::Store(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::Auth => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::Store(StoreError::NotFound) => {
                (StatusCode::NOT_FOUND, "not found").into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "error").into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::start_http;
    use crate::review_service::ReviewService;
    use retake_store::ReviewStore;
    use std::{path::PathBuf, sync::Arc};

    #[tokio::test]
    async fn shutdown_closes_http_listener() {
        let store = Arc::new(ReviewStore::new());
        let service = Arc::new(ReviewService::new(store, None));
        let server = start_http(service, 0, PathBuf::from("ui")).await.unwrap();
        let address = ("127.0.0.1", server.port());

        tokio::net::TcpStream::connect(address).await.unwrap();
        server.shutdown().await;

        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}
