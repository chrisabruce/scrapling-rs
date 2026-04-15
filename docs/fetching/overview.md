# Fetching Overview

scrapling-rs provides three ways to retrieve web pages, each designed for a different level of site protection. All three return the same `Response` type, so your parsing code stays the same regardless of which fetcher you use.

## Three Fetching Approaches

### Fetcher (HTTP)

A pure HTTP client built on [wreq](https://github.com/nickel-org/wreq). No browser, no JavaScript execution. It impersonates real browser TLS fingerprints so requests pass basic bot checks.

**Best for:** APIs, static HTML pages, sites with no JavaScript rendering requirement.

```rust
use scrapling_fetch::Fetcher;

let fetcher = Fetcher::new();
let response = fetcher.get("https://example.com", None).await?;
```

### DynamicSession (Browser)

Launches a real Chromium browser via Playwright to render JavaScript-heavy pages. Waits for the DOM to stabilize before returning content.

**Best for:** Single-page applications, pages that load data via AJAX, sites that require JavaScript to render content.

```rust
use scrapling_browser::{BrowserConfig, DynamicSession};

let config = BrowserConfig { headless: true, ..Default::default() };
let mut session = DynamicSession::new(config)?;
session.start().await?;
let response = session.fetch("https://example.com", None).await?;
session.close().await?;
```

### StealthySession (Anti-Detection)

Everything DynamicSession does, plus anti-detection countermeasures: automation flag removal, WebRTC leak prevention, canvas fingerprint noise, and an automatic Cloudflare Turnstile solver.

**Best for:** Sites protected by Cloudflare, DataDome, Akamai, PerimeterX, or similar bot-detection services.

```rust
use scrapling_browser::{StealthConfig, StealthySession};

let config = StealthConfig {
    solve_cloudflare: true,
    block_webrtc: true,
    ..Default::default()
};
let mut session = StealthySession::new(config)?;
session.start().await?;
let response = session.fetch("https://protected-site.com", None).await?;
session.close().await?;
```

## Which One Should I Use?

| Protection Level | Fetcher | Approach |
|---|---|---|
| No protection / static HTML | `Fetcher` | Fastest option, lowest resource usage |
| JavaScript rendering required | `DynamicSession` | Full browser, JS execution |
| Basic bot detection (User-Agent checks) | `Fetcher` with impersonation | TLS fingerprint + stealth headers |
| Cloudflare, DataDome, Akamai | `StealthySession` | Full stealth + challenge solving |
| Login flows, session cookies | `FetcherSession` or `DynamicSession` | Persistent cookies across requests |

Start with `Fetcher`. Move to `DynamicSession` if you need JavaScript. Move to `StealthySession` only when you hit bot detection walls.

## The Response Object

Every fetcher returns a `scrapling_fetch::Response`. It holds the raw HTTP response and provides lazy HTML parsing -- the DOM is only parsed when you first query it.

### Status and Metadata

```rust
let response = fetcher.get("https://example.com", None).await?;

// HTTP status
println!("Status: {} {}", response.status, response.reason);
println!("Success: {}", response.is_success());     // 2xx
println!("Redirect: {}", response.is_redirect());   // 3xx
println!("Client error: {}", response.is_client_error()); // 4xx
println!("Server error: {}", response.is_server_error()); // 5xx

// Final URL (after redirects)
println!("URL: {}", response.url());
```

### Headers and Cookies

```rust
// Response headers
for (name, value) in &response.headers {
    println!("{name}: {value}");
}

// Cookies set by the server
for (name, value) in &response.cookies {
    println!("Cookie: {name}={value}");
}

// Headers that were sent with the request
for (name, value) in &response.request_headers {
    println!("Sent: {name}: {value}");
}
```

### Raw Body

```rust
// Raw bytes
let bytes: &[u8] = &response.body;
println!("Body size: {} bytes", bytes.len());

// Character encoding (from Content-Type header)
println!("Encoding: {}", response.encoding);
```

### Lazy Selector (HTML Parsing)

The HTML is parsed into a `Selector` on first access. Subsequent calls return the cached result.

```rust
// CSS selector queries
let titles = response.css("h1.title");
for title in titles.iter() {
    println!("{}", title.text());
}

// Direct access to the parsed selector
let selector = response.selector();

// Find elements by text content
let links = response.find_by_text("click here", true, false, true);

// Resolve relative URLs
let absolute = response.urljoin("/path/to/page");
```

### Content Conversion

```rust
// Convert HTML to Markdown (useful for LLM input)
let markdown = response.to_markdown();

// Convert HTML to plain text (strips all tags)
let text = response.to_text();
```

## Parser Configuration

Both `Fetcher` and `FetcherSession` accept a `ParserConfig` that controls how the HTML parser behaves on returned responses.

```rust
use scrapling_fetch::{Fetcher, ParserConfig};

let mut fetcher = Fetcher::new();
fetcher.set_parser_config(ParserConfig {
    adaptive: true,
    adaptive_domain: "example.com".to_string(),
});
```

| Field | Type | Default | Description |
|---|---|---|---|
| `adaptive` | `bool` | `false` | Enable adaptive parsing that remembers page structure from prior crawls |
| `adaptive_domain` | `String` | `""` | Scope adaptive parsing to a specific domain to prevent cross-site interference |

Adaptive parsing stores structural fingerprints of pages you have seen before. When the site changes its HTML layout, the parser uses those fingerprints to relocate elements even if their CSS selectors have changed.
