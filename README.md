# Ship Board

A fast, real-time Kanban board for a solo founder and their AI coding agents.
It uses the classic Trello look (gray lists, white cards). There is no login. One board loads on launch.

```
backend/   Rust · Axum 0.8 · Tokio · SQLite (sqlx, WAL) · WebSockets
frontend/  SolidJS · TypeScript · Vite · Tailwind CSS v4
```

## Run it

```bash
# Terminal 1: API + WebSocket on :3001 (creates and seeds ./board.db on first run)
cd backend && cargo run

# Terminal 2: UI on :5173 (proxies /api and /ws to :3001)
cd frontend && npm install && npm run dev
```

Open http://localhost:5173.

Settings (all optional): `PORT` (3001), `BOARD_DB` (board.db), `BOARD_TITLE` (Ship Board), `RUST_LOG`.
The frontend reads `API_URL` (the Vite proxy target) and `VITE_API_URL`, which skips the proxy and calls the API directly. CORS is open.

## Simulate an agent

Every card shows a short id, such as `#fc4d52cc`, in its modal. The **🤖 Copy agent curl** button copies a ready-made command.
`card_id` takes a full UUID or any unique prefix of 4 or more characters.

```bash
# Pick the first Build-column card and have the "agent" complete it. Watch it glide to Done.
ID=$(curl -s localhost:3001/api/board | python3 -c "import sys,json;print(next(c['id'] for col in json.load(sys.stdin)['columns'] if col['id']=='build' for c in col['cards']))")

curl -X POST localhost:3001/api/agent/action \
  -H 'content-type: application/json' \
  -d "{\"card_id\":\"$ID\",\"action\":\"complete\",\"note\":\"Implemented + tests green ✅\"}"
```

Actions:
- `start`: moves the card to Build (Sell for sell-tagged cards) and assigns it to the agent.
- `complete`: moves the card to the top of Done.
- `note`: only appends the note.

Each action appends a timestamped `🤖 Agent …` entry to the card's Markdown description.

## Telegram bot

The bot runs inside the backend process. It shares the SQLite pool and the WebSocket broadcast, so anything you do in Telegram shows up on the web board right away.

```bash
cp backend/.env.example backend/.env   # set TELEGRAM_BOT_TOKEN and TELEGRAM_ALLOWED_CHAT_ID
cd backend && cargo run                # log: "telegram bot @YourBot online (chat … only)"
```

Without a token, the backend logs a warning and runs without the bot.

| In Telegram | Effect |
|---|---|
| `/start`, `/help` | Shows help and a menu: 📋 View Board · ➕ Quick Add · ⚖️ Build/Sell Ratio |
| `/board`, `/tasks` | Sends one compact board message. Tap a card, then use ▶ Build / 📣 Sell / ✅ Done / 🤖 Assign Agent / 🗑 Delete. Each tap edits that message in place. |
| `/add <title>` or any plain text | Creates a card in Backlog. `#build` `#sell` `#ops` set the tag (default build). `>build` / `>sell` pick the column. `@agent` assigns it to the agent. Extra lines become the description. |
| `/ratio` | Shows `Build: X% \| Sell: Y%` for Done cards, with a nudge when Build is above 75%. |

When an agent calls `/api/agent/action` with `"action":"complete"`, the bot sends `🤖 Agent completed: "…" -> Moved to Done.` with a **↩️ Reopen to Build** button. The notification goes to `TELEGRAM_ALLOWED_CHAT_ID`. If that isn't set, it goes to the last chat that messaged the bot.

## MCP server (for Claude Code, Cursor, Windsurf, Cline)

The backend has an MCP server built in. It speaks JSON-RPC 2.0 over two transports:
- **Streamable HTTP** at `POST /mcp`
- **Legacy HTTP+SSE** at `GET /mcp/sse` with `POST /mcp/messages`

The tools go through the same service layer as everything else. Agent changes show up live on the board, and some also notify Telegram.

| Tool | What it does |
|---|---|
| `get_board` | Returns columns and cards (`column` filter optional). Done is capped at the 10 most recent cards. |
| `create_card` | `title` (required), plus `description`, `column` (default Backlog), `tag` (default build), `assignee` (default agent). |
| `move_card` | `card_id` (full UUID or 8-char short id) and `target_column`. Moving to Done notifies Telegram. |
| `update_card_notes` | `card_id` and `note`. `append` defaults to true and adds a timestamped entry; false replaces the description. |
| `pick_next_agent_task` | Returns the agent's card that is already in Build if there is one. Otherwise it moves the top agent card in Backlog to Build and notifies Telegram. |

Auth: if `MCP_API_KEY` is set, clients must send `Authorization: Bearer <key>`. If it is unset, only localhost can connect, and browser requests from other origins are rejected.

```json
{ "mcpServers": { "ship-board": {
  "type": "http",
  "url": "http://localhost:3001/mcp",
  "headers": { "Authorization": "Bearer ${MCP_API_KEY}" }
} } }
```

## API

| Method | Path | Body |
|---|---|---|
| GET | `/api/board` | returns `{ title, columns: [{ id, title, position, cards: [...] }] }`, cards sorted |
| POST | `/api/cards` | `{ column_id, title, tag?, assignee?, description?, position?, id? }` |
| PATCH | `/api/cards/:id` | any of `{ column_id, position, title, description, tag, assignee }` |
| DELETE | `/api/cards/:id` | none |
| POST | `/api/agent/action` | `{ card_id, action: "start" \| "complete" \| "note", note? }` |
| WS | `/ws` | `{ type: CARD_CREATED \| CARD_MOVED \| CARD_UPDATED \| CARD_DELETED, card \| id, origin }` |

Column ids are `backlog`, `build`, `sell` and `done`. Tags are `build`, `sell` and `ops`. Assignees are `human` and `agent`.

## UI notes

- **Optimistic:** creates, moves, edits and deletes update the store at once. Requests sync in the background, and a failed request rolls back with a toast. Each tab sends an `X-Client-Id`, so it ignores the WebSocket echo of its own changes.
- **Drag and drop:** uses native HTML5 drag and drop. The drop slot is one key held in `createSelector`, so moving the pointer only toggles two placeholders. No list or card re-renders.
- **Remote moves:** a card moved by an agent, curl or another tab slides to its new spot (FLIP) and flashes. After a reconnect or lag, the board fetches itself again.
- **Shortcuts:** `N` opens quick add in Backlog. `Esc` closes the modal or composer. In quick add, `#sell` / `#ops` and `@agent` set the tag and assignee.
- **Build vs. Sell bar:** shows the build/sell split of cards in Done. It warns when Build is above 75%.
- Agent-written Markdown is sanitized with DOMPurify before rendering.
