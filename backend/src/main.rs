mod db;
mod error;
mod models;
mod routes;
mod ws;

use std::net::SocketAddr;

use axum::{
    routing::{get, patch, post},
    Router,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub events: broadcast::Sender<String>,
    pub board_title: String,
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
    let state = AppState { pool, events, board_title };

    let app = Router::new()
        .route("/api/board", get(routes::get_board))
        .route("/api/cards", post(routes::create_card))
        .route(
            "/api/cards/{id}",
            patch(routes::update_card).delete(routes::delete_card),
        )
        .route("/api/agent/action", post(routes::agent_action))
        .route("/ws", get(ws::handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("board api on http://{addr}  (db: {db_path}, ws: /ws)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
