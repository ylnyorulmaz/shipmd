//! Telegram bot: a small `reqwest` long-polling loop against the Bot API.
//!
//! Every action goes through `crate::service`, i.e. the same SQLite pool and the same
//! WebSocket broadcast as the HTTP API — a tap in Telegram animates the open web board.
//!
//! Callback data (≤ 64 bytes): `move:<card_id>:<column>`, `toggle_assignee:<card_id>`,
//! `delete:<card_id>`, `open:<card_id>`, `board`, `menu:<board|add|ratio>`.

use std::{
    fmt,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    db::COLUMNS,
    error::AppError,
    models::{Assignee, Card, CreateCard, PatchCard, Tag},
    service::{self, Ratio, DONE_COLUMN},
    AppState,
};

/// WebSocket `origin` for bot-made changes (never equals a browser tab's id, so every tab animates them).
const ORIGIN: &str = "telegram";
/// Long-poll duration for getUpdates, in seconds.
const POLL_TIMEOUT: u64 = 30;
/// Max card buttons on the /board message (Telegram allows 100 buttons per keyboard).
const MAX_BOARD_BUTTONS: usize = 30;

// ---------------------------------------------------------------------------
// Bot handle
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Telegram {
    http: reqwest::Client,
    /// `{api_url}/bot{token}` — never logged.
    endpoint: Arc<str>,
    allowed_chat: Option<i64>,
    /// Chat that receives agent notifications: the allowed chat, else the last chat that wrote to us (0 = none yet).
    notify_chat: Arc<AtomicI64>,
}

#[derive(Debug)]
pub enum TgError {
    Http(reqwest::Error),
    Api(String),
}

impl fmt::Display for TgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TgError::Http(e) => write!(f, "telegram http error: {e}"),
            TgError::Api(m) => write!(f, "telegram api error: {m}"),
        }
    }
}

impl std::error::Error for TgError {}

impl From<reqwest::Error> for TgError {
    fn from(e: reqwest::Error) -> Self {
        // Strip the URL: it contains the bot token.
        TgError::Http(e.without_url())
    }
}

#[derive(Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    parameters: Option<ResponseParameters>,
}

#[derive(Deserialize)]
struct ResponseParameters {
    retry_after: Option<u64>,
}

impl Telegram {
    /// Reads `TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALLOWED_CHAT_ID` and (for tests/self-hosted Bot API)
    /// `TELEGRAM_API_URL`. Returns `None` — and the web server keeps running — when not configured.
    pub fn from_env() -> Option<Self> {
        let token = match std::env::var("TELEGRAM_BOT_TOKEN") {
            Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => {
                tracing::warn!("TELEGRAM_BOT_TOKEN not set — Telegram bot disabled");
                return None;
            }
        };
        let allowed_chat = match std::env::var("TELEGRAM_ALLOWED_CHAT_ID") {
            Ok(v) if !v.trim().is_empty() => match v.trim().parse::<i64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    tracing::error!("TELEGRAM_ALLOWED_CHAT_ID is not a number — Telegram bot disabled");
                    return None;
                }
            },
            _ => {
                tracing::warn!("TELEGRAM_ALLOWED_CHAT_ID not set — the bot will answer ANY chat (prototype mode)");
                None
            }
        };
        let api = std::env::var("TELEGRAM_API_URL").unwrap_or_else(|_| "https://api.telegram.org".into());
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(POLL_TIMEOUT + 15))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "could not build HTTP client — Telegram bot disabled");
                return None;
            }
        };
        Some(Self {
            http,
            endpoint: format!("{}/bot{}", api.trim_end_matches('/'), token).into(),
            allowed_chat,
            notify_chat: Arc::new(AtomicI64::new(allowed_chat.unwrap_or(0))),
        })
    }

    fn is_allowed(&self, chat_id: i64) -> bool {
        self.allowed_chat.is_none_or(|id| id == chat_id)
    }

    fn notify_chat(&self) -> Option<i64> {
        Some(self.notify_chat.load(Ordering::Relaxed)).filter(|&id| id != 0)
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, body: &Value) -> Result<T, TgError> {
        // One retry when Telegram rate-limits us (429 + retry_after).
        for attempt in 0..2 {
            let res: TgResponse<T> = self
                .http
                .post(format!("{}/{}", self.endpoint, method))
                .json(body)
                .send()
                .await?
                .json()
                .await?;
            if res.ok {
                return res.result.ok_or_else(|| TgError::Api(format!("{method}: empty result")));
            }
            match res.parameters.and_then(|p| p.retry_after) {
                Some(secs) if attempt == 0 => tokio::time::sleep(Duration::from_secs(secs.min(30))).await,
                _ => return Err(TgError::Api(res.description.unwrap_or_else(|| format!("{method} failed")))),
            }
        }
        unreachable!()
    }

    async fn send(&self, chat_id: i64, text: &str, keyboard: Option<Value>) -> Result<(), TgError> {
        let mut body = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
        });
        if let Some(kb) = keyboard {
            body["reply_markup"] = kb;
        }
        self.call::<Value>("sendMessage", &body).await.map(|_| ())
    }

    /// Edit a message in place; "message is not modified" is not an error for us.
    async fn edit(&self, chat_id: i64, message_id: i64, text: &str, keyboard: Value) -> Result<(), TgError> {
        let body = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
            "reply_markup": keyboard,
        });
        match self.call::<Value>("editMessageText", &body).await {
            Err(TgError::Api(m)) if m.contains("message is not modified") => Ok(()),
            other => other.map(|_| ()),
        }
    }

    async fn answer_callback(&self, callback_id: &str, text: Option<&str>) {
        let mut body = json!({ "callback_query_id": callback_id });
        if let Some(t) = text {
            body["text"] = json!(t);
        }
        if let Err(e) = self.call::<bool>("answerCallbackQuery", &body).await {
            tracing::debug!(error = %e, "answerCallbackQuery failed");
        }
    }
}

// ---------------------------------------------------------------------------
// Outbound notifications
// ---------------------------------------------------------------------------

/// Send a one-off HTML message (optionally with an inline keyboard). Failures are logged, never propagated.
pub async fn notify_telegram(bot: &Telegram, chat_id: i64, message: &str, keyboard: Option<Value>) {
    if let Err(e) = bot.send(chat_id, message, keyboard).await {
        tracing::warn!(error = %e, chat_id, "telegram notification failed");
    }
}

/// Fired by `/api/agent/action` when an agent completes a card.
pub async fn notify_agent_completed(bot: Telegram, card: Card) {
    let Some(chat_id) = bot.notify_chat() else {
        tracing::info!("agent completed a card, but no Telegram chat is known yet — send /start to the bot");
        return;
    };
    let text = format!("🤖 Agent completed: \"<b>{}</b>\" -> Moved to Done.", esc(&card.title));
    let keyboard = json!({ "inline_keyboard": [[
        button("↩️ Reopen to Build", format!("move:{}:build", card.id)),
    ]]});
    notify_telegram(&bot, chat_id, &text, Some(keyboard)).await;
}

/// Send `text` (HTML) plus the card's action buttons to the notification chat.
/// Used by the MCP tools ("agent started …", "moved … to Done").
pub async fn notify_card(bot: Telegram, text: String, card: Card) {
    let Some(chat_id) = bot.notify_chat() else {
        tracing::info!("no Telegram chat known yet — send /start to the bot to receive notifications");
        return;
    };
    notify_telegram(&bot, chat_id, &text, Some(card_keyboard(&card))).await;
}

// ---------------------------------------------------------------------------
// Polling loop
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Update {
    update_id: i64,
    message: Option<Message>,
    callback_query: Option<CallbackQuery>,
}

#[derive(Deserialize)]
struct Message {
    message_id: i64,
    chat: Chat,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Chat {
    id: i64,
}

#[derive(Deserialize)]
struct CallbackQuery {
    id: String,
    message: Option<Message>,
    data: Option<String>,
}

#[derive(Deserialize)]
struct BotUser {
    username: Option<String>,
}

pub async fn run(bot: Telegram, state: AppState) {
    match bot.call::<BotUser>("getMe", &json!({})).await {
        Ok(me) => tracing::info!(
            "telegram bot @{} online{}",
            me.username.unwrap_or_default(),
            bot.allowed_chat.map(|id| format!(" (chat {id} only)")).unwrap_or_default()
        ),
        Err(e) => tracing::error!(error = %e, "telegram getMe failed — check TELEGRAM_BOT_TOKEN; will keep retrying"),
    }

    let commands = json!({ "commands": [
        { "command": "board", "description": "View the board & move cards" },
        { "command": "add", "description": "Add a card: /add Fix webhook #build" },
        { "command": "ratio", "description": "Build vs. Sell ratio" },
        { "command": "help", "description": "How this bot works" },
    ]});
    if let Err(e) = bot.call::<bool>("setMyCommands", &commands).await {
        tracing::debug!(error = %e, "setMyCommands failed");
    }

    let mut offset: i64 = 0;
    let mut backoff = 1u64;
    loop {
        let params = json!({
            "offset": offset,
            "timeout": POLL_TIMEOUT,
            "allowed_updates": ["message", "callback_query"],
        });
        match bot.call::<Vec<Update>>("getUpdates", &params).await {
            Ok(updates) => {
                backoff = 1;
                // Sequential on purpose: keeps card positions consistent for rapid-fire quick-adds.
                for update in updates {
                    offset = offset.max(update.update_id + 1);
                    handle_update(&bot, &state, update).await;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "telegram getUpdates failed; retrying in {backoff}s");
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

type BotResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

async fn handle_update(bot: &Telegram, state: &AppState, update: Update) {
    if let Some(msg) = update.message {
        let chat_id = msg.chat.id;
        if !bot.is_allowed(chat_id) {
            tracing::warn!(chat_id, "ignoring message from unauthorized chat");
            return;
        }
        bot.notify_chat.store(chat_id, Ordering::Relaxed);
        let Some(text) = msg.text else { return };
        if let Err(e) = handle_text(bot, state, chat_id, &text).await {
            tracing::warn!(error = %e, "telegram message handler failed");
            let _ = bot.send(chat_id, &format!("⚠️ {}", esc(&user_error(&*e))), None).await;
        }
    } else if let Some(cq) = update.callback_query {
        let Some(msg) = cq.message.as_ref() else {
            bot.answer_callback(&cq.id, Some("This message is too old — send /board")).await;
            return;
        };
        if !bot.is_allowed(msg.chat.id) {
            bot.answer_callback(&cq.id, Some("Not authorized")).await;
            return;
        }
        bot.notify_chat.store(msg.chat.id, Ordering::Relaxed);
        let (chat_id, message_id) = (msg.chat.id, msg.message_id);
        let data = cq.data.clone().unwrap_or_default();
        match handle_callback(bot, state, chat_id, message_id, &data).await {
            Ok(toast) => bot.answer_callback(&cq.id, toast.as_deref()).await,
            Err(e) => {
                tracing::warn!(error = %e, data, "telegram callback failed");
                bot.answer_callback(&cq.id, Some(&user_error(&*e))).await;
            }
        }
    }
}

fn user_error(e: &(dyn std::error::Error + 'static)) -> String {
    match e.downcast_ref::<AppError>() {
        Some(AppError::BadRequest(m)) | Some(AppError::NotFound(m)) => m.clone(),
        _ => "Something went wrong — check the server log".into(),
    }
}

// ---------------------------------------------------------------------------
// Commands & quick-add
// ---------------------------------------------------------------------------

async fn handle_text(bot: &Telegram, state: &AppState, chat_id: i64, text: &str) -> BotResult {
    let text = text.trim();
    let Some(rest) = text.strip_prefix('/') else {
        return quick_add(bot, state, chat_id, text).await;
    };
    let (cmd, args) = match rest.split_once(char::is_whitespace) {
        Some((c, a)) => (c, a.trim()),
        None => (rest, ""),
    };
    // "/add@MyShipBot ..." in groups
    let cmd = cmd.split('@').next().unwrap_or_default().to_ascii_lowercase();

    match cmd.as_str() {
        "start" | "help" => bot.send(chat_id, HELP, Some(main_menu())).await?,
        "board" | "tasks" => {
            let (text, kb) = board_view(state).await?;
            bot.send(chat_id, &text, Some(kb)).await?;
        }
        "add" if args.is_empty() => bot.send(chat_id, ADD_PROMPT, Some(force_reply())).await?,
        "add" => quick_add(bot, state, chat_id, args).await?,
        "ratio" => bot.send(chat_id, &ratio_text(service::shipped_ratio(state).await?), None).await?,
        _ => bot.send(chat_id, "Unknown command. Try /help", None).await?,
    }
    Ok(())
}

const HELP: &str = "👋 <b>Ship Board bot</b>\n\
Your Kanban board, in your pocket. Changes show up live on the web board.\n\n\
/board — view cards &amp; move them with one tap\n\
/add <i>title</i> — add a card to Backlog\n\
/ratio — Build vs. Sell balance of shipped work\n\n\
<b>Quick add:</b> just send any text — it becomes a card.\n\
• <code>#build</code> <code>#sell</code> <code>#ops</code> — tag (default build)\n\
• <code>&gt;build</code> <code>&gt;sell</code> — put it straight into that column\n\
• <code>@agent</code> — assign to your AI agent\n\
• extra lines become the description";

const ADD_PROMPT: &str = "✍️ Send the card title.\n<i>e.g.</i> <code>Tweet demo video #sell</code>";

fn main_menu() -> Value {
    json!({ "inline_keyboard": [
        [button("📋 View Board", "menu:board"), button("➕ Quick Add", "menu:add")],
        [button("⚖️ Build/Sell Ratio", "menu:ratio")],
    ]})
}

fn force_reply() -> Value {
    json!({ "force_reply": true, "input_field_placeholder": "Fix Stripe webhook #build" })
}

struct QuickAdd {
    title: String,
    description: Option<String>,
    tag: Tag,
    column: &'static str,
    assignee: Assignee,
}

/// First line: title + tokens (`#build|#sell|#ops`, `>build|>sell|>backlog`, `@agent`). Other lines: description.
fn parse_quick_add(raw: &str) -> Option<QuickAdd> {
    let mut lines = raw.trim().lines();
    let first = lines.next()?;
    let description = Some(lines.collect::<Vec<_>>().join("\n").trim().to_string()).filter(|d| !d.is_empty());

    let mut out = QuickAdd { title: String::new(), description, tag: Tag::Build, column: "backlog", assignee: Assignee::Human };
    let mut words = Vec::new();
    for word in first.split_whitespace() {
        match word.to_ascii_lowercase().as_str() {
            "#build" => out.tag = Tag::Build,
            "#sell" => out.tag = Tag::Sell,
            "#ops" => out.tag = Tag::Ops,
            ">backlog" => out.column = "backlog",
            ">build" => out.column = "build",
            ">sell" => out.column = "sell",
            "@agent" | "@bot" | "@ai" => out.assignee = Assignee::Agent,
            _ => words.push(word),
        }
    }
    out.title = words.join(" ");
    (!out.title.is_empty()).then_some(out)
}

async fn quick_add(bot: &Telegram, state: &AppState, chat_id: i64, text: &str) -> BotResult {
    let Some(q) = parse_quick_add(text) else {
        bot.send(chat_id, "That card needs a title 🙂", None).await?;
        return Ok(());
    };
    let card = service::create_card(
        state,
        CreateCard {
            id: None,
            column_id: q.column.into(),
            title: q.title,
            description: q.description,
            tag: Some(q.tag),
            assignee: Some(q.assignee),
            position: None,
        },
        Some(ORIGIN.into()),
    )
    .await?;
    let text = format!("➕ <b>Added</b>\n\n{}", card_text(&card));
    bot.send(chat_id, &text, Some(card_keyboard(&card))).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Inline buttons
// ---------------------------------------------------------------------------

/// Returns the toast to show on the user's screen.
async fn handle_callback(
    bot: &Telegram,
    state: &AppState,
    chat_id: i64,
    message_id: i64,
    data: &str,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let (verb, arg) = data.split_once(':').unwrap_or((data, ""));
    let origin = || Some(ORIGIN.to_string());

    // Card-level actions whose target may have been deleted meanwhile.
    let result = match verb {
        "move" => {
            let (id, column) = arg.rsplit_once(':').ok_or("bad move button")?;
            let before = service::fetch_card(state, id).await;
            let patch = PatchCard { column_id: Some(column.into()), ..Default::default() };
            match service::update_card(state, id, patch, origin()).await {
                Ok(card) => {
                    bot.edit(chat_id, message_id, &card_text(&card), card_keyboard(&card)).await?;
                    let same = before.is_ok_and(|b| b.column_id == card.column_id);
                    let col = column_title(&card.column_id);
                    Ok(Some(if same { format!("Already in {col}") } else { format!("Moved to {col}") }))
                }
                Err(e) => Err(e),
            }
        }
        "toggle_assignee" => match service::fetch_card(state, arg).await {
            Ok(card) => {
                let next = if card.assignee == "agent" { Assignee::Human } else { Assignee::Agent };
                let patch = PatchCard { assignee: Some(next), ..Default::default() };
                let card = service::update_card(state, arg, patch, origin()).await?;
                bot.edit(chat_id, message_id, &card_text(&card), card_keyboard(&card)).await?;
                Ok(Some(if next == Assignee::Agent { "Handed to 🤖 Agent".into() } else { "Assigned to 👤 you".into() }))
            }
            Err(e) => Err(e),
        },
        "delete" => match service::delete_card(state, arg, origin()).await {
            Ok(card) => {
                let text = format!("🗑 <b>Deleted</b>\n<s>{}</s>", esc(&card.title));
                bot.edit(chat_id, message_id, &text, board_button()).await?;
                Ok(Some("Card deleted".into()))
            }
            Err(e) => Err(e),
        },
        "open" => match service::fetch_card(state, arg).await {
            Ok(card) => {
                bot.edit(chat_id, message_id, &card_text(&card), card_keyboard(&card)).await?;
                Ok(None)
            }
            Err(e) => Err(e),
        },
        "board" => {
            let (text, kb) = board_view(state).await?;
            bot.edit(chat_id, message_id, &text, kb).await?;
            return Ok(Some("Board refreshed".into()));
        }
        "menu" => {
            match arg {
                "board" => {
                    let (text, kb) = board_view(state).await?;
                    bot.send(chat_id, &text, Some(kb)).await?;
                }
                "add" => bot.send(chat_id, ADD_PROMPT, Some(force_reply())).await?,
                "ratio" => bot.send(chat_id, &ratio_text(service::shipped_ratio(state).await?), None).await?,
                _ => return Err("unknown menu".into()),
            }
            return Ok(None);
        }
        _ => return Err("unknown button".into()),
    };

    match result {
        Ok(toast) => Ok(toast),
        Err(AppError::NotFound(_)) => {
            bot.edit(chat_id, message_id, "This card no longer exists.", board_button()).await?;
            Ok(Some("Card no longer exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn button(text: &str, data: impl Into<String>) -> Value {
    json!({ "text": text, "callback_data": data.into() })
}

fn board_button() -> Value {
    json!({ "inline_keyboard": [[button("📋 Board", "board")]] })
}

fn column_title(id: &str) -> &str {
    COLUMNS.iter().find(|(cid, _)| *cid == id).map_or(id, |(_, title)| title)
}

fn column_emoji(id: &str) -> &'static str {
    match id {
        "backlog" => "📥",
        "build" => "🔨",
        "sell" => "📣",
        "done" => "✅",
        _ => "▫️",
    }
}

fn tag_label(tag: &str) -> &'static str {
    match tag {
        "build" => "🟦 BUILD",
        "sell" => "🟩 SELL",
        _ => "🟨 OPS",
    }
}

fn assignee_label(a: &str) -> &'static str {
    if a == "agent" {
        "🤖 Agent"
    } else {
        "👤 Me"
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn card_text(card: &Card) -> String {
    format!(
        "{} <b>{}</b>\n{} · {} {} · {}\n<code>#{}</code>",
        column_emoji(&card.column_id),
        esc(&card.title),
        tag_label(&card.tag),
        column_emoji(&card.column_id),
        column_title(&card.column_id),
        assignee_label(&card.assignee),
        &card.id[..8.min(card.id.len())],
    )
}

fn card_keyboard(card: &Card) -> Value {
    let id = &card.id;
    let moves: Vec<Value> = if card.column_id == DONE_COLUMN {
        vec![button("↩️ Reopen to Build", format!("move:{id}:build"))]
    } else {
        [("build", "▶ Build"), ("sell", "📣 Sell"), (DONE_COLUMN, "✅ Done")]
            .into_iter()
            .filter(|(col, _)| *col != card.column_id)
            .map(|(col, label)| button(label, format!("move:{id}:{col}")))
            .collect()
    };
    let toggle = if card.assignee == "agent" { "👤 Assign Me" } else { "🤖 Assign Agent" };
    json!({ "inline_keyboard": [
        moves,
        [button(toggle, format!("toggle_assignee:{id}")), button("🗑 Delete", format!("delete:{id}"))],
        [button("📋 Board", "board")],
    ]})
}

fn ratio_text(r: Ratio) -> String {
    if r.build + r.sell == 0 {
        return "⚖️ <b>Build vs. Sell</b>\nNothing shipped yet — move something to Done!".into();
    }
    let mut s = format!(
        "⚖️ <b>Build: {}% | Sell: {}%</b>\n<i>{} build · {} sell cards in Done</i>",
        r.build_pct(),
        r.sell_pct(),
        r.build,
        r.sell
    );
    if r.is_build_heavy() {
        s.push_str("\n\n⚠️ You're shipping mostly code. Time to sell — pick a #sell card or add one now.");
    }
    s
}

/// One compact message: columns with counts, Done summarised, a tap-to-open button per active card.
async fn board_view(state: &AppState) -> Result<(String, Value), AppError> {
    let board = service::board(state).await?;
    let ratio = service::shipped_ratio(state).await?;

    let mut text = format!("📋 <b>{}</b>", esc(&board.title));
    if ratio.build + ratio.sell > 0 {
        text.push_str(&format!(
            "  ·  ⚖️ Build {}% | Sell {}%{}",
            ratio.build_pct(),
            ratio.sell_pct(),
            if ratio.is_build_heavy() { " ⚠️" } else { "" }
        ));
    }

    let mut rows: Vec<Value> = Vec::new();
    let mut hidden = 0;
    for col in &board.columns {
        let id = col.column.id.as_str();
        text.push_str(&format!("\n\n{} <b>{}</b> · {}", column_emoji(id), esc(&col.column.title), col.cards.len()));
        if id == DONE_COLUMN {
            for card in col.cards.iter().take(3) {
                text.push_str(&format!("\n   <s>{}</s>", esc(&truncate(&card.title, 48))));
            }
            if col.cards.len() > 3 {
                text.push_str(&format!("\n   <i>+{} more</i>", col.cards.len() - 3));
            }
            continue;
        }
        for card in &col.cards {
            let who = if card.assignee == "agent" { "🤖" } else { "👤" };
            text.push_str(&format!("\n   {who} {}", esc(&truncate(&card.title, 48))));
            if rows.len() < MAX_BOARD_BUTTONS {
                let label = format!("{} {}", column_emoji(id), truncate(&card.title, 40));
                rows.push(json!([button(&label, format!("open:{}", card.id))]));
            } else {
                hidden += 1;
            }
        }
    }
    if rows.is_empty() {
        text.push_str("\n\nNo active cards. Send me a message to add one.");
    } else {
        text.push_str("\n\n<i>Tap a card to move or reassign it.</i>");
        if hidden > 0 {
            text.push_str(&format!(" <i>({hidden} more on the web board)</i>"));
        }
    }
    rows.push(json!([button("🔄 Refresh", "board")]));
    Ok((text, json!({ "inline_keyboard": rows })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_add_parsing() {
        let q = parse_quick_add("Tweet demo video #sell >sell @agent\nwith a 30s clip").unwrap();
        assert_eq!(q.title, "Tweet demo video");
        assert_eq!(q.tag, Tag::Sell);
        assert_eq!(q.column, "sell");
        assert_eq!(q.assignee, Assignee::Agent);
        assert_eq!(q.description.as_deref(), Some("with a 30s clip"));

        let q = parse_quick_add("Fix Stripe webhook").unwrap();
        assert_eq!((q.tag, q.column, q.assignee), (Tag::Build, "backlog", Assignee::Human));
        assert!(q.description.is_none());

        assert!(parse_quick_add("#ops @agent").is_none());
        assert!(parse_quick_add("   ").is_none());
    }

    #[test]
    fn callback_data_fits_telegram_limit() {
        let id = uuid::Uuid::new_v4().to_string();
        for data in [format!("move:{id}:backlog"), format!("toggle_assignee:{id}"), format!("delete:{id}")] {
            assert!(data.len() <= 64, "{data} is {} bytes", data.len());
        }
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(esc("<b>a & b</b>"), "&lt;b&gt;a &amp; b&lt;/b&gt;");
    }
}
