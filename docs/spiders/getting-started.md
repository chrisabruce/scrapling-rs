# Getting Started with Spiders

A **spider** is a Rust struct that implements the `Spider` trait. It tells the crawler what to scrape: which URLs to start from, how to parse each response, and what data to extract. The `CrawlerEngine` handles the rest -- scheduling, fetching, deduplication, retries, and concurrency.

This guide walks you through writing your first spider, running it, and exporting the results.

## What is a Spider?

The `Spider` trait lives in `scrapling_spider` and has three required methods:

| Method | Purpose |
|--------|---------|
| `name()` | A human-readable identifier used in log output |
| `start_urls()` | The URLs to begin crawling |
| `parse()` | Receives each HTTP response and returns scraped items and/or follow-up requests |

Everything else -- concurrency limits, download delays, session configuration, domain restrictions -- has a sensible default that you can override as needed.

## Minimal Example

Here is a spider that scrapes quotes from a website:

```rust
use scrapling_spider::{Spider, CrawlerEngine, Request, SpiderOutput};
use scrapling_fetch::Response;
use serde_json::json;

struct QuoteSpider;

impl Spider for QuoteSpider {
    fn name(&self) -> &str {
        "quotes"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com/".into()]
    }

    fn parse(&self, response: Response) -> Vec<SpiderOutput> {
        let mut outputs = Vec::new();

        for quote in response.css("div.quote") {
            let text = quote.css("span.text").text().first();
            let author = quote.css("small.author").text().first();

            outputs.push(SpiderOutput::Item(json!({
                "text": text,
                "author": author,
            })));
        }

        // Follow the "Next" link if it exists
        if let Some(next_href) = response.css("li.next a").first().and_then(|el| el.attr("href")) {
            let next_url = response.follow_url(&next_href);
            outputs.push(SpiderOutput::FollowRequest(Request::new(next_url)));
        }

        outputs
    }
}
```

Three things happen in `parse`:

1. CSS selectors extract text and author from each quote on the page.
2. Each quote becomes a `SpiderOutput::Item` containing a `serde_json::Value`.
3. If there is a "Next" link, the spider returns a `SpiderOutput::FollowRequest` so the engine fetches the next page.

## Running with CrawlerEngine

`CrawlerEngine` is the runtime that drives the crawl loop. Create one, hand it your spider, and call `crawl()`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spider = QuoteSpider;
    let mut engine = CrawlerEngine::new(&spider, None, 0.0)?;
    let stats = engine.crawl().await?;

    println!("Scraped {} items in {:.1}s", stats.items_scraped, stats.elapsed_seconds());
    Ok(())
}
```

The two arguments after the spider reference are:

- `crawldir` -- an optional `PathBuf` for checkpoint-based pause/resume. Pass `None` to disable checkpointing.
- `interval_secs` -- how often (in seconds) to auto-save checkpoints during the crawl. Use `0.0` when checkpointing is disabled.

`crawl()` is an async method that runs until the scheduler is empty and all in-flight requests finish, then returns `CrawlStats`.

## CrawlStats

`CrawlStats` is a serializable struct containing everything you might want to know about the crawl:

```rust
let stats = engine.crawl().await?;

println!("Requests:   {}", stats.requests_count);
println!("Failed:     {}", stats.failed_requests_count);
println!("Blocked:    {}", stats.blocked_requests_count);
println!("Items:      {}", stats.items_scraped);
println!("Dropped:    {}", stats.items_dropped);
println!("Bytes:      {}", stats.response_bytes);
println!("Duration:   {:.2}s", stats.elapsed_seconds());
println!("Req/s:      {:.1}", stats.requests_per_second());
```

It also includes per-domain byte counts (`domains_response_bytes`), per-session request counts (`sessions_requests_count`), and HTTP status code breakdowns (`response_status_count`). Since `CrawlStats` derives `Serialize`, you can write it to JSON alongside your data for post-crawl analysis.

## ItemList

The engine collects every `SpiderOutput::Item` into an `ItemList`, which you access via `engine.items()`. It behaves like a `Vec<serde_json::Value>` with a few extras:

```rust
let items = engine.items();

println!("Collected {} items", items.len());

// Iterate over items
for item in items {
    println!("{}", item);
}

// Index into items directly
if !items.is_empty() {
    println!("First item: {}", items[0]);
}
```

## Exporting to JSON and JSONL

`ItemList` has built-in methods for writing scraped data to disk:

```rust
use std::path::Path;

let items = engine.items();

// JSON array (pretty-printed)
items.to_json(Path::new("output/quotes.json"), true)?;

// JSON array (compact)
items.to_json(Path::new("output/quotes.min.json"), false)?;

// JSON Lines (one object per line)
items.to_jsonl(Path::new("output/quotes.jsonl"))?;
```

Both methods create parent directories automatically. JSON Lines is the better choice for large datasets because each line is a self-contained document that can be streamed into data pipelines without loading the entire file into memory.

## CrawlResult

If you need the items and stats together in one value, `CrawlResult` bundles them:

```rust
let result = CrawlResult {
    stats,
    items: engine.items().clone(),
    paused: engine.paused,
};

if result.completed() {
    println!("Crawl finished with {} items", result.len());
} else {
    println!("Crawl was paused -- resume later from checkpoint");
}
```

## Putting It All Together

A complete, runnable spider:

```rust
use std::path::Path;

use scrapling_spider::{Spider, CrawlerEngine, Request, SpiderOutput};
use scrapling_fetch::Response;
use serde_json::json;

struct QuoteSpider;

impl Spider for QuoteSpider {
    fn name(&self) -> &str { "quotes" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com/".into()]
    }

    fn parse(&self, response: Response) -> Vec<SpiderOutput> {
        let mut outputs = Vec::new();

        for quote in response.css("div.quote") {
            let text = quote.css("span.text").text().first();
            let author = quote.css("small.author").text().first();

            outputs.push(SpiderOutput::Item(json!({
                "text": text,
                "author": author,
            })));
        }

        if let Some(next) = response.css("li.next a").first().and_then(|el| el.attr("href")) {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&next))
            ));
        }

        outputs
    }

    fn concurrent_requests(&self) -> u32 { 2 }
    fn download_delay(&self) -> f64 { 0.5 }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spider = QuoteSpider;
    let mut engine = CrawlerEngine::new(&spider, None, 0.0)?;
    let stats = engine.crawl().await?;

    engine.items().to_jsonl(Path::new("quotes.jsonl"))?;
    println!("Done: {} items, {:.1}s", stats.items_scraped, stats.elapsed_seconds());
    Ok(())
}
```

## Next Steps

- [Architecture](architecture.md) -- understand the crawl loop and component interactions
- [Session Management](sessions.md) -- configure HTTP and browser sessions
- [Requests and Responses](requests-responses.md) -- the Request builder and Response API
- [Advanced Features](advanced.md) -- concurrency, checkpointing, streaming, proxies, and hooks
