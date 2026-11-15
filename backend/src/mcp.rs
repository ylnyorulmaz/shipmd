//! Embedded Model Context Protocol server (JSON-RPC 2.0).
//!
//! Transports:
//! - `POST /mcp`: Streamable HTTP (MCP 2025-03-26 / 2025-06-18), stateless, JSON responses.
//! - `GET /mcp/sse` + `POST /mcp/messages?sessionId=…`: legacy HTTP+SSE (MCP 2024-11-05).
//! - Token-in-path aliases: `/mcp/t/{token}`, `/mcp/t/{token}/sse`, `/mcp/t/{token}/messages`.
//!
//! Tools reuse `crate::service` (SQLite + WebSocket broadcast) and `crate::telegram` notifications,
//! so an agent's change animates on the web board and pings Telegram like any other change.
//!
//! Auth: `SHIPMD_MCP_TOKEN` (or fallback `MCP_API_KEY`) sets the required token, accepted via:
//! - `Authorization: Bearer <token>` header,
//! - `?token=<token>` URL query parameter, or
//! - `/mcp/t/<token>` path prefix (for web MCP clients that cannot send custom headers).
//! When unset, a warning is logged and all requests to `/mcp` are rejected with `401 Unauthorized`.

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{Query, Request, State},
    http::{header, HeaderMap, StatusCode, Uri},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Extension, Json, Router,
};
use futures_util::{stream, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    db::COLUMNS,
    error::AppError,
    models::{
        AgentAction, AgentVerb, AssigneeKind, Card, CreateAssignee, CreateCard, CreateProject,
        PatchCard, ProjectLink, Tag,
    },
    service::{self, DONE_COLUMN},
    telegram, AppState,
};

const ORIGIN: &str = "mcp";
const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Done cards returned by get_board without a column filter (newest first).
const DONE_LIMIT: usize = 10;
const DEFAULT_LIST_LIMIT: usize = 50;
const MAX_LIST_LIMIT: usize = 200;

const INSTRUCTIONS: &str = "Ship Board is the Kanban board you share with a solo founder (columns: Backlog, Build, Sell, Done). \
Workflow: call list_projects / get_project_details and list_assignees to understand workspace context, links, and team roles; \
call list_tasks or pick_next_agent_task to claim work, inspect full details with get_task_details, \
log progress / commit hashes / PR links with add_task_comment or update_card_notes, and call update_task_status \
(in_progress, review, done) as you progress. Every change appears live on the founder's board and phone.";

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

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Router with its own state, ready to `.merge()` into the main app.
pub fn router(app: AppState) -> Router {
    let api_key = match non_empty_env("SHIPMD_MCP_TOKEN") {
        Some(token) => {
            tracing::info!("MCP server on /mcp (bearer auth enabled via SHIPMD_MCP_TOKEN)");
            Some(Arc::from(token))
        }
        None => match non_empty_env("MCP_API_KEY") {
            Some(token) => {
                tracing::warn!(
                    "MCP server using legacy MCP_API_KEY — please migrate to SHIPMD_MCP_TOKEN"
                );
                Some(Arc::from(token))
            }
            None => {
                tracing::warn!(
                    "SHIPMD_MCP_TOKEN is not set — /mcp will reject all unauthorized requests with 401 Unauthorized"
                );
                None
            }
        },
    };
    router_with_token(app, api_key)
}

fn router_with_token(app: AppState, api_key: Option<Arc<str>>) -> Router {
    let ctx = McpCtx { app, api_key, sessions: Arc::default() };

    Router::new()
        .route("/mcp", post(streamable_post).get(no_server_stream).delete(no_server_stream))
        .route("/mcp/sse", get(sse_connect))
        .route("/mcp/messages", post(sse_message))
        .route(
            "/mcp/t/{token}",
            post(streamable_post).get(no_server_stream).delete(no_server_stream),
        )
        .route(
            "/mcp/t/{token}/",
            post(streamable_post).get(no_server_stream).delete(no_server_stream),
        )
        .route("/mcp/t/{token}/sse", get(sse_connect))
        .route("/mcp/t/{token}/messages", post(sse_message))
        .layer(middleware::from_fn_with_state(ctx.clone(), auth))
        .with_state(ctx)
}

// ---------------------------------------------------------------------------
// Auth guard
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum AuthSource {
    Bearer,
    QueryToken(String),
    PathToken(String),
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({ "error": msg })),
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, rest) = raw.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| rest.trim())
}

fn raw_query_param(uri: &Uri, name: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

fn query_token(uri: &Uri) -> Result<Option<(String, String)>, ()> {
    let Query(parsed) = Query::<TokenQuery>::try_from_uri(uri).map_err(|_| ())?;
    Ok(parsed.token.map(|decoded| {
        let raw = raw_query_param(uri, "token").unwrap_or_else(|| decoded.clone());
        (raw, decoded)
    }))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode_path(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes.get(i + 1).copied().and_then(hex_val)?;
            let lo = bytes.get(i + 2).copied().and_then(hex_val)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn path_token(path: &str) -> Result<Option<(&str, String)>, ()> {
    let Some(rest) = path.strip_prefix("/mcp/t/") else {
        return Ok(None);
    };
    let raw = rest.split('/').next().unwrap_or("");
    let decoded = percent_decode_path(raw).ok_or(())?;
    Ok(Some((raw, decoded)))
}

async fn auth(State(ctx): State<McpCtx>, mut req: Request, next: Next) -> Response {
    let Some(key) = &ctx.api_key else {
        return unauthorized("SHIPMD_MCP_TOKEN is not configured on the server");
    };

    let bearer = bearer_token(req.headers());
    let Ok(query) = query_token(req.uri()) else {
        return unauthorized("missing or invalid MCP bearer token");
    };
    let Ok(path) = path_token(req.uri().path()) else {
        return unauthorized("missing or invalid MCP bearer token");
    };

    if bearer.is_none() && query.is_none() && path.is_none() {
        return unauthorized("missing or invalid MCP bearer token");
    }

    let matches_key = |candidate: &str| crate::auth::constant_time_eq(candidate.as_bytes(), key.as_bytes());
    if bearer.is_some_and(|t| !matches_key(t))
        || query.as_ref().is_some_and(|(_, t)| !matches_key(t))
        || path.as_ref().is_some_and(|(_, t)| !matches_key(t))
    {
        return unauthorized("missing or invalid MCP bearer token");
    }

    let source = if bearer.is_some() {
        AuthSource::Bearer
    } else if let Some((raw, _)) = path {
        AuthSource::PathToken(raw.to_string())
    } else if let Some((raw, _)) = query {
        AuthSource::QueryToken(raw)
    } else {
        AuthSource::Bearer
    };
    req.extensions_mut().insert(source);

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
async fn sse_connect(
    State(ctx): State<McpCtx>,
    auth_source: Option<Extension<AuthSource>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<String>(64);
    ctx.sessions.lock().unwrap().insert(id.clone(), tx);
    tracing::debug!(session = %id, "MCP SSE session opened");

    let endpoint_url = match auth_source.map(|Extension(s)| s) {
        Some(AuthSource::PathToken(raw)) => format!("/mcp/t/{raw}/messages?sessionId={id}"),
        Some(AuthSource::QueryToken(raw)) => format!("/mcp/messages?sessionId={id}&token={raw}"),
        _ => format!("/mcp/messages?sessionId={id}"),
    };
    let endpoint = Event::default().event("endpoint").data(endpoint_url);
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
        "notifications/initialized" => rpc_result(&id, json!({})),
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
    let status_enum = json!(["todo", "in_progress", "review", "done", "backlog", "build", "sell"]);
    let card_id = json!({
        "type": "string",
        "description": "Card id: the full UUID or its first 8 characters (the short id shown on the board, e.g. \"fc4d52cc\")."
    });
    let task_id = json!({
        "type": "string",
        "description": "Task id: the full UUID or a unique prefix of at least 4 hex characters (e.g. the 8-char short id)."
    });
    json!([
        {
            "name": "list_tasks",
            "title": "List tasks",
            "description": "List tasks filtered by optional assignee (e.g. \"antigravity\", \"kimi\", \"hermes\", \"human\", \"agent\"), \
status (todo, in_progress, review, done, or column names backlog, build, sell), repo (e.g. \"shipmd\"), and/or label(s). \
Matching tasks are returned sorted by priority descending (highest priority first), then board position.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "assignee": { "type": "string", "description": "Filter by assignee (e.g. \"antigravity\", \"kimi\", \"hermes\", \"human\", \"agent\"). Case-insensitive." },
                    "status": { "type": "string", "enum": status_enum, "description": "Filter by status (todo, in_progress, review, done, backlog, build, sell)." },
                    "repo": { "type": "string", "description": "Filter by repository/project name (e.g. \"shipmd\", \"ceviz-ai\"). Case-insensitive." },
                    "label": { "type": "string", "description": "Filter by a single label (with or without leading #, e.g. \"frontend\")." },
                    "labels": { "type": "array", "items": { "type": "string" }, "description": "Filter to tasks containing all specified labels." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIST_LIMIT, "default": DEFAULT_LIST_LIMIT, "description": "Maximum number of tasks to return." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "get_task_details",
            "title": "Get task details",
            "description": "Fetch full details for a task by task_id: full Markdown description, parsed checklist substeps, comments, current status, assignee, repo, labels, priority, and pr_url.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": task_id
                },
                "required": ["task_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "update_task_status",
            "title": "Update task status",
            "description": "Update a task's status (e.g. todo, in_progress, review, done) and optionally reassign it, update repo, labels, priority, or attach a PR URL. \
Moves the card to the matching board column and broadcasts the change live; moving to done also notifies Telegram.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": task_id,
                    "status": { "type": "string", "enum": status_enum, "description": "Target status (e.g. todo, in_progress, review, done)." },
                    "assignee": { "type": "string", "minLength": 1, "maxLength": 64, "description": "Optional new assignee (e.g. \"antigravity\", \"kimi\", \"hermes\", \"human\", \"agent\")." },
                    "repo": { "type": "string", "maxLength": 64, "description": "Optional repository/project name (empty string clears)." },
                    "labels": { "type": "array", "items": { "type": "string" }, "description": "Optional list of labels (replaces existing labels; empty array clears)." },
                    "priority": { "type": "integer", "description": "Optional integer priority (e.g. 0=Low, 5=Normal, 8=High, 10=Urgent)." },
                    "pr_url": { "type": "string", "maxLength": 2048, "description": "Optional pull request URL associated with the task." }
                },
                "required": ["task_id", "status"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "add_task_comment",
            "title": "Add task comment",
            "description": "Add a progress note, error report, or PR link comment to a task with author attribution. \
Stored in task comments and reflected live on the board.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": task_id,
                    "comment": { "type": "string", "minLength": 1, "description": "Comment body (progress note, error report, or PR link)." },
                    "author": { "type": "string", "minLength": 1, "maxLength": 64, "default": "agent", "description": "Author name (e.g. \"hermes\", \"antigravity\", \"human\")." }
                },
                "required": ["task_id", "comment"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "get_board",
            "title": "Get board",
            "description": "Fetch the current Kanban board: columns Backlog, Build, Sell, Done (in that order) with their cards. \
Each card has id, short_id, title, tag (build|sell|ops), assignee, status, priority, pr_url, Markdown description and updated_at. \
Cards are in board position order (top first). Without a filter, Done is limited to the 10 most recent cards.",
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
                    "assignee": { "type": "string", "minLength": 1, "maxLength": 64, "default": "agent", "description": "Who should do it (e.g. \"agent\", \"antigravity\", \"kimi\", \"hermes\", \"human\")." },
                    "repo": { "type": "string", "maxLength": 64, "description": "Optional repository/project name (e.g. \"shipmd\", \"ceviz-ai\", \"antisocial\", \"kahvefalim\")." },
                    "labels": { "type": "array", "items": { "type": "string" }, "description": "Optional list of labels (e.g. [\"frontend\", \"bug\"])." },
                    "priority": { "type": "integer", "default": 0, "description": "Optional integer priority (higher values sort first)." }
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
        },
        {
            "name": "list_projects",
            "title": "List projects",
            "description": "List all defined projects/workspaces with their descriptions and resource link lists (GitHub, Live, Staging, CRM, Ads, etc.).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "get_project_details",
            "title": "Get project details",
            "description": "Fetch the full description and resource links for a single project by id (slug) or name (case-insensitive).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": {
                        "type": "string",
                        "description": "Project id (slug, e.g. \"shipmd\") or name (e.g. \"ShipMD\"). Case-insensitive."
                    }
                },
                "required": ["project"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "list_assignees",
            "title": "List assignees",
            "description": "List all human team members and AI agents with their kind (human|ai), role descriptions, and badges.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["human", "ai"],
                        "description": "Optional filter by member kind (\"human\" or \"ai\")."
                    }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "create_project",
            "title": "Create project",
            "description": "Create a new project/workspace with a name, optional short description/context, and optional resource links.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "maxLength": 64, "description": "Optional slug id (auto-generated from name if omitted)." },
                    "name": { "type": "string", "minLength": 1, "maxLength": 80, "description": "Project display name (e.g. \"CodeSight\", \"Q4 Marketing\")." },
                    "description": { "type": "string", "maxLength": 1000, "description": "Short project/context summary." },
                    "links": {
                        "type": "array",
                        "description": "Resource links such as GitHub, Live, Staging, CRM, Ads Panel.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string", "minLength": 1, "maxLength": 40 },
                                "url": { "type": "string", "minLength": 1, "maxLength": 2048 }
                            },
                            "required": ["label", "url"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "create_assignee",
            "title": "Create assignee",
            "description": "Register a new human team member or AI agent with their kind (human|ai) and role/capability description.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "maxLength": 64, "description": "Optional slug id (auto-generated from name if omitted)." },
                    "name": { "type": "string", "minLength": 1, "maxLength": 80, "description": "Member or agent display name (e.g. \"Yalın\", \"ChatGPT\")." },
                    "kind": { "type": "string", "enum": ["human", "ai"], "description": "\"human\" for a person or \"ai\" for an AI agent." },
                    "role_description": { "type": "string", "maxLength": 1000, "description": "Short role and capability summary." },
                    "avatar_badge": { "type": "string", "maxLength": 32, "description": "Optional icon/emoji badge." }
                },
                "required": ["name", "kind"],
                "additionalProperties": false
            },
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
        "list_tasks" => list_tasks(app, args).await,
        "get_task_details" => get_task_details(app, args).await,
        "update_task_status" => update_task_status(app, args).await,
        "add_task_comment" => add_task_comment(app, args).await,
        "get_board" => get_board(app, args).await,
        "create_card" => create_card(app, args).await,
        "move_card" => move_card(app, args).await,
        "update_card_notes" => update_card_notes(app, args).await,
        "pick_next_agent_task" => pick_next_agent_task(app).await,
        "list_projects" => list_projects(app).await,
        "get_project_details" => get_project_details(app, args).await,
        "list_assignees" => list_assignees(app, args).await,
        "create_project" => create_project(app, args).await,
        "create_assignee" => create_assignee(app, args).await,
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

/// Accepts `key` (or optional fallback key) as string or number.
fn id_arg(args: &Value, primary: &str, fallback: Option<&str>) -> Result<String, String> {
    let val = args.get(primary).or_else(|| fallback.and_then(|k| args.get(k)));
    match val {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(format!("{primary} is required (full UUID or unique prefix)")),
    }
}

/// Card ids may arrive as strings or (from sloppy clients) numbers.
fn card_id_arg(args: &Value) -> Result<String, String> {
    id_arg(args, "card_id", Some("task_id"))
}

fn task_id_arg(args: &Value) -> Result<String, String> {
    id_arg(args, "task_id", Some("card_id"))
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
        "column_id": card.column_id,
        "status": card.status,
        "title": card.title,
        "tag": card.tag,
        "assignee": card.assignee,
        "priority": card.priority,
        "pr_url": card.pr_url,
        "repo": card.repo,
        "labels": card.labels,
        "description": card.description,
        "position": card.position,
        "updated_at": card.updated_at,
    })
}

fn labels_arg(args: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item.as_str().ok_or_else(|| format!("{key} entries must be strings"))?;
                out.push(s.to_string());
            }
            Ok(Some(out))
        }
        Some(Value::String(s)) => Ok(Some(s.split(',').map(ToString::to_string).collect())),
        _ => Err(format!("{key} must be an array of strings")),
    }
}

fn parse_substeps(description: Option<&str>) -> Vec<Value> {
    let Some(desc) = description else {
        return Vec::new();
    };
    desc.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let (done, text) = trimmed
                .strip_prefix("- [ ] ")
                .or_else(|| trimmed.strip_prefix("* [ ] "))
                .map(|t| (false, t))
                .or_else(|| {
                    trimmed
                        .strip_prefix("- [x] ")
                        .or_else(|| trimmed.strip_prefix("- [X] "))
                        .or_else(|| trimmed.strip_prefix("* [x] "))
                        .or_else(|| trimmed.strip_prefix("* [X] "))
                        .map(|t| (true, t))
                })?;
            Some(json!({ "text": text.trim(), "done": done }))
        })
        .collect()
}

fn notify(app: &AppState, text: String, card: &Card) {
    if let Some(bot) = app.telegram.clone() {
        tokio::spawn(telegram::notify_card(bot, text, card.clone()));
    }
}

async fn list_tasks(app: &AppState, args: &Value) -> ToolResult {
    let assignee = str_arg(args, "assignee");
    let status = str_arg(args, "status");
    let repo = str_arg(args, "repo");
    let mut filter_labels = labels_arg(args, "labels")?.unwrap_or_default();
    if let Some(single) = str_arg(args, "label") {
        filter_labels.push(single.to_string());
    }
    let labels_opt = (!filter_labels.is_empty()).then_some(filter_labels.as_slice());

    let limit = match args.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIST_LIMIT,
        Some(v) => {
            let n = v.as_i64().ok_or("limit must be an integer")?;
            if !(1..=MAX_LIST_LIMIT as i64).contains(&n) {
                return Err(format!("limit must be between 1 and {MAX_LIST_LIMIT}"));
            }
            n as usize
        }
    };

    let tasks = service::list_tasks(app, assignee, status, repo, labels_opt, limit)
        .await
        .map_err(app_err)?;
    Ok(json!({
        "count": tasks.len(),
        "tasks": tasks.iter().map(card_json).collect::<Vec<_>>(),
    }))
}

async fn get_task_details(app: &AppState, args: &Value) -> ToolResult {
    let key = task_id_arg(args)?;
    let card = service::resolve_card(app, &key).await.map_err(app_err)?;
    let comments = service::list_comments(app, &card.id).await.map_err(app_err)?;
    let substeps = parse_substeps(card.description.as_deref());
    Ok(json!({
        "task": card_json(&card),
        "substeps": substeps,
        "comments": comments,
    }))
}

async fn update_task_status(app: &AppState, args: &Value) -> ToolResult {
    let key = task_id_arg(args)?;
    let status = str_arg(args, "status").ok_or("status is required")?;
    let assignee = str_arg(args, "assignee").map(str::to_string);
    let pr_url = args.get("pr_url").and_then(Value::as_str).map(str::to_string);
    let repo = args.get("repo").and_then(Value::as_str).map(str::to_string);
    let labels = labels_arg(args, "labels")?;
    let priority = match args.get("priority") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_i64().ok_or("priority must be an integer")?),
    };

    let before = service::resolve_card(app, &key).await.map_err(app_err)?;
    let patch = PatchCard {
        status: Some(status.to_string()),
        assignee,
        pr_url,
        repo,
        labels,
        priority,
        ..Default::default()
    };
    let card = service::update_card(app, &before.id, patch, Some(ORIGIN.into()))
        .await
        .map_err(app_err)?;

    if card.column_id == DONE_COLUMN && before.column_id != DONE_COLUMN {
        notify(
            app,
            format!("🤖 MCP Agent moved \"<b>{}</b>\" to Done", telegram::esc(&card.title)),
            &card,
        );
    }

    Ok(json!({
        "previous_status": before.status,
        "moved_from": column_title(&before.column_id),
        "task": card_json(&card),
    }))
}

async fn add_task_comment(app: &AppState, args: &Value) -> ToolResult {
    let key = task_id_arg(args)?;
    let comment = str_arg(args, "comment").ok_or("comment is required")?;
    let author = str_arg(args, "author");

    let (card, comment_row) = service::add_card_comment(app, &key, author, comment, Some(ORIGIN.into()))
        .await
        .map_err(app_err)?;

    Ok(json!({
        "comment": comment_row,
        "task": card_json(&card),
    }))
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
    let assignee = str_arg(args, "assignee").unwrap_or("agent").to_string();
    let repo = str_arg(args, "repo").map(str::to_string);
    let labels = labels_arg(args, "labels")?;
    let priority = match args.get("priority") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_i64().ok_or("priority must be an integer")?),
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
            status: None,
            priority,
            pr_url: None,
            repo,
            labels,
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
            .and_then(|c| c.cards.iter().find(|card| card.assignee != "human"))
            .cloned()
    };

    if let Some(card) = next_in("build") {
        return Ok(json!({
            "status": "resumed",
            "message": "You already have this task in progress in Build — finish it first.",
            "task": card_json(&card),
            "next_steps": "Record progress with add_task_comment or update_card_notes, then update_task_status to done.",
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
        "next_steps": "Record progress with add_task_comment or update_card_notes, then update_task_status to done.",
    }))
}

async fn list_projects(app: &AppState) -> ToolResult {
    let projects = service::list_projects(app).await.map_err(app_err)?;
    Ok(json!({
        "count": projects.len(),
        "projects": projects,
    }))
}

async fn get_project_details(app: &AppState, args: &Value) -> ToolResult {
    let key = str_arg(args, "project")
        .or_else(|| str_arg(args, "project_id"))
        .or_else(|| str_arg(args, "id"))
        .or_else(|| str_arg(args, "name"))
        .ok_or("project (id or name) is required")?;
    let project = service::resolve_project(app, key).await.map_err(app_err)?;
    Ok(json!({
        "project": project,
    }))
}

async fn list_assignees(app: &AppState, args: &Value) -> ToolResult {
    let kind_filter = match str_arg(args, "kind").map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("human") => Some("human"),
        Some("ai") => Some("ai"),
        Some(other) => return Err(format!("invalid kind '{other}' — use \"human\" or \"ai\"")),
    };
    let mut assignees = service::list_assignees(app).await.map_err(app_err)?;
    if let Some(k) = kind_filter {
        assignees.retain(|a| a.kind == k);
    }
    Ok(json!({
        "count": assignees.len(),
        "assignees": assignees,
    }))
}

async fn create_project(app: &AppState, args: &Value) -> ToolResult {
    let name = str_arg(args, "name").ok_or("name is required")?.to_string();
    let id = str_arg(args, "id").map(str::to_string);
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let links = match args.get("links") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value::<Vec<ProjectLink>>(v.clone())
                .map_err(|e| format!("invalid links array: {e}"))?,
        ),
    };
    let project = service::create_project(
        app,
        CreateProject { id, name, description, links },
        Some(ORIGIN.into()),
    )
    .await
    .map_err(app_err)?;
    Ok(json!({ "project": project }))
}

async fn create_assignee(app: &AppState, args: &Value) -> ToolResult {
    let name = str_arg(args, "name").ok_or("name is required")?.to_string();
    let id = str_arg(args, "id").map(str::to_string);
    let kind = match str_arg(args, "kind").map(str::to_ascii_lowercase).as_deref() {
        Some("human") => AssigneeKind::Human,
        Some("ai") => AssigneeKind::Ai,
        Some(other) => return Err(format!("invalid kind '{other}' — use \"human\" or \"ai\"")),
        None => return Err("kind is required (\"human\" or \"ai\")".into()),
    };
    let role_description = args
        .get("role_description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let avatar_badge = str_arg(args, "avatar_badge").map(str::to_string);
    let assignee = service::create_assignee(
        app,
        CreateAssignee {
            id,
            name,
            kind,
            role_description,
            avatar_badge,
        },
        Some(ORIGIN.into()),
    )
    .await
    .map_err(app_err)?;
    Ok(json!({ "assignee": assignee }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    async fn test_app() -> (AppState, String) {
        let path = format!("/tmp/shipmd_mcp_test_{}.db", uuid::Uuid::new_v4());
        let pool = crate::db::connect(&path).await.unwrap();
        let (events, _) = broadcast::channel(64);
        let state = AppState {
            pool,
            events,
            board_title: "Ship Board Test".into(),
            telegram: None,
        };
        (state, path)
    }

    fn cleanup_db(path: &str) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }

    async fn post_mcp_uri(
        app: &Router,
        uri: &str,
        auth_header: Option<&str>,
        payload: Value,
    ) -> (StatusCode, Value) {
        let mut req = axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(auth) = auth_header {
            req = req.header(header::AUTHORIZATION, auth);
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::from(payload.to_string())).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, body)
    }

    async fn post_mcp(app: &Router, auth_header: Option<&str>, payload: Value) -> (StatusCode, Value) {
        post_mcp_uri(app, "/mcp", auth_header, payload).await
    }

    #[test]
    fn columns_accept_titles_and_ids() {
        assert_eq!(column_arg("Backlog").unwrap(), "backlog");
        assert_eq!(column_arg("done").unwrap(), "done");
        assert_eq!(column_arg("SELL").unwrap(), "sell");
        assert!(column_arg("Doing").is_err());
    }

    #[test]
    fn tool_schemas_are_objects() {
        let tools = tool_definitions();
        let tools = tools.as_array().unwrap();
        assert_eq!(tools.len(), 14);
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            assert_eq!(t["inputSchema"]["additionalProperties"], false, "{}", t["name"]);
        }
    }

    #[tokio::test]
    async fn auth_rejects_unset_token_and_invalid_bearer_with_401() {
        let (state, db_path) = test_app().await;

        // 1. No token configured -> 401 Unauthorized
        let unconfigured = router_with_token(state.clone(), None);
        let (st, _) = post_mcp(
            &unconfigured,
            None,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // 2. Token configured -> missing or wrong bearer -> 401; matching bearer -> 200
        let guarded = router_with_token(state.clone(), Some(Arc::from("secret-shipmd-token")));
        let (st_missing, _) = post_mcp(
            &guarded,
            None,
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
        assert_eq!(st_missing, StatusCode::UNAUTHORIZED);

        let (st_wrong, _) = post_mcp(
            &guarded,
            Some("Bearer wrong-token"),
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
        assert_eq!(st_wrong, StatusCode::UNAUTHORIZED);

        let (st_ok, body_ok) = post_mcp(
            &guarded,
            Some("Bearer secret-shipmd-token"),
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await;
        assert_eq!(st_ok, StatusCode::OK);
        assert_eq!(body_ok["result"]["tools"].as_array().unwrap().len(), 14);

        state.pool.close().await;
        cleanup_db(&db_path);
    }

    #[tokio::test]
    async fn mcp_lifecycle_and_task_tools_end_to_end() {
        let (state, db_path) = test_app().await;
        let app = router_with_token(state.clone(), Some(Arc::from("tok-123")));
        let auth = Some("Bearer tok-123");

        // initialize
        let (st, init) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "hermes" } }
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");

        // notifications/initialized (JSON-RPC notification -> 202 Accepted)
        let (st, notif) = post_mcp(
            &app,
            auth,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        assert_eq!(st, StatusCode::ACCEPTED);
        assert_eq!(notif, Value::Null);

        // Create two tasks with different priorities for hermes
        let low = service::create_card(
            &state,
            CreateCard {
                id: None,
                column_id: "backlog".into(),
                title: "Low priority task".into(),
                description: Some("- [ ] step 1\n- [x] step 0".into()),
                tag: Some(Tag::Build),
                assignee: Some("hermes".into()),
                status: None,
                priority: Some(1),
                pr_url: None,
                repo: Some("ceviz-ai".into()),
                labels: Some(vec!["backend".into()]),
                position: None,
            },
            None,
        )
        .await
        .unwrap();

        let high = service::create_card(
            &state,
            CreateCard {
                id: None,
                column_id: "backlog".into(),
                title: "High priority task".into(),
                description: Some("- [ ] wire /mcp\n- [ ] run cargo test".into()),
                tag: Some(Tag::Build),
                assignee: Some("antigravity".into()),
                status: None,
                priority: Some(10),
                pr_url: None,
                repo: Some("shipmd".into()),
                labels: Some(vec!["#frontend".into(), "bug".into()]),
                position: None,
            },
            None,
        )
        .await
        .unwrap();

        // list_tasks sorted by priority descending
        let (st, list_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "list_tasks",
                    "arguments": { "status": "todo", "limit": 10 }
                }
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let parsed: Value = serde_json::from_str(list_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["id"], high.id);
        assert_eq!(tasks[0]["priority"], 10);
        assert_eq!(tasks[1]["id"], low.id);

        // list_tasks filtered by assignee
        let (_, hermes_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "list_tasks",
                    "arguments": { "assignee": "hermes" }
                }
            }),
        )
        .await;
        let hermes_parsed: Value =
            serde_json::from_str(hermes_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(hermes_parsed["count"], 1);
        assert_eq!(hermes_parsed["tasks"][0]["id"], low.id);

        // update_task_status -> review with assignee & pr_url
        let (_, upd_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "update_task_status",
                    "arguments": {
                        "task_id": &high.id[..8],
                        "status": "review",
                        "assignee": "hermes",
                        "pr_url": "https://github.com/ylnyorulmaz/shipmd/pull/42"
                    }
                }
            }),
        )
        .await;
        let upd_parsed: Value =
            serde_json::from_str(upd_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(upd_parsed["task"]["status"], "review");
        assert_eq!(upd_parsed["task"]["column_id"], "build");
        assert_eq!(upd_parsed["task"]["assignee"], "hermes");
        assert_eq!(
            upd_parsed["task"]["pr_url"],
            "https://github.com/ylnyorulmaz/shipmd/pull/42"
        );

        // add_task_comment
        let (_, cmt_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "add_task_comment",
                    "arguments": {
                        "task_id": &high.id[..8],
                        "author": "hermes",
                        "comment": "Implemented /mcp and opened PR #42"
                    }
                }
            }),
        )
        .await;
        assert_eq!(cmt_res["result"]["isError"], false);

        // get_task_details returns description, substeps, comments, and status
        let (_, det_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "get_task_details",
                    "arguments": { "task_id": high.id }
                }
            }),
        )
        .await;
        let det_parsed: Value =
            serde_json::from_str(det_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(det_parsed["task"]["status"], "review");
        assert_eq!(det_parsed["substeps"].as_array().unwrap().len(), 2);
        assert_eq!(det_parsed["comments"].as_array().unwrap().len(), 1);
        assert_eq!(det_parsed["comments"][0]["author"], "hermes");
        assert_eq!(det_parsed["comments"][0]["comment"], "Implemented /mcp and opened PR #42");
        assert_eq!(det_parsed["task"]["repo"], "shipmd");
        assert_eq!(det_parsed["task"]["labels"], json!(["frontend", "bug"]));

        // MCP create_card with repo and labels, list_tasks filtering by repo & label, update_task_status updating repo & labels
        let (_, create_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "create_card",
                    "arguments": {
                        "title": "Kimi modal task",
                        "assignee": "kimi",
                        "repo": "antisocial",
                        "labels": ["#ui", "priority"],
                        "priority": 8
                    }
                }
            }),
        )
        .await;
        let created_parsed: Value =
            serde_json::from_str(create_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        let created_id = created_parsed["created"]["id"].as_str().unwrap().to_string();
        assert_eq!(created_parsed["created"]["repo"], "antisocial");
        assert_eq!(created_parsed["created"]["labels"], json!(["ui", "priority"]));
        assert_eq!(created_parsed["created"]["assignee"], "kimi");

        let (_, filter_repo_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": "list_tasks",
                    "arguments": { "repo": "antisocial", "label": "#ui" }
                }
            }),
        )
        .await;
        let filter_parsed: Value =
            serde_json::from_str(filter_repo_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(filter_parsed["count"], 1);
        assert_eq!(filter_parsed["tasks"][0]["id"], created_id);

        let (_, upd_labels_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {
                    "name": "update_task_status",
                    "arguments": {
                        "task_id": created_id,
                        "status": "in_progress",
                        "repo": "kahvefalim",
                        "labels": ["ux"]
                    }
                }
            }),
        )
        .await;
        let upd_labels_parsed: Value =
            serde_json::from_str(upd_labels_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(upd_labels_parsed["task"]["repo"], "kahvefalim");
        assert_eq!(upd_labels_parsed["task"]["labels"], json!(["ux"]));

        state.pool.close().await;
        cleanup_db(&db_path);
    }

    #[tokio::test]
    async fn auth_accepts_query_and_path_tokens_and_rejects_invalid_with_401() {
        let (state, db_path) = test_app().await;
        let tools_req = || json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});

        // Unconfigured server rejects query & path tokens as 401 as well.
        let unconfigured = router_with_token(state.clone(), None);
        let (st, _) = post_mcp_uri(&unconfigured, "/mcp?token=any", None, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _) = post_mcp_uri(&unconfigured, "/mcp/t/any", None, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        let token = "shipmd_mcp_9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c1234";
        let guarded = router_with_token(state.clone(), Some(Arc::from(token)));

        // Query token: valid -> 200 (also alongside extra query params)
        let (st, body) = post_mcp_uri(
            &guarded,
            &format!("/mcp?token={token}"),
            None,
            tools_req(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["result"]["tools"].as_array().unwrap().len(), 14);

        let (st, _) = post_mcp_uri(
            &guarded,
            &format!("/mcp?client=web&token={token}"),
            None,
            tools_req(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // Query token: wrong, empty, or duplicated -> 401
        for bad_uri in [
            "/mcp?token=wrong",
            "/mcp?token=",
            &format!("/mcp?token={token}&token=wrong"),
        ] {
            let (st, _) = post_mcp_uri(&guarded, bad_uri, None, tools_req()).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "expected 401 for {bad_uri}");
        }

        // Path prefix /mcp/t/<token> (and trailing slash): valid -> 200; wrong -> 401
        let (st, body) = post_mcp_uri(
            &guarded,
            &format!("/mcp/t/{token}"),
            None,
            tools_req(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["result"]["tools"].as_array().unwrap().len(), 14);

        let (st, _) = post_mcp_uri(
            &guarded,
            &format!("/mcp/t/{token}/"),
            None,
            tools_req(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        let (st, _) = post_mcp_uri(&guarded, "/mcp/t/wrong-token", None, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        let (st, _) = post_mcp_uri(&guarded, "/mcp/t/bad%ZZ", None, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // Mismatched dual credentials (e.g. valid Bearer + wrong query/path) -> 401
        let auth_ok = Some("Bearer shipmd_mcp_9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c1234");
        let (st, _) = post_mcp_uri(&guarded, "/mcp?token=wrong", auth_ok, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _) = post_mcp_uri(&guarded, "/mcp/t/wrong", auth_ok, tools_req()).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // Percent-encoded special chars in path and query
        let special = router_with_token(state.clone(), Some(Arc::from("tok+123/abc")));
        let (st, _) = post_mcp_uri(&special, "/mcp/t/tok%2B123%2Fabc", None, tools_req()).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) = post_mcp_uri(&special, "/mcp?token=tok%2B123%2Fabc", None, tools_req()).await;
        assert_eq!(st, StatusCode::OK);

        state.pool.close().await;
        cleanup_db(&db_path);
    }

    async fn open_sse_endpoint(
        app: &Router,
        uri: &str,
        auth_header: Option<&str>,
    ) -> (StatusCode, Option<String>, Option<axum::body::BodyDataStream>) {
        let mut req = axum::http::Request::builder().method("GET").uri(uri);
        if let Some(auth) = auth_header {
            req = req.header(header::AUTHORIZATION, auth);
        }
        let resp = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        if status != StatusCode::OK {
            return (status, None, None);
        }
        let mut data_stream = resp.into_body().into_data_stream();
        let first = data_stream.next().await.unwrap().unwrap();
        let text = String::from_utf8(first.to_vec()).unwrap();
        let endpoint = text
            .lines()
            .find_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .map(ToString::to_string);
        (status, endpoint, Some(data_stream))
    }

    #[tokio::test]
    async fn sse_preserves_query_and_path_token_for_messages() {
        let (state, db_path) = test_app().await;
        let token = "sse-tok-123";
        let app = router_with_token(state.clone(), Some(Arc::from(token)));
        let list_req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});

        // Unauthorized SSE attempts -> 401
        let (st, _, _) = open_sse_endpoint(&app, "/mcp/sse", None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _, _) = open_sse_endpoint(&app, "/mcp/sse?token=wrong", None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _, _) = open_sse_endpoint(&app, "/mcp/t/wrong/sse", None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // 1. Bearer SSE -> clean /mcp/messages?sessionId=...
        let (st, endpoint, _stream1) =
            open_sse_endpoint(&app, "/mcp/sse", Some("Bearer sse-tok-123")).await;
        assert_eq!(st, StatusCode::OK);
        let ep1 = endpoint.unwrap();
        assert!(ep1.starts_with("/mcp/messages?sessionId="), "got {ep1}");
        assert!(!ep1.contains("token="));

        // 2. Query-token SSE -> endpoint keeps &token=..., POST without headers succeeds (202)
        let (st, endpoint, _stream2) =
            open_sse_endpoint(&app, "/mcp/sse?token=sse-tok-123", None).await;
        assert_eq!(st, StatusCode::OK);
        let ep2 = endpoint.unwrap();
        assert!(ep2.starts_with("/mcp/messages?sessionId="), "got {ep2}");
        assert!(ep2.ends_with("&token=sse-tok-123"), "got {ep2}");
        let (post_st, _) = post_mcp_uri(&app, &ep2, None, list_req.clone()).await;
        assert_eq!(post_st, StatusCode::ACCEPTED);

        // 3. Path-prefix SSE -> endpoint uses /mcp/t/<token>/messages?sessionId=..., POST without headers succeeds (202)
        let (st, endpoint, _stream3) =
            open_sse_endpoint(&app, "/mcp/t/sse-tok-123/sse", None).await;
        assert_eq!(st, StatusCode::OK);
        let ep3 = endpoint.unwrap();
        assert!(ep3.starts_with("/mcp/t/sse-tok-123/messages?sessionId="), "got {ep3}");
        let (post_st, _) = post_mcp_uri(&app, &ep3, None, list_req).await;
        assert_eq!(post_st, StatusCode::ACCEPTED);

        state.pool.close().await;
        cleanup_db(&db_path);
    }

    #[tokio::test]
    async fn mcp_projects_and_assignees_tools_end_to_end() {
        let (state, db_path) = test_app().await;
        let app = router_with_token(state.clone(), Some(Arc::from("tok-proj")));
        let auth = Some("Bearer tok-proj");

        // create_project
        let (st, cp_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "create_project",
                    "arguments": {
                        "name": "CodeSight",
                        "description": "AI code intelligence platform",
                        "links": [
                            { "label": "GitHub", "url": "https://github.com/org/codesight" },
                            { "label": "Live", "url": "https://codesight.dev" }
                        ]
                    }
                }
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let cp_parsed: Value =
            serde_json::from_str(cp_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(cp_parsed["project"]["id"], "codesight");
        assert_eq!(cp_parsed["project"]["links"].as_array().unwrap().len(), 2);

        // list_projects
        let (_, lp_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": { "name": "list_projects", "arguments": {} }
            }),
        )
        .await;
        let lp_parsed: Value =
            serde_json::from_str(lp_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(lp_parsed["projects"].as_array().unwrap().iter().any(|p| p["id"] == "codesight"));

        // get_project_details by name (case-insensitive)
        let (_, gpd_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "get_project_details", "arguments": { "project": "codesight" } }
            }),
        )
        .await;
        let gpd_parsed: Value =
            serde_json::from_str(gpd_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(gpd_parsed["project"]["name"], "CodeSight");
        assert_eq!(gpd_parsed["project"]["description"], "AI code intelligence platform");

        // create_assignee (human + ai)
        let (_, ca_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "create_assignee",
                    "arguments": {
                        "name": "ChatGPT",
                        "kind": "ai",
                        "role_description": "Architecture & spec writer"
                    }
                }
            }),
        )
        .await;
        let ca_parsed: Value =
            serde_json::from_str(ca_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(ca_parsed["assignee"]["id"], "chatgpt");
        assert_eq!(ca_parsed["assignee"]["kind"], "ai");

        // list_assignees (all & filtered)
        let (_, la_res) = post_mcp(
            &app,
            auth,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": { "name": "list_assignees", "arguments": { "kind": "ai" } }
            }),
        )
        .await;
        let la_parsed: Value =
            serde_json::from_str(la_res["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(la_parsed["assignees"].as_array().unwrap().iter().any(|a| a["id"] == "chatgpt"));
        assert!(la_parsed["assignees"].as_array().unwrap().iter().all(|a| a["kind"] == "ai"));

        state.pool.close().await;
        cleanup_db(&db_path);
    }
}
