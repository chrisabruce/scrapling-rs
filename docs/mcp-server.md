# MCP Server

scrapling-rs includes an MCP (Model Context Protocol) server that lets AI agents scrape web pages as part of their workflows. The server communicates via JSON-RPC 2.0 over stdio, which is the standard MCP transport used by Claude, Cursor, and other AI tools.

## Running the server

```bash
cargo install scrapling-mcp
scrapling-mcp
```

The server reads JSON-RPC requests from stdin and writes responses to stdout. AI agents connect to it as an MCP tool provider.

## Available tools

### HTTP scraping

**`get`** fetches a single URL using the HTTP client with browser impersonation. Good for pages that don't require JavaScript rendering.

Parameters:
- `url` (required) — The URL to fetch
- `extraction_type` — `"markdown"` (default), `"html"`, or `"text"`
- `css_selector` — CSS selector to extract specific elements

**`bulk_get`** fetches multiple URLs in sequence. Same parameters as `get` but takes `urls` (array) instead of `url`.

### Browser-based scraping

**`fetch`** and **`bulk_fetch`** use a headless browser to render JavaScript-heavy pages. Suitable for SPAs and pages with dynamic content.

**`stealthy_fetch`** and **`bulk_stealthy_fetch`** use the anti-detection browser with Cloudflare bypass capabilities. Use these for sites with bot protection.

Additional parameters for stealth tools:
- `solve_cloudflare` — Automatically solve Cloudflare Turnstile challenges

### Session management

**`open_session`** creates a persistent browser session that can be reused across multiple fetch calls, avoiding the overhead of launching a new browser each time.

Parameters:
- `session_type` — `"dynamic"` or `"stealthy"`
- `headless` — Run in headless mode (default: true)

**`close_session`** closes a session and frees its resources.

Parameters:
- `session_id` (required) — The session ID from `open_session`

**`list_sessions`** returns all active browser sessions.

## Content extraction

All scraping tools support three output formats via the `extraction_type` parameter:

- **`markdown`** (default) — Converts the HTML to clean Markdown. Script, style, and noscript tags are stripped automatically.
- **`html`** — Returns the raw HTML content.
- **`text`** — Extracts plain text with all markup removed.

The `css_selector` parameter lets you target specific parts of the page before extraction. If the selector matches multiple elements, all are included in the output.

## Example interaction

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
```

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get","arguments":{"url":"https://example.com","extraction_type":"markdown"}}}
```

The server responds with the page content in the requested format, ready for the AI agent to process.
