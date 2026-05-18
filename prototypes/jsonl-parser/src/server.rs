use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::broadcast};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::{
    db::DbPool,
    types::{HookPayload, HookResponse, WsMessage},
    watcher,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: Arc<DbPool>,
    pub token: String,
    pub watcher_tx: tokio::sync::mpsc::Sender<std::path::PathBuf>,
    pub ws_tx: broadcast::Sender<WsMessage>,
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected = format!("Bearer {}", state.token);
    if auth != expected {
        warn!("Unauthorized request (wrong/missing token)");
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"unauthorized"}))).into_response();
    }
    next.run(req).await
}

async fn handle_session_start(
    State(state): State<AppState>,
    Json(payload): Json<HookPayload>,
) -> impl IntoResponse {
    let sid = payload.session_id.as_deref().unwrap_or("unknown").to_string();
    let path = payload.transcript_path.as_deref().unwrap_or("").to_string();
    let cwd = payload.cwd.as_deref().unwrap_or("").to_string();

    info!(session_id = %sid, "hook: session-start");

    let pool = Arc::clone(&state.pool);
    let sid_c = sid.clone();
    let path_c = path.clone();
    let cwd_c = cwd.clone();
    let tx = state.watcher_tx.clone();
    let ws_tx = state.ws_tx.clone();

    tokio::task::spawn_blocking(move || {
        if let Err(e) = crate::db::upsert_session_from_hook(&pool, &sid_c, &path_c, &cwd_c) {
            warn!("session-start db error: {}", e);
        } else {
            let _ = ws_tx.send(WsMessage::SessionUpsert { session_id: sid_c });
        }
    });

    // Trigger watcher để parse file ngay nếu có path
    if !path.is_empty() {
        let _ = tx.try_send(path.into());
    }

    Json(HookResponse::default())
}

async fn handle_user_prompt_submit(
    State(state): State<AppState>,
    Json(payload): Json<HookPayload>,
) -> impl IntoResponse {
    let sid = payload.session_id.as_deref().unwrap_or("unknown");
    info!(session_id = %sid, "hook: user-prompt-submit");

    // Trigger watcher refresh
    if let Some(path) = &payload.transcript_path {
        let _ = state.watcher_tx.try_send(path.into());
    }

    Json(HookResponse::default())
}

async fn handle_stop(
    State(state): State<AppState>,
    Json(payload): Json<HookPayload>,
) -> impl IntoResponse {
    let sid = payload.session_id.as_deref().unwrap_or("unknown").to_string();
    info!(session_id = %sid, "hook: stop");

    let pool = Arc::clone(&state.pool);
    let path_opt = payload.transcript_path.clone();
    let tx = state.watcher_tx.clone();

    let ws_tx = state.ws_tx.clone();
    tokio::task::spawn_blocking(move || {
        if let Some(ref path) = path_opt {
            let p = std::path::Path::new(path);
            if p.exists() {
                match watcher::process_file(&pool, p) {
                    Ok(Some(report)) => {
                        let _ = ws_tx.send(WsMessage::SessionUpsert {
                            session_id: report.session_id.clone(),
                        });
                        if report.inserted > 0 {
                            let _ = ws_tx.send(WsMessage::EventBatch {
                                session_id: report.session_id,
                                inserted: report.inserted,
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(e) => warn!("stop hook parse error: {}", e),
                }
            }
            let _ = tx.try_send(p.to_path_buf());
        }
    });

    Json(HookResponse::default())
}

async fn handle_session_end(
    State(state): State<AppState>,
    Json(payload): Json<HookPayload>,
) -> impl IntoResponse {
    let sid = payload.session_id.as_deref().unwrap_or("unknown").to_string();
    info!(session_id = %sid, "hook: session-end");

    let pool = Arc::clone(&state.pool);
    let sid_c = sid.clone();
    let ws_tx = state.ws_tx.clone();

    tokio::task::spawn_blocking(move || {
        if let Err(e) = crate::db::mark_session_ended(&pool, &sid_c) {
            warn!("session-end db error: {}", e);
        } else {
            let _ = ws_tx.send(WsMessage::SessionUpsert { session_id: sid_c });
        }
    });

    Json(HookResponse::default())
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_upgrade(
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Auth qua query param (browser WebSocket API không support custom headers).
    if q.token.as_deref() != Some(state.token.as_str()) {
        warn!("WS upgrade unauthorized (wrong/missing token)");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"unauthorized"})),
        )
            .into_response();
    }
    let rx = state.ws_tx.subscribe();
    ws.on_upgrade(move |socket| ws_handler(socket, rx))
}

async fn ws_handler(mut socket: WebSocket, mut rx: broadcast::Receiver<WsMessage>) {
    info!("WS client connected");
    // Welcome message để frontend biết kết nối OK
    let _ = socket
        .send(Message::Text(r#"{"kind":"hello"}"#.to_string()))
        .await;

    loop {
        tokio::select! {
            res = rx.recv() => {
                match res {
                    Ok(msg) => {
                        let payload = match serde_json::to_string(&msg) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("WS serialize error: {}", e);
                                continue;
                            }
                        };
                        if socket.send(Message::Text(payload)).await.is_err() {
                            break;
                        }
                    }
                    // Lagged: bỏ qua, client tự sync lại lần message tiếp theo
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("WS client lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Đọc ping/close từ client để giữ socket alive
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = socket.send(Message::Pong(p)).await;
                    }
                    Some(Err(e)) => {
                        warn!("WS recv error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    info!("WS client disconnected");
}

pub async fn run_server(
    state: AppState,
    port: u16,
    cancel: CancellationToken,
) -> Result<()> {
    // Auth-protected hooks router (chỉ POST cần Bearer)
    let hooks_router = Router::new()
        .route("/hooks/session-start", post(handle_session_start))
        .route("/hooks/user-prompt-submit", post(handle_user_prompt_submit))
        .route("/hooks/stop", post(handle_stop))
        .route("/hooks/session-end", post(handle_session_end))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // WS có auth riêng qua query token (không qua middleware)
    let app = hooks_router
        .route("/ws", get(ws_upgrade))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
            info!("HTTP server shutting down");
        })
        .await?;

    Ok(())
}
