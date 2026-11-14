//! Thin HTTP adapters over `service` — parse the request, call the service, shape the response.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};

use crate::{error::AppResult, models::*, service, telegram, AppState};

fn origin(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-client-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(64).collect())
}

pub async fn get_board(State(state): State<AppState>) -> AppResult<Json<Board>> {
    Ok(Json(service::board(&state).await?))
}

pub async fn create_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateCard>,
) -> AppResult<(StatusCode, Json<Card>)> {
    let card = service::create_card(&state, input, origin(&headers)).await?;
    Ok((StatusCode::CREATED, Json(card)))
}

pub async fn update_card(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(patch): Json<PatchCard>,
) -> AppResult<Json<Card>> {
    Ok(Json(service::update_card(&state, &id, patch, origin(&headers)).await?))
}

pub async fn delete_card(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    service::delete_card(&state, &id, origin(&headers)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/agent/action — bot-friendly: start / complete / note.
/// A `complete` also pings Telegram (if the bot is configured) without delaying the response.
pub async fn agent_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AgentAction>,
) -> AppResult<Json<Card>> {
    let verb = input.action;
    let card = service::agent_action(&state, input, origin(&headers)).await?;
    if matches!(verb, AgentVerb::Complete) {
        if let Some(bot) = state.telegram.clone() {
            tokio::spawn(telegram::notify_agent_completed(bot, card.clone()));
        }
    }
    Ok(Json(card))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
        routing::{get, patch, post},
        Router,
    };
    use serde_json::{json, Value};
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    use super::*;
    use crate::db;

    async fn test_router() -> (Router, AppState, PathBuf) {
        let db_path = std::env::temp_dir().join(format!("shipmd-routes-{}.db", uuid::Uuid::new_v4()));
        let pool = db::connect(db_path.to_str().unwrap()).await.unwrap();
        let (events, _) = broadcast::channel(16);
        let state = AppState {
            pool,
            events,
            board_title: "Test".into(),
            telegram: None,
        };
        let app = Router::new()
            .route("/api/board", get(get_board))
            .route("/api/cards", post(create_card))
            .route("/api/cards/{id}", patch(update_card))
            .with_state(state.clone());
        (app, state, db_path)
    }

    async fn send_json(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, parsed)
    }

    fn cleanup_db(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn create_and_patch_card_accept_multi_agent_and_custom_assignees() {
        let (app, state, db_path) = test_router().await;

        for (raw, expected) in [
            ("antigravity", "antigravity"),
            ("hermes", "hermes"),
            ("  Codex  ", "codex"),
        ] {
            let (st, created) = send_json(
                &app,
                "POST",
                "/api/cards",
                json!({
                    "column_id": "backlog",
                    "title": format!("Task for {expected}"),
                    "tag": "build",
                    "assignee": raw
                }),
            )
            .await;
            assert_eq!(st, StatusCode::CREATED);
            assert_eq!(created["assignee"], expected);
        }

        let (st, card) = send_json(
            &app,
            "POST",
            "/api/cards",
            json!({
                "column_id": "backlog",
                "title": "Reassignable task",
                "assignee": "human"
            }),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let id = card["id"].as_str().unwrap();

        for (raw, expected) in [
            ("antigravity", "antigravity"),
            ("Hermes", "hermes"),
            ("alice-ops", "alice-ops"),
        ] {
            let (pst, patched) = send_json(
                &app,
                "PATCH",
                &format!("/api/cards/{id}"),
                json!({ "assignee": raw }),
            )
            .await;
            assert_eq!(pst, StatusCode::OK);
            assert_eq!(patched["assignee"], expected);
        }

        // Reject empty or >64-char assignees.
        let (st_empty, _) = send_json(
            &app,
            "PATCH",
            &format!("/api/cards/{id}"),
            json!({ "assignee": "   " }),
        )
        .await;
        assert_eq!(st_empty, StatusCode::BAD_REQUEST);

        let too_long = "a".repeat(65);
        let (st_long, _) = send_json(
            &app,
            "POST",
            "/api/cards",
            json!({
                "column_id": "backlog",
                "title": "Too long assignee",
                "assignee": too_long
            }),
        )
        .await;
        assert_eq!(st_long, StatusCode::BAD_REQUEST);

        state.pool.close().await;
        cleanup_db(&db_path);
    }

    #[tokio::test]
    async fn create_patch_and_board_handle_repo_labels_and_priority_order() {
        let (app, state, db_path) = test_router().await;

        let (st1, low) = send_json(
            &app,
            "POST",
            "/api/cards",
            json!({
                "column_id": "backlog",
                "title": "Low card",
                "assignee": "kimi",
                "repo": "Shipmd",
                "labels": ["#Frontend", "bug", "frontend"],
                "priority": 0
            }),
        )
        .await;
        assert_eq!(st1, StatusCode::CREATED);
        assert_eq!(low["assignee"], "kimi");
        assert_eq!(low["repo"], "shipmd");
        assert_eq!(low["labels"], json!(["frontend", "bug"]));
        assert_eq!(low["priority"], 0);

        let (st2, urgent) = send_json(
            &app,
            "POST",
            "/api/cards",
            json!({
                "column_id": "backlog",
                "title": "Urgent card",
                "repo": "ceviz-ai",
                "labels": "#api, urgent",
                "priority": 10
            }),
        )
        .await;
        assert_eq!(st2, StatusCode::CREATED);
        assert_eq!(urgent["repo"], "ceviz-ai");
        assert_eq!(urgent["labels"], json!(["api", "urgent"]));
        assert_eq!(urgent["priority"], 10);

        // GET /api/board orders higher priority cards first within each column.
        let board_resp = app
            .clone()
            .oneshot(Request::builder().method("GET").uri("/api/board").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(board_resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(board_resp.into_body(), 1024 * 1024).await.unwrap();
        let board_json: Value = serde_json::from_slice(&bytes).unwrap();
        let backlog_cards = board_json["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "backlog")
            .unwrap()["cards"]
            .as_array()
            .unwrap();
        assert_eq!(backlog_cards[0]["id"], urgent["id"]);

        // PATCH updates and clears repo & labels.
        let id = low["id"].as_str().unwrap();
        let (pst, patched) = send_json(
            &app,
            "PATCH",
            &format!("/api/cards/{id}"),
            json!({
                "repo": "",
                "labels": [],
                "priority": 8
            }),
        )
        .await;
        assert_eq!(pst, StatusCode::OK);
        assert_eq!(patched["repo"], Value::Null);
        assert_eq!(patched["labels"], json!([]));
        assert_eq!(patched["priority"], 8);

        state.pool.close().await;
        cleanup_db(&db_path);
    }
}
