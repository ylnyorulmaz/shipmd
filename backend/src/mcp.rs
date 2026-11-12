//! Embedded Model Context Protocol server (JSON-RPC 2.0).
//!
//! Transports:
//! - `POST /mcp`: Streamable HTTP (MCP 2025-03-26 / 2025-06-18), stateless, JSON responses.
//! - `GET /mcp/sse` + `POST /mcp/messages?sessionId=…`: legacy HTTP+SSE (MCP 2024-11-05).
//!
//! Tools reuse `crate::service` (SQLite + WebSocket broadcast) and `crate::telegram` notifications,
//! so an agent's change animates on the web board and pings Telegram like any other change.
//!
//! Auth: `MCP_API_KEY` set ⇒ `Authorization: Bearer <key>` required. Unset ⇒ loopback-only, and
//! browser requests from non-localhost origins are rejected (DNS-rebinding guard).

use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{ConnectInfo, Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    db::COLUMNS,
    error::AppError,
    models::{AgentAction, AgentVerb, Assignee, Card, CreateCard, PatchCard, Tag},
    service::{self, DONE_COLUMN},
    telegram, AppState,
};

const ORIGIN: &str = "mcp";
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Done cards returned by get_board without a column filter (newest first).
const DONE_LIMIT: usize = 10;

const INSTRUCTIONS: &str = "Ship Board is the Kanban board you share with a solo founder (columns: Backlog, Build, Sell, Done). \
Workflow: call pick_next_agent_task to claim work, log progress / commit hashes / PR links with update_card_notes, \
and move_card to Done when finished. Every change appears live on the founder's board and phone.";

// ---------------------------------------------------------------------------
// Router & state
// ---------------------------------------------------------------------------

type Sessions = Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>;

#[derive(Clone)]
struct McpCtx {
    app: AppState,
    api_key: Option<Arc<str>>,
    sessions: Sessions,
}

/// Router with its own state, ready to `.merge()` into the main app.
/// Requires serving with `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn router(app: AppState) -> Router {
    let api_key = std::env::var("MCP_API_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .map(Arc::from);
    match &api_key {
        Some(_) => tracing::info!("MCP server on /mcp (bearer auth enabled)"),
        None => tracing::warn!("MCP_API_KEY not set — MCP server on /mcp accepts unauthenticated localhost requests only"),
    }
    let ctx = McpCtx { app, api_key, sessions: Arc::default() };

    Router::new()
        .route("/mcp", post(streamable_post).get(no_server_stream).delete(no_server_stream))
        .route("/mcp/sse", get(sse_connect))
        .route("/mcp/messages", post(sse_message))
        .layer(middleware::from_fn_with_state(ctx.clone(), auth))
        .with_state(ctx)
}

// ---------------------------------------------------------------------------
// Auth guard
// ---------------------------------------------------------------------------

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn is_local_origin(origin: &str) -> bool {
    let host = origin.split("://").nth(1).unwrap_or(origin);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.rsplit_once(':').map_or(host, |(h, port)| if port.chars().all(|c| c.is_ascii_digit()) { h } else { host });
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

async fn auth(
    State(ctx): State<McpCtx>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    if let Some(key) = &ctx.api_key {
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .unwrap_or_default();
        if !constant_time_eq(presented.as_bytes(), key.as_bytes()) {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                Json(json!({ "error": "missing or invalid MCP bearer token" })),
            )
                .into_response();
        }
    } else {
        if !peer.ip().to_canonical().is_loopback() {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "MCP_API_KEY is not set: only localhost may use /mcp" })),
            )
                .into_response();
        }
        if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
            if !is_local_origin(origin) {
                return (StatusCode::FORBIDDEN, Json(json!({ "error": "origin not allowed" }))).into_response();
            }
        }
    }
    next.run(req).await
}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

/// Streamable HTTP: one JSON-RPC message (or a batch) per POST.
async fn streamable_post(State(ctx): State<McpCtx>, body: Bytes) -> Response {
    match process_body(&ctx, &body).await {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(), // only notifications / responses
    }
}

/// We never push server-initiated messages, so there is no standalone GET stream.
async fn no_server_stream() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")])
}

/// Removes the SSE session when the client disconnects (the stream is dropped).
struct SessionGuard {
    id: String,
    sessions: Sessions,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.sessions.lock().unwrap().remove(&self.id);
        tracing::debug!(session = %self.id, "MCP SSE session closed");
    }
}

/// Legacy transport: open the stream, tell the client where to POST.
async fn sse_connect(State(ctx): State<McpCtx>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<String>(64);
    ctx.sessions.lock().unwrap().insert(id.clone(), tx);
    tracing::debug!(session = %id, "MCP SSE session opened");

    let endpoint = Event::default().event("endpoint").data(format!("/mcp/messages?sessionId={id}"));
    let guard = SessionGuard { id, sessions: ctx.sessions.clone() };
    let messages = stream::unfold((rx, guard), |(mut rx, guard)| async move {
        let msg = rx.recv().await?;
        Some((Ok(Event::default().event("message").data(msg)), (rx, guard)))
    });

    Sse::new(stream::once(async { Ok(endpoint) }).chain(messages))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

#[derive(Deserialize)]
struct SessionQuery {
    #[serde(rename = "sessionId")]
    session_id: String,
}

/// Legacy transport: accept a message, deliver the reply over the session's SSE stream.
async fn sse_message(State(ctx): State<McpCtx>, Query(q): Query<SessionQuery>, body: Bytes) -> Response {
    let Some(tx) = ctx.sessions.lock().unwrap().get(&q.session_id).cloned() else {
        return (StatusCode::NOT_FOUND, "unknown or expired sessionId").into_response();
    };
    if let Some(reply) = process_body(&ctx, &body).await {
        if tx.send(reply.to_string()).await.is_err() {
            return (StatusCode::GONE, "session closed").into_response();
        }
    }
    StatusCode::ACCEPTED.into_response()
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0
// ---------------------------------------------------------------------------

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

async fn process_body(ctx: &McpCtx, body: &[u8]) -> Option<Value> {
    let parsed: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return Some(rpc_error(&Value::Null, -32700, format!("parse error: {e}"))),
    };
    match parsed {
        Value::Array(batch) if batch.is_empty() => Some(rpc_error(&Value::Null, -32600, "empty batch")),
        Value::Array(batch) => {
            let mut replies = Vec::new();
            for msg in batch {
                replies.extend(handle_message(ctx, msg).await);
            }
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        msg => handle_message(ctx, msg).await,
    }
}

/// Returns `None` for notifications and client responses (nothing to send back).
async fn handle_message(ctx: &McpCtx, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // A response to a server request (we never send any) or garbage.
        return match id {
            Some(id) if msg.get("result").is_none() && msg.get("error").is_none() => {
                Some(rpc_error(&id, -32600, "invalid request: missing method"))
            }
            _ => None,
        };
    };
    if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return id.map(|id| rpc_error(&id, -32600, "invalid request: jsonrpc must be \"2.0\""));
    }
    let Some(id) = id else {
        // Notification (notifications/initialized, notifications/cancelled, …).
        tracing::debug!(method, "MCP notification");
        return None;
    };
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    Some(match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = SUPPORTED_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&SUPPORTED_VERSIONS[0]);
            let client = params.pointer("/clientInfo/name").and_then(Value::as_str).unwrap_or("unknown");
            tracing::info!(client, version, "MCP client initialized");
            rpc_result(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "ship-board", "title": "Ship Board", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(&id, json!({})),
        "tools/list" => rpc_result(&id, json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(&id, -32602, "tools/call requires params.name"));
            };
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call_tool(&ctx.app, name, &args).await {
                ToolOutcome::Unknown => rpc_error(&id, -32602, format!("unknown tool: {name}")),
                ToolOutcome::Ok(text) => rpc_result(&id, json!({ "content": [{ "type": "text", "text": text }], "isError": false })),
                // Tool failures are results (not protocol errors) so the model can read and self-correct.
                ToolOutcome::Err(text) => rpc_result(&id, json!({ "content": [{ "type": "text", "text": text }], "isError": true })),
            }
        }
        // Clients may probe these; we have none.
        "resources/list" => rpc_result(&id, json!({ "resources": [] })),
        "prompts/list" => rpc_result(&id, json!({ "prompts": [] })),
        other => rpc_error(&id, -32601, format!("method not found: {other}")),
    })
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    let column_enum = json!(["Backlog", "Build", "Sell", "Done"]);
    let card_id = json!({
        "type": "string",
        "description": "Card id: the full UUID or its first 8 characters (the short id shown on the board, e.g. \"fc4d52cc\")."
    });
    json!([
        {
            "name": "get_board",
            "title": "Get board",
            "description": "Fetch the current Kanban board: columns Backlog, Build, Sell, Done (in that order) with their cards. \
Each card has id, short_id, title, tag (build|sell|ops), assignee (human|agent), Markdown description and updated_at. \
Cards are in priority order (top first). Without a filter, Done is limited to the 10 most recent cards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "column": { "type": "string", "enum": column_enum, "description": "Only return this column." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "create_card",
            "title": "Create card",
            "description": "Create a new task card. It appears live on the founder's board. Returns the created card.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "minLength": 1, "maxLength": 500, "description": "Short, imperative task title." },
                    "description": { "type": "string", "description": "Optional Markdown details (acceptance criteria, links, checklists)." },
                    "column": { "type": "string", "enum": column_enum, "default": "Backlog", "description": "Column to create the card in." },
                    "tag": { "type": "string", "enum": ["build", "sell", "ops"], "default": "build", "description": "build = product/engineering, sell = marketing/sales, ops = operations." },
                    "assignee": { "type": "string", "enum": ["human", "agent"], "default": "agent", "description": "Who should do it: the AI agent or the human founder." }
                },
                "required": ["title"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "move_card",
            "title": "Move card",
            "description": "Move a card to another column, e.g. Backlog → Build when starting work, Build → Done when finished. \
Moving to Done notifies the founder on Telegram. Returns the updated card.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": card_id,
                    "target_column": { "type": "string", "enum": column_enum, "description": "Destination column." }
                },
                "required": ["card_id", "target_column"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "update_card_notes",
            "title": "Update card notes",
            "description": "Record progress on a card: notes, commit hashes, PR links, implementation details. \
With append=true (default) the note is added as a timestamped entry below the existing Markdown description; \
with append=false it replaces the whole description. Returns the updated card.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "card_id": card_id,
                    "note": { "type": "string", "minLength": 1, "description": "Markdown text to record." },
                    "append": { "type": "boolean", "default": true, "description": "Append (true) or replace the description (false)." }
                },
                "required": ["card_id", "note"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "pick_next_agent_task",
            "title": "Pick next agent task",
            "description": "Claim the next task for the coding agent and return its full details. \
Priority: an agent-assigned card already in Build (unfinished work) first, otherwise the top agent-assigned card in Backlog, \
which is moved to Build and announced to the founder on Telegram. Returns a message saying so when no agent work is queued. \
When finished, call update_card_notes with what you did, then move_card to Done.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        }
    ])
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

enum ToolOutcome {
    Ok(String),
    Err(String),
    Unknown,
}

type ToolResult = Result<Value, String>;

async fn call_tool(app: &AppState, name: &str, args: &Value) -> ToolOutcome {
    let result = match name {
        "get_board" => get_board(app, args).await,
        "create_card" => create_card(app, args).await,
        "move_card" => move_card(app, args).await,
        "update_card_notes" => update_card_notes(app, args).await,
        "pick_next_agent_task" => pick_next_agent_task(app).await,
        _ => return ToolOutcome::Unknown,
    };
    tracing::info!(tool = name, ok = result.is_ok(), "MCP tool call");
    match result {
        Ok(v) => ToolOutcome::Ok(serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => ToolOutcome::Err(e),
    }
}

fn app_err(e: AppError) -> String {
    match e {
        AppError::NotFound(m) | AppError::BadRequest(m) => m,
        AppError::Db(_) => "internal database error".into(),
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Card ids may arrive as strings or (from sloppy clients) numbers.
fn card_id_arg(args: &Value) -> Result<String, String> {
    match args.get("card_id") {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err("card_id is required (full UUID or 8-char short id)".into()),
    }
}

/// Accepts a column title or id, case-insensitively ("Backlog", "backlog").
fn column_arg(raw: &str) -> Result<&'static str, String> {
    COLUMNS
        .iter()
        .find(|(id, title)| raw.eq_ignore_ascii_case(id) || raw.eq_ignore_ascii_case(title))
        .map(|(id, _)| *id)
        .ok_or_else(|| format!("unknown column '{raw}' — use one of Backlog, Build, Sell, Done"))
}

fn column_title(id: &str) -> &str {
    COLUMNS.iter().find(|(cid, _)| *cid == id).map_or(id, |(_, t)| t)
}

fn card_json(card: &Card) -> Value {
    json!({
        "id": card.id,
        "short_id": &card.id[..8.min(card.id.len())],
        "column": column_title(&card.column_id),
        "title": card.title,
        "tag": card.tag,
        "assignee": card.assignee,
        "description": card.description,
        "updated_at": card.updated_at,
    })
}

fn notify(app: &AppState, text: String, card: &Card) {
    if let Some(bot) = app.telegram.clone() {
        tokio::spawn(telegram::notify_card(bot, text, card.clone()));
    }
}

async fn get_board(app: &AppState, args: &Value) -> ToolResult {
    let filter = str_arg(args, "column").map(column_arg).transpose()?;
    let board = service::board(app).await.map_err(app_err)?;
    let columns: Vec<Value> = board
        .columns
        .iter()
        .filter(|c| filter.is_none_or(|f| f == c.column.id))
        .map(|c| {
            let limit = if filter.is_none() && c.column.id == DONE_COLUMN { DONE_LIMIT } else { usize::MAX };
            json!({
                "column": c.column.title,
                "card_count": c.cards.len(),
                "cards": c.cards.iter().take(limit).map(card_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "board": board.title, "columns": columns }))
}

async fn create_card(app: &AppState, args: &Value) -> ToolResult {
    let title = str_arg(args, "title").ok_or("title is required")?;
    let column = column_arg(str_arg(args, "column").unwrap_or("Backlog"))?;
    let tag = match str_arg(args, "tag").map(str::to_ascii_lowercase).as_deref() {
        None | Some("build") => Tag::Build,
        Some("sell") => Tag::Sell,
        Some("ops") => Tag::Ops,
        Some(other) => return Err(format!("invalid tag '{other}' — use build, sell or ops")),
    };
    let assignee = match str_arg(args, "assignee").map(str::to_ascii_lowercase).as_deref() {
        None | Some("agent") => Assignee::Agent,
        Some("human") => Assignee::Human,
        Some(other) => return Err(format!("invalid assignee '{other}' — use human or agent")),
    };
    let card = service::create_card(
        app,
        CreateCard {
            id: None,
            column_id: column.into(),
            title: title.into(),
            description: str_arg(args, "description").map(Into::into),
            tag: Some(tag),
            assignee: Some(assignee),
            position: None,
        },
        Some(ORIGIN.into()),
    )
    .await
    .map_err(app_err)?;
    Ok(json!({ "created": card_json(&card) }))
}

async fn move_card(app: &AppState, args: &Value) -> ToolResult {
    let key = card_id_arg(args)?;
    let target = column_arg(str_arg(args, "target_column").ok_or("target_column is required")?)?;
    let before = service::resolve_card(app, &key).await.map_err(app_err)?;
    if before.column_id == target {
        return Ok(json!({ "unchanged": true, "message": format!("Card is already in {}", column_title(target)), "card": card_json(&before) }));
    }
    let patch = PatchCard { column_id: Some(target.into()), ..Default::default() };
    let card = service::update_card(app, &before.id, patch, Some(ORIGIN.into())).await.map_err(app_err)?;
    if target == DONE_COLUMN {
        notify(app, format!("🤖 MCP Agent moved \"<b>{}</b>\" to Done", telegram::esc(&card.title)), &card);
    }
    Ok(json!({ "moved_from": column_title(&before.column_id), "card": card_json(&card) }))
}

async fn update_card_notes(app: &AppState, args: &Value) -> ToolResult {
    let key = card_id_arg(args)?;
    let note = str_arg(args, "note").ok_or("note is required")?;
    let append = args.get("append").and_then(Value::as_bool).unwrap_or(true);

    let card = if append {
        // Same timestamped "🤖 Agent noted" entry format as POST /api/agent/action.
        let action = AgentAction { card_id: key, action: AgentVerb::Note, note: Some(note.into()) };
        service::agent_action(app, action, Some(ORIGIN.into())).await
    } else {
        let id = service::resolve_card(app, &key).await.map_err(app_err)?.id;
        let patch = PatchCard { description: Some(note.into()), ..Default::default() };
        service::update_card(app, &id, patch, Some(ORIGIN.into())).await
    }
    .map_err(app_err)?;
    Ok(json!({ "updated": card_json(&card) }))
}

async fn pick_next_agent_task(app: &AppState) -> ToolResult {
    let board = service::board(app).await.map_err(app_err)?;
    let next_in = |col: &str| {
        board
            .columns
            .iter()
            .find(|c| c.column.id == col)
            .and_then(|c| c.cards.iter().find(|card| card.assignee == "agent"))
            .cloned()
    };

    if let Some(card) = next_in("build") {
        return Ok(json!({
            "status": "resumed",
            "message": "You already have this task in progress in Build — finish it first.",
            "task": card_json(&card),
            "next_steps": "Record progress with update_card_notes, then move_card to Done.",
        }));
    }
    let Some(card) = next_in("backlog") else {
        return Ok(json!({ "status": "idle", "message": "No agent-assigned cards in Backlog or Build. Nothing to pick up." }));
    };

    let patch = PatchCard { column_id: Some("build".into()), ..Default::default() };
    let card = service::update_card(app, &card.id, patch, Some(ORIGIN.into())).await.map_err(app_err)?;
    notify(app, format!("🤖 Agent started working on: \"<b>{}</b>\"", telegram::esc(&card.title)), &card);
    Ok(json!({
        "status": "started",
        "message": "Moved from Backlog to Build. This task is yours now.",
        "task": card_json(&card),
        "next_steps": "Record progress with update_card_notes, then move_card to Done.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_accept_titles_and_ids() {
        assert_eq!(column_arg("Backlog").unwrap(), "backlog");
        assert_eq!(column_arg("done").unwrap(), "done");
        assert_eq!(column_arg("SELL").unwrap(), "sell");
        assert!(column_arg("Doing").is_err());
    }

    #[test]
    fn local_origins() {
        assert!(is_local_origin("http://localhost:5173"));
        assert!(is_local_origin("http://127.0.0.1"));
        assert!(is_local_origin("http://[::1]:3001"));
        assert!(!is_local_origin("https://evil.example"));
        assert!(!is_local_origin("http://localhost.evil.example"));
    }

    #[test]
    fn tool_schemas_are_objects() {
        let tools = tool_definitions();
        let tools = tools.as_array().unwrap();
        assert_eq!(tools.len(), 5);
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        }
    }
}
