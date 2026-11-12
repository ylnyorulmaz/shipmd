use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::*,
    now, AppState,
};

const GAP: f64 = 1024.0;
const CARD_COLS: &str = "id, column_id, title, description, tag, assignee, position, updated_at";

fn origin(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-client-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(64).collect())
}

fn clean_title(raw: &str) -> AppResult<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }
    if t.chars().count() > 500 {
        return Err(AppError::BadRequest("title is too long (max 500 chars)".into()));
    }
    Ok(t.to_string())
}

fn clean_description(raw: Option<String>) -> Option<String> {
    raw.filter(|d| !d.trim().is_empty())
}

fn check_position(p: f64) -> AppResult<f64> {
    if p.is_finite() {
        Ok(p)
    } else {
        Err(AppError::BadRequest("position must be a finite number".into()))
    }
}

async fn fetch_card(state: &AppState, id: &str) -> AppResult<Card> {
    sqlx::query_as::<_, Card>(&format!("SELECT {CARD_COLS} FROM cards WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("card {id} not found")))
}

async fn ensure_column(state: &AppState, column_id: &str) -> AppResult<()> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM columns WHERE id = ?")
        .bind(column_id)
        .fetch_optional(&state.pool)
        .await?;
    exists
        .map(|_| ())
        .ok_or_else(|| AppError::BadRequest(format!("unknown column_id '{column_id}'")))
}

async fn end_position(state: &AppState, column_id: &str) -> AppResult<f64> {
    let (max,): (Option<f64>,) = sqlx::query_as("SELECT MAX(position) FROM cards WHERE column_id = ?")
        .bind(column_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(max.map_or(GAP, |m| m + GAP))
}

async fn top_position(state: &AppState, column_id: &str) -> AppResult<f64> {
    let (min,): (Option<f64>,) = sqlx::query_as("SELECT MIN(position) FROM cards WHERE column_id = ?")
        .bind(column_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(min.map_or(GAP, |m| m - GAP))
}

async fn save_card(state: &AppState, c: &Card) -> AppResult<()> {
    sqlx::query(
        "UPDATE cards SET column_id = ?, title = ?, description = ?, tag = ?, assignee = ?,
                          position = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&c.column_id)
    .bind(&c.title)
    .bind(&c.description)
    .bind(&c.tag)
    .bind(&c.assignee)
    .bind(c.position)
    .bind(&c.updated_at)
    .bind(&c.id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// GET /api/board
// ---------------------------------------------------------------------------

pub async fn get_board(State(state): State<AppState>) -> AppResult<Json<Board>> {
    let columns = sqlx::query_as::<_, Column>("SELECT id, title, position FROM columns ORDER BY position")
        .fetch_all(&state.pool)
        .await?;
    let cards = sqlx::query_as::<_, Card>(&format!(
        "SELECT {CARD_COLS} FROM cards ORDER BY position, updated_at"
    ))
    .fetch_all(&state.pool)
    .await?;

    let columns = columns
        .into_iter()
        .map(|column| {
            let cards = cards.iter().filter(|c| c.column_id == column.id).cloned().collect();
            ColumnWithCards { column, cards }
        })
        .collect();

    Ok(Json(Board { title: state.board_title.clone(), columns }))
}

// ---------------------------------------------------------------------------
// POST /api/cards
// ---------------------------------------------------------------------------

pub async fn create_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateCard>,
) -> AppResult<(StatusCode, Json<Card>)> {
    let title = clean_title(&input.title)?;
    ensure_column(&state, &input.column_id).await?;
    let position = match input.position {
        Some(p) => check_position(p)?,
        None => end_position(&state, &input.column_id).await?,
    };

    let card = Card {
        id: input.id.unwrap_or_else(uuid::Uuid::new_v4).to_string(),
        column_id: input.column_id,
        title,
        description: clean_description(input.description),
        tag: input.tag.unwrap_or(Tag::Build).as_str().into(),
        assignee: input.assignee.unwrap_or(Assignee::Human).as_str().into(),
        position,
        updated_at: now(),
    };

    let res = sqlx::query(
        "INSERT INTO cards (id, column_id, title, description, tag, assignee, position, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&card.id)
    .bind(&card.column_id)
    .bind(&card.title)
    .bind(&card.description)
    .bind(&card.tag)
    .bind(&card.assignee)
    .bind(card.position)
    .bind(&card.updated_at)
    .execute(&state.pool)
    .await;

    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::BadRequest(format!("card {} already exists", card.id)));
        }
    }
    res?;

    state.emit("CARD_CREATED", json!({ "card": card }), origin(&headers));
    Ok((StatusCode::CREATED, Json(card)))
}

// ---------------------------------------------------------------------------
// PATCH /api/cards/:id
// ---------------------------------------------------------------------------

pub async fn update_card(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(patch): Json<PatchCard>,
) -> AppResult<Json<Card>> {
    let mut card = fetch_card(&state, &id).await?;
    let mut moved = false;

    if let Some(col) = patch.column_id {
        if col != card.column_id {
            ensure_column(&state, &col).await?;
            card.column_id = col;
            moved = true;
            if patch.position.is_none() {
                card.position = end_position(&state, &card.column_id).await?;
            }
        }
    }
    if let Some(p) = patch.position {
        let p = check_position(p)?;
        if p != card.position {
            card.position = p;
            moved = true;
        }
    }
    if let Some(t) = patch.title {
        card.title = clean_title(&t)?;
    }
    if let Some(d) = patch.description {
        card.description = clean_description(Some(d));
    }
    if let Some(tag) = patch.tag {
        card.tag = tag.as_str().into();
    }
    if let Some(a) = patch.assignee {
        card.assignee = a.as_str().into();
    }

    card.updated_at = now();
    save_card(&state, &card).await?;

    let kind = if moved { "CARD_MOVED" } else { "CARD_UPDATED" };
    state.emit(kind, json!({ "card": card }), origin(&headers));
    Ok(Json(card))
}

// ---------------------------------------------------------------------------
// DELETE /api/cards/:id
// ---------------------------------------------------------------------------

pub async fn delete_card(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM cards WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("card {id} not found")));
    }
    state.emit("CARD_DELETED", json!({ "id": id }), origin(&headers));
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/agent/action   — bot-friendly: start / complete / note
// ---------------------------------------------------------------------------

/// Resolve a full id or a unique short prefix (the 8-char id shown in the UI).
async fn resolve_card(state: &AppState, key: &str) -> AppResult<Card> {
    let key = key.trim().to_ascii_lowercase();
    if key.len() < 4 || !key.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(AppError::BadRequest(
            "card_id must be a UUID or a unique prefix of at least 4 hex chars".into(),
        ));
    }
    let mut hits = sqlx::query_as::<_, Card>(&format!(
        "SELECT {CARD_COLS} FROM cards WHERE id LIKE ? || '%' LIMIT 2"
    ))
    .bind(&key)
    .fetch_all(&state.pool)
    .await?;
    match hits.len() {
        0 => Err(AppError::NotFound(format!("no card matches '{key}'"))),
        1 => Ok(hits.remove(0)),
        _ => Err(AppError::BadRequest(format!("'{key}' matches more than one card"))),
    }
}

pub async fn agent_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AgentAction>,
) -> AppResult<Json<Card>> {
    let mut card = resolve_card(&state, &input.card_id).await?;
    let from_column = card.column_id.clone();

    let target = match input.action {
        AgentVerb::Start => {
            card.assignee = Assignee::Agent.as_str().into();
            Some(if card.tag == "sell" { "sell" } else { "build" })
        }
        AgentVerb::Complete => Some("done"),
        AgentVerb::Note => None,
    };

    if let Some(col) = target {
        if col != card.column_id {
            card.column_id = col.into();
            card.position = top_position(&state, col).await?;
        }
    }

    let verb = match input.action {
        AgentVerb::Start => "started",
        AgentVerb::Complete => "completed",
        AgentVerb::Note => "noted",
    };
    let note = input.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let entry = match note {
        Some(n) => format!("🤖 **Agent {verb}** · _{stamp}_\n\n{n}"),
        None => format!("🤖 **Agent {verb}** · _{stamp}_"),
    };
    if note.is_some() || !matches!(input.action, AgentVerb::Note) {
        card.description = Some(match card.description.take() {
            Some(d) if !d.trim().is_empty() => format!("{d}\n\n---\n\n{entry}"),
            _ => entry,
        });
    }

    card.updated_at = now();
    save_card(&state, &card).await?;

    let kind = if card.column_id != from_column { "CARD_MOVED" } else { "CARD_UPDATED" };
    state.emit(kind, json!({ "card": card }), origin(&headers));
    Ok(Json(card))
}
