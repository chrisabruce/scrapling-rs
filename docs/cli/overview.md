# Command Line Interface

scrapling-rs ships with a `scrapling` CLI binary for quick scraping tasks without writing Rust code.

## Installation

```bash
cargo install scrapling-cli
```

## Commands

### Extract: Fetch and save web content

The `extract` command group makes HTTP requests and saves the response to a file.

```bash
# GET a page and save as HTML
scrapling extract get https://example.com page.html

# Extract only specific elements
scrapling extract get https://example.com titles.html -s "h1, h2, h3"

# Save as plain text
scrapling extract get https://example.com content.txt

# POST with JSON body
scrapling extract post https://api.example.com/data response.json -j '{"query": "rust"}'

# Custom headers and cookies
scrapling extract get https://example.com page.html \
  -H "Authorization: Bearer token123" \
  --cookies "session=abc; theme=dark"
```

#### Supported HTTP methods

| Command | Use case |
|---------|----------|
| `extract get` | Fetch pages, download content |
| `extract post` | Submit forms, API calls with body |
| `extract put` | Update resources |
| `extract delete` | Remove resources |

#### Common options

| Flag | Description | Default |
|------|-------------|---------|
| `-s, --css-selector` | Extract only elements matching this selector | All content |
| `-H, --header` | Add a custom header (repeatable) | None |
| `-p, --param` | Add a query parameter (repeatable) | None |
| `--cookies` | Cookie string (`name=value; name2=value2`) | None |
| `--proxy` | Proxy URL | None |
| `--timeout` | Request timeout in seconds | 30 |
| `--impersonate` | Browser to impersonate (comma-separated for random) | chrome |
| `--stealthy-headers` | Inject real browser headers | true |
| `--verify` | Verify TLS certificates | true |
| `--follow-redirects` | Follow HTTP redirects | true |

#### POST/PUT data options

| Flag | Description |
|------|-------------|
| `-j, --json` | JSON string for the request body |
| `-d, --data` | Raw form data for the request body |

#### Output formats

The output format is determined by the file extension:

- `.html` — Raw HTML response
- `.txt` — Plain text (markup stripped)
- `.json` — Structured JSON

### Info: Show build information

```bash
scrapling info
```

Shows which scrapling-rs components are compiled in and their versions.

### Shell

```bash
scrapling shell
```

Interactive scraping REPL (placeholder — use the Python version for now).

## Examples

```bash
# Scrape product names from an e-commerce page
scrapling extract get https://shop.example.com/products items.txt -s ".product-name"

# Fetch with Firefox impersonation through a proxy
scrapling extract get https://example.com page.html \
  --impersonate firefox \
  --proxy http://proxy:8080

# POST JSON to an API and save the response
scrapling extract post https://api.example.com/search result.json \
  -j '{"q": "web scraping", "limit": 10}' \
  -H "Content-Type: application/json"
```
