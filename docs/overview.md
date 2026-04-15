# Getting Started with scrapling-rs

scrapling-rs is a Rust web scraping framework, ported from the Python [scrapling](https://github.com/camoufox/scrapling) library. It gives you fast HTML parsing, CSS selection with pseudo-element support, adaptive element relocation, HTTP fetching with browser fingerprint impersonation, headless browser automation, and a full crawl engine -- all as composable crates in a single workspace.

## Pick your path

scrapling-rs is organized into separate crates so you only pull in what you need.

| I want to... | Crate | Install |
|---|---|---|
| Parse HTML I already have | `scrapling` | `cargo add scrapling` |
| Fetch a page over HTTP | `scrapling-fetch` | `cargo add scrapling-fetch` |
| Render JavaScript with a headless browser | `scrapling-browser` | `cargo add scrapling-browser` |
| Crawl a site (scheduler, dedup, robots.txt) | `scrapling-spider` | `cargo add scrapling-spider` |
| Quick scraping from the terminal | `scrapling-cli` | `cargo install scrapling-cli` |

Every crate above the core re-exports `scrapling`, so you always have access to `Selector`, `TextHandler`, and friends.

## 30-second examples

### Parse HTML you already have

```rust
use scrapling::selector::Selector;

let html = r#"
    <div class="products">
        <div class="item"><span class="name">Widget</span><span class="price">$9.99</span></div>
        <div class="item"><span class="name">Gadget</span><span class="price">$14.50</span></div>
    </div>
"#;

let page = Selector::from_html(html);
let names = page.css("span.name::text");
for name in &names {
    println!("{}", name.text());
}
// Widget
// Gadget
```

### Fetch a page

```rust,ignore
use scrapling_fetch::Fetcher;

#[tokio::main]
async fn main() -> scrapling_fetch::Result<()> {
    let fetcher = Fetcher::new();
    let response = fetcher.get("https://example.com", None).await?;

    let title = response.css("title");
    if let Some(t) = title.first() {
        println!("Page title: {}", t.text());
    }
    Ok(())
}
```

### Automate a browser

```rust,ignore
use scrapling_browser::{BrowserConfig, DynamicSession};

#[tokio::main]
async fn main() -> scrapling_browser::Result<()> {
    let config = BrowserConfig {
        headless: true,
        disable_resources: true,
        ..Default::default()
    };

    let mut session = DynamicSession::new(config)?;
    session.start().await?;

    let response = session.fetch("https://example.com", None).await?;
    println!("status: {}", response.status);

    session.close().await?;
    Ok(())
}
```

### Build a crawler

```rust,ignore
use scrapling_spider::{Spider, CrawlerEngine, Request, SpiderOutput};
use scrapling_fetch::Response;

struct ProductScraper;

impl Spider for ProductScraper {
    fn name(&self) -> &str { "products" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com/products".into()]
    }

    fn parse(&self, response: Response) -> Vec<SpiderOutput> {
        let items = response.css("div.product");
        for item in &items {
            let name = item.css("h2::text").get_first();
            let price = item.css("span.price::text").get_first();
            println!("{:?} - {:?}", name, price);
        }
        vec![]
    }
}

#[tokio::main]
async fn main() {
    let spider = ProductScraper;
    let mut engine = CrawlerEngine::new(&spider, None, 0.0).unwrap();
    let stats = engine.crawl().await.unwrap();
    println!("Crawled {} pages, scraped {} items", stats.pages_crawled, stats.items_scraped);
}
```

### CLI

```bash
# Fetch a page and save as HTML
scrapling extract get https://example.com output.html

# Extract specific elements with a CSS selector
scrapling extract get https://example.com output.html -s "h1, p"

# POST with JSON body
scrapling extract post https://api.example.com response.json -j '{"key":"value"}'
```

## Crate feature flags

The core `scrapling` crate has one optional feature:

| Flag | Default | What it enables |
|---|---|---|
| `storage` | on | SQLite-backed persistent element storage for adaptive scraping (via `rusqlite`). |

Disable it if you only need parsing and want to avoid the SQLite dependency:

```toml
[dependencies]
scrapling = { version = "0.1", default-features = false }
```

## Minimum supported Rust version

scrapling-rs requires **Rust 1.85** or later (edition 2024).

## Links

- [Python scrapling](https://github.com/camoufox/scrapling) -- the original project this is ported from
- [scrapling-rs repository](https://github.com/chrisabruce/scrapling-rs)
