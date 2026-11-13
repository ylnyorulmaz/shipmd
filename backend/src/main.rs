mod auth;
mod db;
mod error;
mod mcp;
mod models;
mod routes;
mod service;
mod telegram;
mod ws;

use std::{net::SocketAddr, path::PathBuf};

use axum::{
    middleware,
    routing::{get, patch, post},
    Router,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    pub board_title: String,
    /// Present when TELEGRAM_BOT_TOKEN is configured; used for outbound agent notifications.
    pub telegram: Option<telegram::Telegram>,
}

impl AppState {
    /// Broadcast a mutation to every connected WebSocket client.
    /// `origin` is the `X-Client-Id` of the tab that caused it (lets that tab skip its own echo).
    pub fn emit(&self, kind: &str, mut payload: Value, origin: Option<String>) {
        payload["type"] = json!(kind);
        payload["origin"] = json!(origin);
        // Err only means "no subscribers right now", which is fine.
        let _ = self.events.send(payload.to_string());
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load backend/.env if present (real env vars win).
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let db_path = std::env::var("BOARD_DB").unwrap_or_else(|_| "board.db".into());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3001);
    let board_title = std::env::var("BOARD_TITLE").unwrap_or_else(|_| "Ship Board".into());

    let pool = db::connect(&db_path).await?;
    let (events, _) = broadcast::channel(256);
    let telegram = telegram::Telegram::from_env();
    let state = AppState { pool, events, board_title, telegram };

    // Telegram long-polling runs beside the web server on the same state:
    // same SQLite pool, same WebSocket broadcast channel.
    if let Some(bot) = state.telegram.clone() {
        tokio::spawn(telegram::run(bot, state.clone()));
    }

    // Board routes: REST API, WebSocket and (in production) the built frontend.
    let mut board = Router::new()
        .route("/api/board", get(routes::get_board))
        .route("/api/cards", post(routes::create_card))
        .route(
            "/api/cards/{id}",
            patch(routes::update_card).delete(routes::delete_card),
        )
        .route("/api/agent/action", post(routes::agent_action))
        .route("/ws", get(ws::handler))
        .with_state(state.clone());

    // STATIC_DIR (e.g. frontend/dist) is served when it exists; unknown paths get index.html.
    let static_dir = std::env::var("STATIC_DIR").map(PathBuf::from).ok().filter(|d| d.join("index.html").is_file());
    if let Some(dir) = &static_dir {
        tracing::info!("serving frontend from {}", dir.display());
        board = board.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))));
    }

    // Optional BOARD_PASSWORD guard over the board routes (/mcp has its own auth).
    let board = board.layer(middleware::from_fn_with_state(auth::BoardAuth::from_env(), auth::require));

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(board)
        .merge(mcp::router(state))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("board api on http://{addr}  (db: {db_path}, ws: /ws)");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
