//! The HTTP surface: a drop-in replacement for an nginx static host with JSON
//! autoindex. Directory requests return the same `[{name,type,…}]` array the
//! Gausify gallery crawls; file requests stream with Range support; CORS and an
//! immutable cache policy for frame files are applied for the whole tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeFile;

/// Shared, cheap-to-clone server state.
#[derive(Clone)]
pub struct AppState {
    pub root: Arc<PathBuf>,
    pub stats: Arc<Stats>,
}

pub struct Stats {
    pub requests: AtomicU64,
    pub bytes: AtomicU64,
    pub started: Instant,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            requests: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            started: Instant::now(),
        }
    }
}

/// nginx `autoindex_format json` entry (the fields the crawler reads: name/type).
#[derive(Serialize)]
struct DirEntry {
    name: String,
    #[serde(rename = "type")]
    kind: &'static str,
    mtime: String,
    size: u64,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .fallback(serve_path)
        .layer(cors())
        .with_state(state)
}

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE, RANGE])
        .expose_headers([CONTENT_LENGTH, CONTENT_RANGE, ACCEPT_RANGES])
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "requests": state.stats.requests.load(Ordering::Relaxed),
        "bytesServed": state.stats.bytes.load(Ordering::Relaxed),
        "uptimeSeconds": state.stats.started.elapsed().as_secs(),
        "library": state.root.display().to_string(),
    }))
}

/// Fallback: serve a directory as JSON, or a file with Range support.
async fn serve_path(State(state): State<AppState>, req: Request) -> Response {
    state.stats.requests.fetch_add(1, Ordering::Relaxed);

    let Some(rel) = safe_relative(req.uri().path()) else {
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    };
    let full = state.root.join(&rel);

    match tokio::fs::metadata(&full).await {
        Ok(meta) if meta.is_dir() => directory_listing(&full).await,
        Ok(_) => serve_file(&state, full, req).await,
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn directory_listing(dir: &Path) -> Response {
    let mut reader = match tokio::fs::read_dir(dir).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "cannot read directory").into_response(),
    };

    let mut entries: Vec<DirEntry> = Vec::new();
    while let Ok(Some(entry)) = reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Hide dotfiles — notably the `.gausify` cert cache we create.
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata().await else {
            continue;
        };
        let is_dir = meta.is_dir();
        entries.push(DirEntry {
            name,
            kind: if is_dir { "directory" } else { "file" },
            mtime: meta
                .modified()
                .ok()
                .map(httpdate::fmt_http_date)
                .unwrap_or_default(),
            size: if is_dir { 0 } else { meta.len() },
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Json(entries).into_response()
}

async fn serve_file(state: &AppState, full: PathBuf, req: Request) -> Response {
    let immutable = matches!(
        full.extension().and_then(|e| e.to_str()),
        Some("sog" | "ply" | "splat")
    );

    // ServeFile handles Range/If-Range, Content-Type, Content-Length, ETag and
    // Last-Modified; its Service error is Infallible (IO errors become
    // error responses), so the unwrap can never fire.
    let served = match ServeFile::new(&full).oneshot(req).await {
        Ok(response) => response,
        Err(err) => match err {},
    };

    if let Some(len) = served
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
    {
        state.stats.bytes.fetch_add(len, Ordering::Relaxed);
    }

    let mut response = served.into_response();
    if immutable {
        response.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

/// Resolve a request path to a safe relative path, rejecting traversal.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8().ok()?;
    let mut out = PathBuf::new();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return None,
            s if s.contains('\\') || s.contains('\0') => return None,
            s => out.push(s),
        }
    }
    Some(out)
}
