# ShipMD

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
| **Drag and drop** | Pointer-based dragging within and across columns. The card lifts with a Trello-style tilt and swings with momentum. Other cards slide aside, the card glides into its slot on release, and `Esc` sends it back. Columns auto-scroll near the edges, and hovering doesn't re-render the board. |
| **Optimistic UI** | Every edit shows immediately and syncs in the background. If the server rejects it, the change is undone and a toast appears. |
| **Real-time sync** | A WebSocket (`/ws`) sends every change. Cards moved by an agent, the bot or another tab slide to their new spot on screen. |
| **Build vs. Sell bar** | The header shows the build/sell split of cards in Done. It warns when Build is above 75%. |
| **Shortcuts** | `N` opens quick add. `Esc` closes dialogs. In quick add, `#sell` / `#ops` and `@agent` set the tag and assignee. |
| **Agent HTTP endpoint** | `POST /api/agent/action` takes `start` / `complete` / `note` and accepts 8-character short ids. |
| **Telegram bot** | `/board`, `/add`, `/ratio`, and plain-text quick add. Inline buttons move, reassign and delete cards. You get a notification when an agent finishes a task. |
| **MCP server** | `POST /mcp` (Streamable HTTP) and `GET /mcp/sse` (legacy SSE) with 14 tools (`list_tasks`, `get_task_details`, `update_task_status`, `add_task_comment`, `list_projects`, `get_project_details`, `list_assignees`, plus board & workspace helpers). Autonomous agents (Hermes, Antigravity, Claude Code, Cursor) can inspect project context, team roles, claim work, log progress and close tasks. |

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
| `BOARD_TITLE` | `ShipMD` | Board name in the UI and in Telegram |
| `RUST_LOG` | `info,sqlx=warn` | Log level |
| `BOARD_PASSWORD` | — | If set, the web UI, `/api` and `/ws` need this password: HTTP Basic with any username, then a session cookie. **Required when public.** |
| `STATIC_DIR` | — | Folder with the built frontend (`frontend/dist`). If set, the backend serves the UI itself. |
| `TELEGRAM_BOT_TOKEN` | — | Bot token from @BotFather. If unset, the bot is off. |
| `TELEGRAM_ALLOWED_CHAT_ID` | — | Your numeric chat id. If set, the bot ignores every other chat. **Set this.** |
| `TELEGRAM_API_URL` | `https://api.telegram.org` | Override for a self-hosted Bot API server or for tests |
| `SHIPMD_MCP_TOKEN` | — | **Required for `/mcp`.** Clients must send `Authorization: Bearer <token>`. If unset, `/mcp` logs a warning and rejects all requests with `401 Unauthorized` (`MCP_API_KEY` is accepted as a legacy fallback). |

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

**Claude Code / Hermes / Antigravity**: add this to `.mcp.json` in your project, or run `claude mcp add --transport http ship-board http://localhost:3001/mcp --header "Authorization: Bearer $SHIPMD_MCP_TOKEN"`:

```json
{
  "mcpServers": {
    "ship-board": {
      "type": "http",
      "url": "http://localhost:3001/mcp",
      "headers": { "Authorization": "Bearer YOUR_SHIPMD_MCP_TOKEN" }
    }
  }
}
```

**Cursor**: use `.cursor/mcp.json`, the same entry without `"type"`. Clients that only support the older SSE transport should use `http://localhost:3001/mcp/sse`. Set `SHIPMD_MCP_TOKEN` in the backend environment; requests without a matching `Authorization: Bearer <SHIPMD_MCP_TOKEN>` header receive `401 Unauthorized`.

| Tool | Input | Effect |
|---|---|---|
| `list_tasks` | `assignee?`, `status?` (`todo`/`in_progress`/`review`/`done`), `limit?` | Filters tasks and returns them sorted by `priority` descending (highest priority first), then board position. |
| `get_task_details` | `task_id` | Returns full task fields (`status`, `priority`, `pr_url`, `assignee`), Markdown description, parsed checklist `substeps`, and `comments`. |
| `update_task_status` | `task_id`, `status`, `assignee?`, `pr_url?` | Updates status (`todo`, `in_progress`, `review`, `done`, …), optional assignee / PR URL, syncs the board column, and notifies Telegram on `done`. |
| `add_task_comment` | `task_id`, `comment`, `author?` (`agent`) | Records a structured comment (progress note, error report, or PR link) and appends it to the card live. |
| `get_board` | `column?` | Columns and cards. Without a filter, Done is capped at 10. |
| `create_card` | `title`, `description?`, `column?` (Backlog), `tag?` (build), `assignee?` (agent), `priority?` (0) | Creates a card and broadcasts `CARD_CREATED`. |
| `move_card` | `card_id`, `target_column` | Moves the card and broadcasts `CARD_MOVED`. Moving to Done also notifies Telegram. |
| `update_card_notes` | `card_id`, `note`, `append?` (true) | Appends a timestamped note, or replaces the description when `append` is false. Broadcasts `CARD_UPDATED`. |
| `pick_next_agent_task` | — | Returns the agent's card that is already in Build. Otherwise it moves the top agent card in Backlog to Build and notifies Telegram. |
| `list_projects` | — | Lists all defined projects/workspaces with their descriptions and resource link lists. |
| `get_project_details` | `project` | Returns the full description and resource links for a project by ID (slug) or name (case-insensitive). |
| `list_assignees` | `kind?` (`human` \| `ai`) | Lists all human team members and AI agents with their kind, role descriptions, and badges. |
| `create_project` | `name`, `id?`, `description?`, `links?` | Creates a new project/workspace and broadcasts `PROJECT_CREATED`. |
| `create_assignee` | `name`, `kind`, `id?`, `role_description?`, `avatar_badge?` | Registers a new human team member or AI agent and broadcasts `ASSIGNEE_CREATED`. |

The intended loop is: `list_tasks` (or `pick_next_agent_task`) → `get_task_details` → `update_task_status` (`in_progress`) → work → `add_task_comment` (commit, PR link) → `update_task_status` (`review` or `done`, with `pr_url`).

Quick check with curl:

```bash
curl -s localhost:3001/mcp -H 'Content-Type: application/json' -H "Authorization: Bearer $SHIPMD_MCP_TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_tasks","arguments":{"status":"todo"}}}'
```

## API reference

| Method | Path | Body / response |
|---|---|---|
| GET | `/api/board` | `{ title, columns: [{ id, title, position, cards: [...] }], projects, assignees }` |
| POST | `/api/cards` | `{ column_id, title, tag?, assignee?, description?, position?, id? }` → `201` + card |
| PATCH | `/api/cards/:id` | Any of `{ column_id, position, title, description, tag, assignee }`. An empty `description` clears it. |
| DELETE | `/api/cards/:id` | `204` |
| GET / POST | `/api/projects` | List projects or create `{ name, id?, description?, links? }` → `201` + project |
| PUT / DELETE | `/api/projects/:id` | Update `{ name?, description?, links? }` or delete project (`204`) |
| GET / POST | `/api/assignees` | List team members/agents or create `{ name, kind, id?, role_description?, avatar_badge? }` → `201` + assignee |
| PUT / DELETE | `/api/assignees/:id` | Update `{ name?, kind?, role_description?, avatar_badge? }` or delete (`204`) |
| POST | `/api/agent/action` | `{ card_id, action: "start" \| "complete" \| "note", note? }` |
| WS | `/ws` | Server → client: `{ type, card \| project \| assignee \| id, origin }` |
| POST | `/mcp` | MCP JSON-RPC 2.0 (Streamable HTTP) |
| GET | `/healthz` | `ok`, no auth (for health checks) |
| GET / POST | `/mcp/sse`, `/mcp/messages?sessionId=` | MCP legacy SSE transport |

- **WebSocket event types:** `CARD_CREATED`, `CARD_MOVED`, `CARD_UPDATED`, `CARD_DELETED`, `PROJECT_CREATED`, `PROJECT_UPDATED`, `PROJECT_DELETED`, `ASSIGNEE_CREATED`, `ASSIGNEE_UPDATED`, `ASSIGNEE_DELETED`. The server also sends `HELLO` on connect and `RESYNC` when a client falls behind and should reload the board.
- **`origin`:** the browser tab's `X-Client-Id`, or `"telegram"` / `"mcp"` for changes from those sources.
- **Allowed values:** columns are `backlog`, `build`, `sell`, `done`. Tags are `build`, `sell`, `ops`. Assignees accept `"human"`, `"agent"`, or specific agent names such as `"antigravity"` and `"hermes"`. Statuses are `todo`, `in_progress`, `review`, `sell`, `done`.
- **Errors:** returned as `{ "error": "…" }` with status 400, 404 or 422.

## Project layout

```
backend/src/
  main.rs       Startup: config, SQLite, router, static frontend, Telegram task, CORS
  auth.rs       Optional BOARD_PASSWORD guard (Basic auth → session cookie)
  service.rs    All card logic: validate → SQLite → WebSocket broadcast (shared by every entry point)
  routes.rs     Thin REST handlers over service
  ws.rs         /ws: forwards broadcast events, heartbeat, RESYNC on lag
  telegram.rs   Bot: long polling, commands, inline buttons, notifications
  mcp.rs        MCP server: JSON-RPC, both transports, SHIPMD_MCP_TOKEN bearer auth, 14 tools
  db.rs         Schema, WAL setup, idempotent migrations, seed data, column list
  models.rs     Card / Column / request types
  error.rs      AppError → JSON HTTP errors
frontend/src/
  store.ts      Solid store, optimistic changes, WebSocket sync, remote-move animation
  dnd.ts        Pointer drag: ghost, tilt, auto-scroll, drop target, glide-in drop
  flip.ts       FLIP helper that slides cards to their new positions
  api.ts        fetch wrapper (adds X-Client-Id), WebSocket URL
  components/   Header (ratio bar, theme), BoardColumn, CardItem, Composer, CardModal
```

## Development

```bash
cd backend  && cargo check && cargo test       # must finish with 0 warnings
cd frontend && npm run typecheck && npm run build
```

## Deploying (Fly.io)

The root `Dockerfile` builds a single image: the frontend is built, and the Rust binary serves it from `STATIC_DIR`. `fly.toml` runs one always-on machine with a volume at `/data` for SQLite.

```bash
fly launch --copy-config --no-deploy          # or the Fly web launcher; uses fly.toml (app "shipmd", region ams)
fly secrets set BOARD_PASSWORD='…' SHIPMD_MCP_TOKEN='…'
fly secrets set TELEGRAM_BOT_TOKEN='…' TELEGRAM_ALLOWED_CHAT_ID='…'   # optional
fly deploy
fly scale count 1                              # make sure exactly one machine runs
```

Open `https://<app>.fly.dev`. The browser asks for the password; any username works. To test the image locally: `docker build -t shipmd . && docker run -p 8080:8080 -v shipmd:/data -e BOARD_PASSWORD=dev shipmd`.

With a password set, scripts authenticate with Basic auth:

```bash
curl -u ":$BOARD_PASSWORD" https://<app>.fly.dev/api/board
```

MCP clients keep using `Authorization: Bearer $SHIPMD_MCP_TOKEN` on `https://<app>.fly.dev/mcp`.

### Checklist before going public

1. **Set `BOARD_PASSWORD`.** Without it, anyone with the URL can read and change the board.
2. **Set `SHIPMD_MCP_TOKEN`.** Without it, `/mcp` rejects all requests with `401 Unauthorized`.
3. **Set `TELEGRAM_ALLOWED_CHAT_ID`.** Otherwise anyone who finds the bot can change your board.
4. **Run exactly one instance.** WebSocket events live in the process's memory, and Telegram allows only one bot polling at a time. The SQLite file needs a persistent disk.

Other hosts work too: a small VPS (run the image with a volume), or Render on a paid plan with a persistent disk. Serverless platforms such as Cloudflare Workers, and free tiers that sleep, won't work.

## Known limitations

- Dragging needs a mouse or pen. Touch dragging isn't supported yet; tapping a card still opens it.
- Positions are floats, and dropping between two cards uses their midpoint. After thousands of reorders in the same spot, precision runs out. There's no rebalancing yet.
- `pick_next_agent_task` doesn't claim cards atomically. Two agents calling it at the same moment can get the same card.
- `start` from `/api/agent/action` sends sell-tagged cards to Sell, but `pick_next_agent_task` always moves cards to Build.
- `db::migrate()` runs idempotently at startup to add `status`, `priority`, `pr_url`, and `card_comments` to existing `board.db` files, while relaxing the `assignee` constraint.
