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
