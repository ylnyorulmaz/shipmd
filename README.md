# Ship Board

A real-time Kanban board for a solo founder and their AI coding agents.

It has one board with four columns (**Backlog → Build → Sell → Done**) and no login. You can change it from four places:

- **Web UI:** drag and drop, edit cards, see updates live.
- **Telegram bot:** add and move cards from your phone.
- **MCP server:** Claude Code, Cursor, Windsurf and Cline can pick up and finish tasks.
- **HTTP API:** for scripts and simple bots.

A change from any of them shows up on every open board within milliseconds.

```
backend/   Rust · Axum 0.8 · Tokio · SQLite (sqlx, WAL) · WebSockets · Telegram · MCP
frontend/  SolidJS · TypeScript · Vite · Tailwind CSS v4
```

---

## What it does

| Feature | Details |
|---|---|
| **Board** | Classic Trello look (gray lists, white cards) with light and dark themes. Each card has a title, a Markdown description, a tag (`build` / `sell` / `ops`) and an assignee (`👤 Me` / `🤖 Agent`). |
| **Drag and drop** | Native HTML5 drag and drop within and across columns, with a drop placeholder. Hovering doesn't re-render the board. |
| **Optimistic UI** | Every edit shows immediately and syncs in the background. If the server rejects it, the change is undone and a toast appears. |
| **Real-time sync** | A WebSocket (`/ws`) sends every change. Cards moved by an agent, the bot or another tab slide to their new spot on screen. |
| **Build vs. Sell bar** | The header shows the build/sell split of cards in Done. It warns when Build is above 75%. |
| **Shortcuts** | `N` opens quick add. `Esc` closes dialogs. In quick add, `#sell` / `#ops` and `@agent` set the tag and assignee. |
| **Agent HTTP endpoint** | `POST /api/agent/action` takes `start` / `complete` / `note` and accepts 8-character short ids. |
| **Telegram bot** | `/board`, `/add`, `/ratio`, and plain-text quick add. Inline buttons move, reassign and delete cards. You get a notification when an agent finishes a task. |
| **MCP server** | `POST /mcp` (Streamable HTTP) and `GET /mcp/sse` (legacy SSE) with 5 tools. AI coding agents can claim work, log progress and close tasks. |

## Architecture

```
 Browser (SolidJS) ──REST──┐                        ┌──► SQLite board.db (WAL)
        ▲                  │                        │
        │ WebSocket /ws    ├──► service.rs ─────────┤
        │                  │   (validate, persist,  │
 curl / scripts ──REST─────┤    broadcast)          └──► broadcast channel ──► every /ws client
 Telegram ◄─long-poll─► telegram.rs ──┤                                       
 Claude Code / Cursor ──► mcp.rs ─────┘          agent "complete" / MCP ──► Telegram notification
```

Everything runs in one Rust process. The HTTP routes, the Telegram bot and the MCP server all call the same functions in `backend/src/service.rs`, so validation, saving and the WebSocket broadcast happen in one place.

## Quick start

Requirements: Rust (stable) and Node 20+.

```bash
# Terminal 1: backend on :3001 (creates and seeds ./board.db on first run)
cd backend
cp .env.example .env          # optional: Telegram / MCP settings
cargo run

# Terminal 2: frontend on :5173 (proxies /api and /ws to :3001)
cd frontend
npm install
npm run dev
```

Open http://localhost:5173.

## Configuration

The backend reads environment variables and loads `backend/.env` if it exists. Real environment variables win over the file.

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `3001` | HTTP, WebSocket and MCP port |
| `BOARD_DB` | `board.db` | SQLite file path; created and seeded if missing |
| `BOARD_TITLE` | `Ship Board` | Board name in the UI and in Telegram |
| `RUST_LOG` | `info,sqlx=warn` | Log level |
| `TELEGRAM_BOT_TOKEN` | — | Bot token from @BotFather. If unset, the bot is off. |
| `TELEGRAM_ALLOWED_CHAT_ID` | — | Your numeric chat id. If set, the bot ignores every other chat. **Set this.** |
| `TELEGRAM_API_URL` | `https://api.telegram.org` | Override for a self-hosted Bot API server or for tests |
| `MCP_API_KEY` | — | If set, `/mcp` requires `Authorization: Bearer <key>`. If unset, only localhost can connect. |

Frontend: `API_URL` changes the target of the Vite dev proxy. `VITE_API_URL` makes the browser call the API directly instead of going through the proxy.

## Using it

### From a script or simple agent (HTTP)

```bash
# Complete the first card in Build. It slides to Done on the board, and Telegram is notified.
ID=$(curl -s localhost:3001/api/board | python3 -c "import sys,json;print(next(c['id'] for col in json.load(sys.stdin)['columns'] if col['id']=='build' for c in col['cards']))")
curl -X POST localhost:3001/api/agent/action -H 'content-type: application/json' \
  -d "{\"card_id\":\"$ID\",\"action\":\"complete\",\"note\":\"Implemented + tests green ✅\"}"
```

Actions:
- `start` moves the card to Build (Sell for sell-tagged cards) and assigns it to the agent.
- `complete` moves it to the top of Done.
- `note` only adds a note.

Each action adds a timestamped `🤖 Agent …` entry to the card's description. `card_id` can be a full UUID or any unique prefix of 4 or more characters.

### From Telegram

Create a bot with @BotFather. Put `TELEGRAM_BOT_TOKEN` and `TELEGRAM_ALLOWED_CHAT_ID` in `backend/.env`, then restart the backend. You should see `telegram bot @YourBot online (chat … only)` in the log.

| Message | Effect |
|---|---|
| `/start`, `/help` | Help text and a menu: 📋 View Board · ➕ Quick Add · ⚖️ Build/Sell Ratio |
| `/board`, `/tasks` | One compact board message. Tap a card, then use ▶ Build / 📣 Sell / ✅ Done / 🤖 Assign Agent / 🗑 Delete. Each tap edits the message in place. |
| `/add <title>` or any plain text | Creates a card. `#build` `#sell` `#ops` set the tag. `>build` / `>sell` pick the column (default Backlog). `@agent` assigns it to the agent. Extra lines become the description. |
| `/ratio` | `Build: X% \| Sell: Y%` for Done cards, with a nudge when Build is above 75% |

When an agent completes a card, the bot sends `🤖 Agent completed: "…" -> Moved to Done.` with a **↩️ Reopen to Build** button.

### From an AI coding agent (MCP)

**Claude Code**: add this to `.mcp.json` in your project, or run `claude mcp add --transport http ship-board http://localhost:3001/mcp --header "Authorization: Bearer $MCP_API_KEY"`:

```json
{
  "mcpServers": {
    "ship-board": {
      "type": "http",
      "url": "http://localhost:3001/mcp",
      "headers": { "Authorization": "Bearer YOUR_MCP_API_KEY" }
    }
  }
}
```

**Cursor**: use `.cursor/mcp.json`, the same entry without `"type"`. Clients that only support the older SSE transport should use `http://localhost:3001/mcp/sse`. If `MCP_API_KEY` is unset, leave out `headers`.

| Tool | Input | Effect |
|---|---|---|
| `get_board` | `column?` | Columns and cards. Without a filter, Done is capped at 10. |
| `create_card` | `title`, `description?`, `column?` (Backlog), `tag?` (build), `assignee?` (agent) | Creates a card and broadcasts `CARD_CREATED` |
| `move_card` | `card_id`, `target_column` | Moves the card and broadcasts `CARD_MOVED`. Moving to Done also notifies Telegram. |
| `update_card_notes` | `card_id`, `note`, `append?` (true) | Appends a timestamped note, or replaces the description when `append` is false. Broadcasts `CARD_UPDATED`. |
| `pick_next_agent_task` | — | Returns the agent's card that is already in Build. Otherwise it moves the top agent card in Backlog to Build and notifies Telegram. |

The intended loop is: `pick_next_agent_task` → work → `update_card_notes` (commit, PR link) → `move_card` to Done.

Quick check with curl:

```bash
curl -s localhost:3001/mcp -H 'Content-Type: application/json' -H "Authorization: Bearer $MCP_API_KEY" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"pick_next_agent_task","arguments":{}}}'
```

## API reference

| Method | Path | Body / response |
|---|---|---|
| GET | `/api/board` | `{ title, columns: [{ id, title, position, cards: [...] }] }`, cards sorted by position |
| POST | `/api/cards` | `{ column_id, title, tag?, assignee?, description?, position?, id? }` → `201` + card |
| PATCH | `/api/cards/:id` | Any of `{ column_id, position, title, description, tag, assignee }`. An empty `description` clears it. |
| DELETE | `/api/cards/:id` | `204` |
| POST | `/api/agent/action` | `{ card_id, action: "start" \| "complete" \| "note", note? }` |
| WS | `/ws` | Server → client: `{ type, card \| id, origin }` |
| POST | `/mcp` | MCP JSON-RPC 2.0 (Streamable HTTP) |
| GET / POST | `/mcp/sse`, `/mcp/messages?sessionId=` | MCP legacy SSE transport |

- **WebSocket event types:** `CARD_CREATED`, `CARD_MOVED`, `CARD_UPDATED`, `CARD_DELETED`. The server also sends `HELLO` on connect and `RESYNC` when a client falls behind and should reload the board.
- **`origin`:** the browser tab's `X-Client-Id`, or `"telegram"` / `"mcp"` for changes from those sources.
- **Allowed values:** columns are `backlog`, `build`, `sell`, `done`. Tags are `build`, `sell`, `ops`. Assignees are `human`, `agent`.
- **Errors:** returned as `{ "error": "…" }` with status 400, 404 or 422.

## Project layout

```
backend/src/
  main.rs       Startup: config, SQLite, router, Telegram task, CORS
  service.rs    All card logic: validate → SQLite → WebSocket broadcast (shared by every entry point)
  routes.rs     Thin REST handlers over service
  ws.rs         /ws: forwards broadcast events, heartbeat, RESYNC on lag
  telegram.rs   Bot: long polling, commands, inline buttons, notifications
  mcp.rs        MCP server: JSON-RPC, both transports, auth guard, 5 tools
  db.rs         Schema, WAL setup, seed data, column list
  models.rs     Card / Column / request types
  error.rs      AppError → JSON HTTP errors
frontend/src/
  store.ts      Solid store, optimistic changes, WebSocket sync, remote-move animation
  dnd.ts        Drag and drop state and drop-position math
  api.ts        fetch wrapper (adds X-Client-Id), WebSocket URL
  components/   Header (ratio bar, theme), BoardColumn, CardItem, Composer, CardModal
```

## Development

```bash
cd backend  && cargo check && cargo test       # must finish with 0 warnings
cd frontend && npm run typecheck && npm run build
```

## Before deploying publicly

This is a prototype built for local use. Before putting it on the internet:

1. **The web UI and `/api/*` have no authentication.** Anyone with the URL can read and change the board. Add a login or token check, or put the app behind Cloudflare Access or Tailscale.
2. **Set `MCP_API_KEY`.** If the key is unset, MCP trusts loopback connections. Behind a reverse proxy on the same host (Caddy, nginx), every request looks like loopback, so anyone could connect.
3. **Set `TELEGRAM_ALLOWED_CHAT_ID`.** Otherwise anyone who finds the bot can change your board.
4. **Run exactly one instance.** WebSocket events live in the process's memory, and Telegram allows only one bot polling at a time. The SQLite file needs a persistent disk.
5. **Serve the frontend.** Run `npm run build` and serve `frontend/dist` from a static host, setting `VITE_API_URL`. The backend can't serve it yet.

Good hosts: Fly.io (one machine plus a volume), a small VPS, or Render on a paid plan with a persistent disk. Serverless platforms such as Cloudflare Workers or the free tiers that sleep won't work.

## Known limitations

- Desktop only for drag and drop. HTML5 drag and drop doesn't work on touch screens.
- Positions are floats, and dropping between two cards uses their midpoint. After thousands of reorders in the same spot, precision runs out. There's no rebalancing yet.
- `pick_next_agent_task` doesn't claim cards atomically. Two agents calling it at the same moment can get the same card.
- `start` from `/api/agent/action` sends sell-tagged cards to Sell, but `pick_next_agent_task` always moves cards to Build.
- No schema migrations. The schema is created with `CREATE TABLE IF NOT EXISTS` in `db.rs`.
