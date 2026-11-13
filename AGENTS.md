# AGENTS.md

Instructions for AI coding agents (Claude Code, Codex, Cursor, Copilot, …) working on this repository. Read `README.md` for what the product does. This file covers how to change it safely.

## Project in one paragraph

Ship Board is a real-time Kanban board: a Rust backend and a SolidJS frontend. A single Axum process serves:
- the REST API (`/api/*`)
- a WebSocket (`/ws`) that sends every change
- a Telegram long-polling bot
- an MCP server (`/mcp`)

All four write through the same service layer to one SQLite file and send events on one in-process broadcast channel. The frontend is optimistic: it updates locally, syncs over REST, and applies other clients' changes from the WebSocket.

## Commands

Run these from the repo root. All must pass before you call a task done.

```bash
cd backend && cargo check            # must print 0 warnings, not just 0 errors
cd backend && cargo test             # unit tests live in telegram.rs and mcp.rs
cd frontend && npm run typecheck     # tsc --noEmit
cd frontend && npm run build         # tsc + vite build
docker build -t shipmd .             # when you touch Dockerfile, deps or build config
```

Run locally with `cd backend && cargo run` (port 3001) and `cd frontend && npm run dev` (port 5173, proxies `/api` and `/ws`). Use `BOARD_DB=/tmp/x.db` for a throwaway database. The database is created and seeded when the file doesn't exist.

## Where things live

| Path | Responsibility |
|---|---|
| `backend/src/service.rs` | **The only place card data changes.** Validates, writes SQLite, calls `state.emit(...)`. Also `board()`, `shipped_ratio()`, `resolve_card()` (prefix lookup). |
| `backend/src/routes.rs` | Thin HTTP adapters: extract `X-Client-Id` as `origin`, call `service`, shape the response. |
| `backend/src/telegram.rs` | Bot: `Telegram` handle, `run()` polling loop, command and callback handlers, `notify_*` helpers, rendering. |
| `backend/src/mcp.rs` | MCP: `router()`, auth middleware, Streamable HTTP and SSE transports, JSON-RPC dispatch, `tool_definitions()`, tool functions. |
| `backend/src/ws.rs` | Forwards broadcast strings to sockets, heartbeat ping, sends `RESYNC` when a client lags. |
| `backend/src/db.rs` | Pool and WAL options, schema (`CREATE TABLE IF NOT EXISTS`), seed data, `COLUMNS`. |
| `backend/src/models.rs` | Row types (`Card`, `Column`) and request types (`CreateCard`, `PatchCard`, `AgentAction`, `Tag`, `Assignee`). |
| `backend/src/main.rs` | `AppState`, `emit()`, `now()`, router wiring, static frontend (`STATIC_DIR`), Telegram spawn. |
| `backend/src/auth.rs` | Optional `BOARD_PASSWORD` guard: Basic auth, then an HttpOnly session cookie. Also `constant_time_eq`, which MCP reuses. |
| `Dockerfile`, `fly.toml` | Single image (frontend build + Rust binary) and Fly.io config: one always-on machine, volume at `/data`. |
| `frontend/src/store.ts` | Global Solid store, optimistic mutations with rollback, WebSocket client, FLIP animation for remote moves. |
| `frontend/src/dnd.ts` | Drag and drop state (`createSelector`-based) and drop-position math. |
| `frontend/src/components/*` | `Header` (ratio bar, theme), `BoardColumn`, `CardItem`, `Composer`, `CardModal`. |

## Rules that must hold

### Backend

1. **Every card change goes through `service.rs`.** Don't write SQL or call `state.emit` from `routes.rs`, `telegram.rs` or `mcp.rs`. Need a new operation? Add it to `service.rs` and call it from each entry point.
2. **Every change broadcasts exactly one event:**
   - `CARD_CREATED { card }`
   - `CARD_MOVED { card }` when `column_id` or `position` changed
   - `CARD_UPDATED { card }` for any other change
   - `CARD_DELETED { id }`

   Each event also has a `type` and an `origin`. The frontend in `store.ts → applyEvent` depends on these exact shapes.
3. **`origin` meaning:** a browser tab's `X-Client-Id`, `"telegram"`, or `"mcp"`. A tab ignores events carrying its own id, because it already applied them optimistically. Never reuse a tab's id as a server origin.
4. **Columns are fixed:** `backlog`, `build`, `sell`, `done`. `db::COLUMNS` is the source of truth, and `service::DONE_COLUMN` names Done. The frontend treats the *last* column as Done for the ratio bar.
5. **Positions** are `f64` with a gap of 1024. Inserting between two cards uses their midpoint. When no position is given, new cards go to the end, but moves into Done go to the top (Done is newest first).
6. **Queries are checked at runtime, not compile time.** Use `sqlx::query` / `query_as::<_, T>` and `#[derive(FromRow)]`. Don't use `query!` macros: the build must not need `DATABASE_URL`.
7. **Schema changes:** there is no migration system. `CREATE TABLE IF NOT EXISTS` won't change existing `board.db` files. If you change the schema, add an explicit, idempotent migration step in `db.rs` and say so in your summary.
8. **Keep Telegram replies tidy.**
   - Inline-button `callback_data` must be ≤ 64 bytes. A test enforces this for the UUID-based formats.
   - Escape all user text with `telegram::esc` (messages use `parse_mode: HTML`).
   - Prefer editing the existing message over sending a new one.
9. **Never log secrets.** `TgError` strips request URLs because they contain the bot token. Keep it that way.
10. **MCP:**
    - A tool failure returns a normal result with `isError: true`, so the model can read it and retry. Use JSON-RPC errors only for protocol problems such as an unknown method or tool.
    - Inputs accept column titles or ids case-insensitively, and full or 8-char short card ids.
    - If you add or change a tool, update `tool_definitions()` (strict JSON Schema, `additionalProperties: false`), the `tool_schemas_are_objects` test count, and the README tool table.
11. **Router wiring:** board routes (`/api/*`, `/ws`, the static fallback) are one router wrapped in `auth::require`. `/healthz` stays outside it and must remain public for Fly health checks. `mcp::router()` brings its own state and auth, and is `.merge()`d after `.with_state(state)`. CORS goes last. The server must be started with `into_make_service_with_connect_info::<SocketAddr>()`, because the MCP auth guard reads the peer address. A new board route must sit inside the guarded router.
12. **Single instance.** Don't add anything that assumes more than one backend process: the broadcast channel is in memory, and Telegram's `getUpdates` allows only one poller.

### Frontend

1. **Change cards in place** with `setBoard('cards', id, {...})`. Never replace the whole `cards` object or a card object, except through `reconcile` in `loadBoard`. `<For>` keys on object identity, and replacing objects re-renders the board.
2. **Follow the optimistic pattern** (`moveCard`, `updateCard`, `createCard`, `deleteCard`): update the store, call the API, roll back and `showToast` on failure. New cards get a client-side UUID so the optimistic card and the server's card are the same object.
3. **Drag and drop must not re-render the board while hovering.** The drop target is one primitive key read through `createSelector`. Keep it that way.
4. **Sanitize Markdown.** Render it only through `renderMarkdown()`, which uses DOMPurify, because agents and Telegram write descriptions too.
5. **Styling:** Tailwind v4 utility classes, with `dark:` variants keyed on the `.dark` class on `<html>`. There's no `tailwind.config`; theme tokens are in `src/index.css`.

## Testing changes end to end

- **Backend over HTTP:** start with a scratch DB, then use `curl` for `/api/*`. To watch events, open `ws://localhost:3001/ws` with any WebSocket client (Node 22 has a global `WebSocket`).
- **Telegram without Telegram:** set `TELEGRAM_BOT_TOKEN=TEST` and `TELEGRAM_API_URL=http://127.0.0.1:<port>`, and point it at a small fake server. The fake must answer `POST /botTEST/<method>` with `{"ok":true,"result":…}` and serve queued updates on `getUpdates`.
- **MCP:** `curl` JSON-RPC to `POST /mcp` (see README). For real client compatibility, test with `@modelcontextprotocol/sdk`'s `StreamableHTTPClientTransport` and `SSEClientTransport`.
- **UI:** Playwright drives native drag and drop (`locator.dragTo`) in Chromium. Open two pages to check real-time sync.

## Using the board yourself (MCP)

If this repo's board is running and connected as an MCP server, work this way: `pick_next_agent_task` → implement → `update_card_notes` (commit hash, PR link, short summary) → `move_card` to `Done`. Never move a card to Done unless the checks above pass.

## Known gaps

The README's "Known limitations" and "Checklist before going public" sections list them. The main ones: the web UI and REST API are open unless `BOARD_PASSWORD` is set, there's no position rebalancing, and `pick_next_agent_task` doesn't claim cards atomically.
