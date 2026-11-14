@AGENTS.md

## Claude Code notes

- `AGENTS.md` (imported above) holds the rules and commands for this repo. Keep shared guidance there, not here.
- Definition of done: `cargo check` with **0 warnings**, `cargo test`, `npm run typecheck` and `npm run build` all pass.
- To use the live board as your task queue, add the MCP server. Put this in `.mcp.json`, or run `claude mcp add --transport http ship-board http://localhost:3001/mcp`:

  ```json
  { "mcpServers": { "ship-board": { "type": "http", "url": "http://localhost:3001/mcp",
      "headers": { "Authorization": "Bearer ${SHIPMD_MCP_TOKEN}" } } } }
  ```

  `SHIPMD_MCP_TOKEN` must be set on the server; requests without a matching Bearer token receive `401 Unauthorized`.
- Never commit `backend/.env`, `*.db` files or tokens. `.gitignore` already covers them.
