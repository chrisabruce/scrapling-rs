# scrapling-rs

The Rust port of [Scrapling](https://github.com/D4Vinci/Scrapling), a web scraping framework that actually handles the messy reality of modern websites. Built for speed, built for stealth, built to keep working when sites change their HTML.

If you've used the Python version, you already know the API. If you haven't, here's the short version: Scrapling finds elements even after a website redesigns, impersonates real browsers so anti-bot systems can't tell you apart, and does it all fast enough to crawl thousands of pages concurrently.

This Rust port takes everything that makes Scrapling good and removes the performance ceiling. No GIL. No garbage collector. Native async. Single binary deployment.

## What makes this different

Most scraping libraries break the moment a website changes a CSS class or moves a div. Scrapling doesn't. It saves a structural fingerprint of every element you care about and uses a 12-factor similarity algorithm to find it again, even when the surrounding HTML looks completely different. That's the adaptive engine, and it's the reason people use Scrapling over everything else.

The other big thing: real browser fingerprint impersonation. Not just setting a User-Agent header. Full TLS fingerprint emulation (JA3/JA4, HTTP/2 settings, cipher order) through 135+ browser profiles so anti-bot systems see Chrome, Firefox, or Safari instead of a Rust HTTP client.

## Features

**HTML parsing and selection**
- Fast DOM parsing via html5ever with CSS selector support, including `::text` and `::attr()` pseudo-elements
- Full DOM navigation: parent, children, siblings, ancestors, descendants
- Find elements by text content, regex patterns, or compound filters
- Auto-generate unique CSS and XPath selectors for any element

**Adaptive element relocation**
- 12-factor structural similarity scoring (tag, text, attributes, path, parent, siblings, and more)
- Survives DOM restructuring, class renames, ID changes, and wrapper element additions
- SQLite-backed fingerprint storage across scraping sessions

**HTTP fetching with browser impersonation**
- 135+ browser emulation profiles (Chrome, Firefox, Safari, Edge, Opera, OkHttp) via wreq
- TLS fingerprint impersonation (JA3/JA4/HTTP2 APERT)
- Proxy rotation with pluggable strategies
- Automatic retry with configurable backoff
- Stealth headers with Google referer injection

**Browser automation**
- Playwright-based headless browser control
- 99 Chromium stealth flags for anti-detection
- Cloudflare Turnstile solver (non-interactive, managed, interactive, embedded challenges)
- Resource and ad blocking (3,527 domain blocklist)
- Network interception with domain suffix matching

**Spider framework**
- Concurrent crawler with configurable parallelism
- Request deduplication via SHA-1 fingerprinting
- Robots.txt compliance with crawl-delay support
- Checkpoint/resume for long-running crawls
- Development mode with response caching

**Extras**
- CLI for quick extraction jobs
- MCP server for AI agent integration
- Python bindings via PyO3
- Curl command parser (paste from DevTools, get a request)
- HTML to Markdown and plain text conversion

## Quick start

```rust
use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <h1 class="title">Hello, Scrapling!</h1>
            <div class="products">
                <div class="product" data-id="1"><span class="price">$10.99</span></div>
                <div class="product" data-id="2"><span class="price">$24.99</span></div>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // CSS selectors with pseudo-elements
    let prices = page.css(".price::text");
    for price in prices.iter() {
        println!("{}", price.text());
    }

    // Extract structured data
    for product in page.css(".product").iter() {
        let id = &product.attrib()["data-id"];
        let price = product.css(".price").first().unwrap().text();
        println!("Product {id}: {price}");
    }

    // Find elements by text
    let matches = page.find_by_text("$10", true, false, false);
    println!("Found {} elements containing '$10'", matches.len());
}
```

## HTTP fetching with impersonation

```rust
use scrapling_fetch::{Fetcher, FetcherConfig, Impersonate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fetcher = Fetcher::with_config(FetcherConfig {
        impersonate: Impersonate::Single("chrome".into()),
        stealthy_headers: true,
        ..Default::default()
    });

    let response = fetcher.get("https://example.com", None).await?;
    println!("Status: {}", response.status);

    // Response has full CSS selector support
    let title = response.css("title::text");
    println!("Title: {}", title.first().unwrap().text());

    // Convert to markdown
    println!("{}", response.to_markdown());

    Ok(())
}
```

## Adaptive relocation

```rust
use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

fn main() {
    let storage = SqliteStorage::new(":memory:", Some("https://example.com")).unwrap();

    // Save a fingerprint from the original page
    let page = Selector::from_html(r#"<div id="price" class="amount">$42.99</div>"#);
    page.css_adaptive("#price", &storage, false, true, Some("price"), 0.0);

    // Website redesigns, the ID is gone, class changed
    let new_page = Selector::from_html(r#"<span class="cost" data-type="price">$42.99</span>"#);

    // Normal selector fails
    assert!(new_page.css("#price").is_empty());

    // Adaptive finds it by structural similarity
    let found = new_page.css_adaptive("#price", &storage, true, false, Some("price"), 0.0);
    assert!(!found.is_empty());
}
```

## Project structure

```
scrapling-rs/
├── crates/
│   ├── scrapling/          Core: HTML parsing, selectors, adaptive engine
│   ├── scrapling-fetch/    HTTP client with TLS impersonation (wreq)
│   ├── scrapling-browser/  Playwright browser automation + stealth
│   ├── scrapling-spider/   Concurrent crawler framework
│   ├── scrapling-cli/      Command-line interface
│   ├── scrapling-mcp/      MCP server for AI agents
│   └── scrapling-python/   PyO3 Python bindings
├── examples/               13 runnable examples
├── fuzz/                   Fuzz testing targets
└── .github/workflows/      CI (fmt, clippy, test)
```

## Installation

Add the crates you need:

```toml
[dependencies]
scrapling = "0.1"                           # Core parsing + adaptive
scrapling-fetch = "0.1"                     # HTTP fetching
scrapling-browser = "0.1"                   # Browser automation
scrapling-spider = "0.1"                    # Crawler framework
```

## Examples

Run any of the 13 included examples:

```bash
cargo run -p scrapling-examples --example 01_parse_html
cargo run -p scrapling-examples --example 07_adaptive
cargo run -p scrapling-examples --example 09_http_fetch
```

## Status

This is a complete port. 279 tests passing, zero clippy warnings.

| Component | Status |
|-----------|--------|
| HTML parsing, DOM traversal, CSS/XPath selectors | Complete |
| Adaptive element relocation with SQLite storage | Complete |
| HTTP fetcher with 135+ browser profiles | Complete |
| Playwright browser automation + Cloudflare solver | Complete |
| Spider framework with checkpointing + robots.txt | Complete |
| CLI, MCP server, Python bindings | Complete |

## Minimum Rust version

1.85 or later.

## Credits

This project is a Rust port of [Scrapling](https://github.com/D4Vinci/Scrapling) by [Karim Shoair](https://github.com/D4Vinci). The original architecture, API design, adaptive algorithms, and anti-detection strategies all come from the Python project. This port exists because those ideas deserved native performance.

## License

MIT
