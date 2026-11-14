//! Board domain logic shared by every entry point (HTTP API, agent endpoint, Telegram bot).
//!
//! Each mutation validates, persists to SQLite and broadcasts the matching WebSocket event,
//! so callers never touch SQL or the broadcast channel directly.

use serde_json::json;

use crate::{
    error::{AppError, AppResult},
    models::*,
    now, AppState,
};

const GAP: f64 = 1024.0;
const CARD_COLS: &str =
    "id, column_id, title, description, tag, assignee, status, priority, pr_url, repo, labels, position, updated_at";

/// Column the Build/Sell ratio counts as "shipped".
pub const DONE_COLUMN: &str = "done";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn clean_assignee(raw: &str) -> AppResult<String> {
    let a = raw.trim();
    if a.is_empty() {
        return Err(AppError::BadRequest("assignee must not be empty".into()));
    }
    if a.chars().count() > 64 || a.chars().any(char::is_control) {
        return Err(AppError::BadRequest("assignee must be at most 64 printable chars".into()));
    }
    Ok(a.to_ascii_lowercase())
}

fn clean_pr_url(raw: Option<String>) -> AppResult<Option<String>> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > 2048 || trimmed.chars().any(char::is_control) {
                return Err(AppError::BadRequest("pr_url is invalid or too long".into()));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

fn clean_repo(raw: Option<String>) -> AppResult<Option<String>> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > 64 || trimmed.chars().any(char::is_control) {
                return Err(AppError::BadRequest("repo must be at most 64 printable chars".into()));
            }
            Ok(Some(trimmed.to_ascii_lowercase()))
        }
    }
}

fn clean_labels(raw: Vec<String>) -> AppResult<Vec<String>> {
    let labels = normalize_label_list(raw);
    if labels.len() > 20 {
        return Err(AppError::BadRequest("at most 20 labels are allowed per card".into()));
    }
    for label in &labels {
        if label.chars().count() > 40 || label.chars().any(char::is_control) {
            return Err(AppError::BadRequest("each label must be at most 40 printable chars".into()));
        }
    }
    Ok(labels)
}

fn labels_json(labels: &[String]) -> String {
    serde_json::to_string(labels).unwrap_or_else(|_| "[]".into())
}

pub fn status_for_column(col: &str) -> &'static str {
    match col {
        "backlog" => "todo",
        "build" => "in_progress",
        "sell" => "sell",
        "done" => "done",
        _ => "todo",
    }
}

pub fn normalize_status(raw: &str) -> AppResult<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "todo" | "backlog" => Ok("todo"),
        "in_progress" | "in-progress" | "in progress" | "build" => Ok("in_progress"),
        "review" => Ok("review"),
        "sell" => Ok("sell"),
        "done" => Ok("done"),
        other => Err(AppError::BadRequest(format!(
            "unknown status '{other}' — use todo, in_progress, review, sell, or done"
        ))),
    }
}

fn target_column_for_status(status: &str, current_column: Option<&str>) -> &'static str {
    match status {
        "todo" => "backlog",
        "in_progress" => "build",
        "review" => match current_column {
            Some("sell") => "sell",
            _ => "build",
        },
        "sell" => "sell",
        "done" => DONE_COLUMN,
        _ => "backlog",
    }
}

fn check_position(p: f64) -> AppResult<f64> {
    if p.is_finite() {
        Ok(p)
    } else {
        Err(AppError::BadRequest("position must be a finite number".into()))
    }
}

pub async fn fetch_card(state: &AppState, id: &str) -> AppResult<Card> {
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
                          status = ?, priority = ?, pr_url = ?, repo = ?, labels = ?, position = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&c.column_id)
    .bind(&c.title)
    .bind(&c.description)
    .bind(&c.tag)
    .bind(&c.assignee)
    .bind(&c.status)
    .bind(c.priority)
    .bind(&c.pr_url)
    .bind(&c.repo)
    .bind(labels_json(&c.labels))
    .bind(c.position)
    .bind(&c.updated_at)
    .bind(&c.id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub async fn board(state: &AppState) -> AppResult<Board> {
    let columns = sqlx::query_as::<_, Column>("SELECT id, title, position FROM columns ORDER BY position")
        .fetch_all(&state.pool)
        .await?;
    let cards = sqlx::query_as::<_, Card>(&format!(
        "SELECT {CARD_COLS} FROM cards ORDER BY priority DESC, position ASC, updated_at ASC"
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

    Ok(Board { title: state.board_title.clone(), columns })
}

/// Build vs. Sell split of shipped (Done) cards — same rule as the web header bar.
#[derive(Debug, Clone, Copy)]
pub struct Ratio {
    pub build: i64,
    pub sell: i64,
}

impl Ratio {
    pub fn build_pct(&self) -> i64 {
        let total = self.build + self.sell;
        if total == 0 {
            0
        } else {
            ((self.build as f64 / total as f64) * 100.0).round() as i64
        }
    }
    pub fn sell_pct(&self) -> i64 {
        if self.build + self.sell == 0 {
            0
        } else {
            100 - self.build_pct()
        }
    }
    pub fn is_build_heavy(&self) -> bool {
        self.build_pct() > 75
    }
}

pub async fn shipped_ratio(state: &AppState) -> AppResult<Ratio> {
    let (build, sell): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(tag = 'build'), 0), COALESCE(SUM(tag = 'sell'), 0)
         FROM cards WHERE column_id = ?",
    )
    .bind(DONE_COLUMN)
    .fetch_one(&state.pool)
    .await?;
    Ok(Ratio { build, sell })
}

// ---------------------------------------------------------------------------
// Mutations (each one broadcasts its WebSocket event)
// ---------------------------------------------------------------------------

pub async fn create_card(state: &AppState, input: CreateCard, origin: Option<String>) -> AppResult<Card> {
    let title = clean_title(&input.title)?;
    ensure_column(state, &input.column_id).await?;
    let position = match input.position {
        Some(p) => check_position(p)?,
        None => end_position(state, &input.column_id).await?,
    };
    let assignee = match input.assignee {
        Some(a) => clean_assignee(&a)?,
        None => Assignee::Human.as_str().into(),
    };
    let status = match input.status {
        Some(s) => normalize_status(&s)?.into(),
        None => status_for_column(&input.column_id).into(),
    };
    let pr_url = clean_pr_url(input.pr_url)?;
    let repo = clean_repo(input.repo)?;
    let labels = clean_labels(input.labels.unwrap_or_default())?;

    let card = Card {
        id: input.id.unwrap_or_else(uuid::Uuid::new_v4).to_string(),
        column_id: input.column_id,
        title,
        description: clean_description(input.description),
        tag: input.tag.unwrap_or(Tag::Build).as_str().into(),
        assignee,
        status,
        priority: input.priority.unwrap_or(0),
        pr_url,
        repo,
        labels,
        position,
        updated_at: now(),
    };

    let res = sqlx::query(
        "INSERT INTO cards (id, column_id, title, description, tag, assignee, status, priority, pr_url, repo, labels, position, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&card.id)
    .bind(&card.column_id)
    .bind(&card.title)
    .bind(&card.description)
    .bind(&card.tag)
    .bind(&card.assignee)
    .bind(&card.status)
    .bind(card.priority)
    .bind(&card.pr_url)
    .bind(&card.repo)
    .bind(labels_json(&card.labels))
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

    state.emit("CARD_CREATED", json!({ "card": card }), origin);
    Ok(card)
}

pub async fn update_card(state: &AppState, id: &str, patch: PatchCard, origin: Option<String>) -> AppResult<Card> {
    let mut card = fetch_card(state, id).await?;
    let mut moved = false;
    let has_explicit_column = patch.column_id.is_some();

    if let Some(col) = patch.column_id {
        if col != card.column_id {
            ensure_column(state, &col).await?;
            card.column_id = col;
            moved = true;
            if patch.position.is_none() {
                // Done is newest-first; every other column appends.
                card.position = if card.column_id == DONE_COLUMN {
                    top_position(state, &card.column_id).await?
                } else {
                    end_position(state, &card.column_id).await?
                };
            }
            if patch.status.is_none() {
                card.status = status_for_column(&card.column_id).into();
            }
        }
    }
    if let Some(s) = patch.status {
        let normalized = normalize_status(&s)?;
        card.status = normalized.into();
        if !has_explicit_column {
            let target_col = target_column_for_status(normalized, Some(&card.column_id));
            if target_col != card.column_id {
                card.column_id = target_col.into();
                moved = true;
                if patch.position.is_none() {
                    card.position = if card.column_id == DONE_COLUMN {
                        top_position(state, &card.column_id).await?
                    } else {
                        end_position(state, &card.column_id).await?
                    };
                }
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
        card.assignee = clean_assignee(&a)?;
    }
    if let Some(priority) = patch.priority {
        card.priority = priority;
    }
    if let Some(pr_url) = patch.pr_url {
        card.pr_url = clean_pr_url(Some(pr_url))?;
    }
    if let Some(repo) = patch.repo {
        card.repo = clean_repo(Some(repo))?;
    }
    if let Some(labels) = patch.labels {
        card.labels = clean_labels(labels)?;
    }

    card.updated_at = now();
    save_card(state, &card).await?;

    let kind = if moved { "CARD_MOVED" } else { "CARD_UPDATED" };
    state.emit(kind, json!({ "card": card }), origin);
    Ok(card)
}

/// Deletes a card and returns what it was (handy for "Deleted: …" confirmations).
pub async fn delete_card(state: &AppState, id: &str, origin: Option<String>) -> AppResult<Card> {
    let card = sqlx::query_as::<_, Card>(&format!("DELETE FROM cards WHERE id = ? RETURNING {CARD_COLS}"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("card {id} not found")))?;
    state.emit("CARD_DELETED", json!({ "id": card.id }), origin);
    Ok(card)
}

/// Resolve a full id or a unique short prefix (the 8-char id shown in the UI).
pub async fn resolve_card(state: &AppState, key: &str) -> AppResult<Card> {
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

pub async fn agent_action(state: &AppState, input: AgentAction, origin: Option<String>) -> AppResult<Card> {
    let mut card = resolve_card(state, &input.card_id).await?;
    let from_column = card.column_id.clone();

    let target = match input.action {
        AgentVerb::Start => {
            card.assignee = Assignee::Agent.as_str().into();
            Some(if card.tag == "sell" { "sell" } else { "build" })
        }
        AgentVerb::Complete => Some(DONE_COLUMN),
        AgentVerb::Note => None,
    };

    if let Some(col) = target {
        card.status = status_for_column(col).into();
        if col != card.column_id {
            card.column_id = col.into();
            card.position = top_position(state, col).await?;
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
    save_card(state, &card).await?;

    let kind = if card.column_id != from_column { "CARD_MOVED" } else { "CARD_UPDATED" };
    state.emit(kind, json!({ "card": card }), origin);
    Ok(card)
}

pub async fn list_tasks(
    state: &AppState,
    assignee: Option<&str>,
    status: Option<&str>,
    repo: Option<&str>,
    labels: Option<&[String]>,
    limit: usize,
) -> AppResult<Vec<Card>> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!("SELECT {CARD_COLS} FROM cards"));
    let mut has_where = false;
    let mut push_cond = |qb: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>| {
        qb.push(if has_where { " AND " } else { " WHERE " });
        has_where = true;
    };

    if let Some(a) = assignee.map(str::trim).filter(|s| !s.is_empty()) {
        push_cond(&mut qb);
        qb.push("LOWER(assignee) = LOWER(");
        qb.push_bind(a);
        qb.push(")");
    }

    if let Some(r) = repo.map(str::trim).filter(|s| !s.is_empty()) {
        push_cond(&mut qb);
        qb.push("LOWER(COALESCE(repo, '')) = LOWER(");
        qb.push_bind(r);
        qb.push(")");
    }

    if let Some(raw_status) = status.map(str::trim).filter(|s| !s.is_empty()) {
        let lower = raw_status.to_ascii_lowercase();
        let (col_filter, status_filter) = match lower.as_str() {
            "backlog" | "build" | "sell" => (Some(lower.as_str()), None),
            "todo" => (None, Some("todo")),
            "in_progress" | "in-progress" | "in progress" => (None, Some("in_progress")),
            "review" => (None, Some("review")),
            "done" => (None, Some("done")),
            _ => {
                return Err(AppError::BadRequest(format!(
                    "unknown status '{raw_status}' — use todo, in_progress, review, done, backlog, build, or sell"
                )));
            }
        };
        push_cond(&mut qb);
        if let Some(col) = col_filter {
            qb.push("column_id = ");
            qb.push_bind(col.to_string());
        } else if let Some(st) = status_filter {
            qb.push("status = ");
            qb.push_bind(st.to_string());
        }
    }

    let wanted_labels = labels
        .map(|l| normalize_label_list(l.iter().map(String::as_str)))
        .unwrap_or_default();

    qb.push(" ORDER BY priority DESC, position ASC, updated_at ASC");
    if wanted_labels.is_empty() {
        qb.push(" LIMIT ");
        qb.push_bind(limit as i64);
    }

    let mut cards = qb.build_query_as::<Card>().fetch_all(&state.pool).await?;
    if !wanted_labels.is_empty() {
        cards.retain(|c| wanted_labels.iter().all(|w| c.labels.contains(w)));
        cards.truncate(limit);
    }
    Ok(cards)
}

pub async fn list_comments(state: &AppState, card_id: &str) -> AppResult<Vec<CardComment>> {
    let comments = sqlx::query_as::<_, CardComment>(
        "SELECT id, card_id, author, comment, created_at
         FROM card_comments
         WHERE card_id = ?
         ORDER BY created_at ASC, rowid ASC",
    )
    .bind(card_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(comments)
}

pub async fn add_card_comment(
    state: &AppState,
    card_id: &str,
    author: Option<&str>,
    comment: &str,
    origin: Option<String>,
) -> AppResult<(Card, CardComment)> {
    let comment_text = comment.trim();
    if comment_text.is_empty() {
        return Err(AppError::BadRequest("comment must not be empty".into()));
    }
    if comment_text.chars().count() > 20_000 {
        return Err(AppError::BadRequest("comment is too long (max 20000 chars)".into()));
    }
    let author_text = match author.map(str::trim).filter(|s| !s.is_empty()) {
        Some(a) => clean_assignee(a)?,
        None => "agent".into(),
    };

    let mut card = resolve_card(state, card_id).await?;
    let created_at = now();
    let comment_row = CardComment {
        id: uuid::Uuid::new_v4().to_string(),
        card_id: card.id.clone(),
        author: author_text,
        comment: comment_text.to_string(),
        created_at: created_at.clone(),
    };

    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let entry = format!("💬 **{}** · _{stamp}_\n\n{}", comment_row.author, comment_row.comment);
    card.description = Some(match card.description.take() {
        Some(d) if !d.trim().is_empty() => format!("{d}\n\n---\n\n{entry}"),
        _ => entry,
    });
    card.updated_at = created_at;

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO card_comments (id, card_id, author, comment, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&comment_row.id)
    .bind(&comment_row.card_id)
    .bind(&comment_row.author)
    .bind(&comment_row.comment)
    .bind(&comment_row.created_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE cards SET description = ?, updated_at = ? WHERE id = ?")
        .bind(&card.description)
        .bind(&card.updated_at)
        .bind(&card.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    state.emit("CARD_UPDATED", json!({ "card": card }), origin);
    Ok((card, comment_row))
}
