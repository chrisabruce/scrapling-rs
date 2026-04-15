# Advanced Features

This document covers the features you reach for once your basic spider is working: concurrency tuning, robots.txt compliance, pause/resume checkpointing, streaming mode, development caching, lifecycle hooks, proxy rotation, and statistics.

## Concurrency Control

### Global Concurrency

`concurrent_requests()` sets the maximum number of requests the engine dispatches at the same time. The default is 4:

```rust
impl Spider for MySpider {
    fn concurrent_requests(&self) -> u32 { 16 }
    // ...
}
```

Higher values speed up crawls but put more load on target servers. Start conservative and increase after confirming the site handles it.

### Per-Domain Concurrency

`concurrent_requests_per_domain()` limits how many requests target any single host at once. The default is 0, meaning unlimited (constrained only by the global limit):

```rust
impl Spider for MySpider {
    fn concurrent_requests(&self) -> u32 { 16 }
    fn concurrent_requests_per_domain(&self) -> u32 { 4 }
    // ...
}
```

This is useful when crawling multiple domains simultaneously -- you can allow high overall throughput while being polite to each individual host.

### Download Delay

`download_delay()` inserts a pause (in seconds) between consecutive request dispatches:

```rust
impl Spider for MySpider {
    fn download_delay(&self) -> f64 { 1.5 }
    // ...
}
```

The engine sleeps for this duration after dispatching each request. A delay of `0.0` (the default) means no pause.

All three values are recorded in `CrawlStats` so you can verify what was active during a given crawl run.

## Robots.txt Compliance

Enable robots.txt enforcement by returning `true` from `robots_txt_obey()`:

```rust
impl Spider for MySpider {
    fn robots_txt_obey(&self) -> bool { true }
    // ...
}
```

When enabled, the engine:

1. Prefetches `robots.txt` for every domain in `start_urls()` before the crawl loop begins.
2. Checks each request's URL against the domain's `Disallow` directives before fetching.
3. Silently drops disallowed URLs and increments `CrawlStats::robots_disallowed_count`.

The parser reads rules under the `User-agent: *` section. Domains whose `robots.txt` cannot be fetched or parsed are treated as "allow all." The `Crawl-delay` directive is parsed and available via the robots.txt manager, though the engine does not automatically apply it (use `download_delay()` for that).

Each domain's rules are cached for the duration of the crawl, so `robots.txt` is fetched at most once per domain.

## Pause/Resume with Checkpointing

Long-running crawls can be interrupted and resumed without losing progress. Enable checkpointing by passing a directory path and interval to `CrawlerEngine::new()`:

```rust
use std::path::PathBuf;

let crawldir = Some(PathBuf::from("./crawl_state"));
let interval_secs = 60.0;  // auto-save every 60 seconds

let mut engine = CrawlerEngine::new(&spider, crawldir, interval_secs)?;
```

### How It Works

A checkpoint is a JSON file (`checkpoint.json`) in the crawl directory containing:

- The URLs of all pending requests still in the scheduler's queue.
- The SHA-1 fingerprints of every request that has been seen (for dedup restoration).

Checkpoints are written atomically (temp file + rename) so a crash mid-write cannot corrupt the file.

### When Checkpoints Are Saved

- **Periodically** -- every `interval_secs` seconds during the crawl loop (set to `0.0` to disable periodic saves).
- **On pause** -- when `request_pause()` is called.

### Pausing a Crawl

Call `request_pause()` on the engine to initiate a graceful wind-down:

```rust
engine.request_pause();
```

The first call lets in-flight requests finish, then saves a checkpoint and exits the loop. Calling it a second time triggers a force stop that abandons in-flight requests immediately.

After `crawl()` returns, check `engine.paused` to determine whether the crawl completed or was interrupted.

### Resuming a Crawl

Create a new engine pointed at the same `crawldir`. The engine automatically detects the checkpoint file, restores the queue and seen set, and continues from where it left off:

```rust
let mut engine = CrawlerEngine::new(
    &spider,
    Some(PathBuf::from("./crawl_state")),
    60.0,
)?;
let stats = engine.crawl().await?;
// The spider's on_start(true) fires, indicating a resume
```

When the crawl completes normally (not paused), the checkpoint file is deleted automatically.

## Streaming Mode

For pipelines that need to process items as they arrive rather than waiting for the crawl to finish, use `stream()`:

```rust
let mut engine = CrawlerEngine::new(&spider, None, 0.0)?;
let mut rx = engine.stream();

// Spawn the crawl in the background
let crawl_handle = tokio::spawn(async move {
    engine.crawl().await
});

// Process items as they are scraped
while let Some(item) = rx.recv().await {
    println!("Got: {}", item);
    // write to database, send to queue, etc.
}

let stats = crawl_handle.await??;
```

Call `stream()` before `crawl()`. It returns an unbounded `tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>`. Each item passes through `on_scraped_item()` and is sent to both the receiver and the internal `ItemList`.

## Development Mode

When iterating on parse logic, you do not want to hit the network on every test run. Enable development mode to cache responses to disk:

```rust
impl Spider for MySpider {
    fn development_mode(&self) -> bool { true }

    // Optional: customize the cache directory (default: ".scrapling_cache")
    fn development_cache_dir(&self) -> Option<PathBuf> {
        Some(PathBuf::from("./dev_cache"))
    }
    // ...
}
```

On the first run, every response is saved as a JSON file (with the body base64-encoded) keyed by the request's fingerprint. On subsequent runs, cached responses are served directly without network I/O.

Cache statistics are tracked in `CrawlStats`:

```rust
println!("Cache hits:   {}", stats.cache_hits);
println!("Cache misses: {}", stats.cache_misses);
```

The cache does not implement expiration or size limits. It is not intended for production use. To force a fresh crawl, delete the cache directory.

## Lifecycle Hooks

The `Spider` trait provides hooks that fire at specific points during the crawl. All have default no-op implementations.

### on_start

Called once when the crawl begins. The `resuming` parameter is `true` if restoring from a checkpoint:

```rust
fn on_start(&self, resuming: bool) {
    if resuming {
        println!("Resuming crawl from checkpoint");
    } else {
        println!("Starting fresh crawl");
    }
}
```

### on_close

Called once when the crawl finishes or is paused. Use this for cleanup:

```rust
fn on_close(&self) {
    println!("Crawl finished, flushing buffers");
    // close database connections, flush write buffers, etc.
}
```

### on_error

Called when a request fails with a network error (timeout, DNS failure, connection refused, etc.):

```rust
fn on_error(&self, request: &Request, error: &SpiderError) {
    eprintln!("Failed to fetch {}: {}", request.url, error);
}
```

This fires for fetch-level errors, not for blocked responses (those are handled by `is_blocked` and the retry mechanism).

### on_scraped_item

Called for each item before it is added to the `ItemList`. Return `Some(item)` to keep it, or `None` to drop it:

```rust
fn on_scraped_item(&self, item: serde_json::Value) -> Option<serde_json::Value> {
    // Drop items without a title
    if item.get("title").and_then(|t| t.as_str()).is_none() {
        return None;
    }

    // Add a timestamp to every item
    let mut item = item;
    item["scraped_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    Some(item)
}
```

Dropped items are counted in `CrawlStats::items_dropped`.

### is_blocked

Determines whether a response indicates the request was blocked by the server. The default checks against a built-in list of status codes (401, 403, 407, 429, 444, 500, 502, 503, 504):

```rust
fn is_blocked(&self, response: &Response) -> bool {
    // Default behavior plus CAPTCHA page detection
    let default_blocked = [401, 403, 407, 429, 444, 500, 502, 503, 504];
    if default_blocked.contains(&response.status) {
        return true;
    }

    // Check for CAPTCHA in the response body
    let text = String::from_utf8_lossy(&response.body);
    text.contains("captcha") || text.contains("verify you are human")
}
```

When a response is detected as blocked and the retry count is under `max_blocked_retries()` (default 3), the engine re-enqueues the request with lower priority and `dont_filter: true`.

## Allowed Domains

Restrict the crawl to specific domains by overriding `allowed_domains()`:

```rust
fn allowed_domains(&self) -> HashSet<String> {
    HashSet::from([
        "example.com".into(),
        "shop.example.com".into(),
    ])
}
```

Requests targeting domains not in this set are silently dropped and counted in `CrawlStats::offsite_requests_count`. An empty set (the default) means all domains are allowed. Subdomains are matched automatically -- including `"example.com"` also allows `"shop.example.com"`.

## ProxyRotator Integration

The `ProxyRotator` from `scrapling_fetch` cycles through a pool of proxy servers:

```rust
use scrapling_fetch::proxy::{Proxy, ProxyRotator};

let proxies = vec![
    Proxy::Url("http://proxy1.example.com:8080".into()),
    Proxy::Url("http://proxy2.example.com:8080".into()),
    Proxy::Config {
        server: "http://proxy3.example.com:8080".into(),
        username: Some("user".into()),
        password: Some("pass".into()),
    },
];

let rotator = ProxyRotator::new(proxies)?;
```

The rotator cycles sequentially by default (proxy 1, then 2, then 3, then back to 1). You can supply a custom `RotationStrategy` function for random, weighted, or geo-aware selection:

```rust
use scrapling_fetch::proxy::{Proxy, ProxyRotator, RotationStrategy};

fn random_rotation(proxies: &[Proxy], _current: usize) -> usize {
    rand::random::<usize>() % proxies.len()
}

let rotator = ProxyRotator::with_strategy(proxies, random_rotation)?;
```

The `is_proxy_error()` helper inspects error messages to identify proxy-related failures (connection refused, tunnel failed, etc.), which is useful for retry logic.

Proxy addresses used during a crawl are recorded in `CrawlStats::proxies`.

## Statistics and Logging

### CrawlStats

After a crawl, `CrawlStats` provides a comprehensive breakdown:

```rust
let stats = engine.crawl().await?;

// Request counters
println!("Total requests:     {}", stats.requests_count);
println!("Failed:             {}", stats.failed_requests_count);
println!("Blocked:            {}", stats.blocked_requests_count);
println!("Offsite (dropped):  {}", stats.offsite_requests_count);
println!("Robots disallowed:  {}", stats.robots_disallowed_count);

// Item counters
println!("Items scraped:      {}", stats.items_scraped);
println!("Items dropped:      {}", stats.items_dropped);

// Timing
println!("Duration:           {:.2}s", stats.elapsed_seconds());
println!("Requests/sec:       {:.1}", stats.requests_per_second());

// Bandwidth
println!("Total bytes:        {}", stats.response_bytes);
for (domain, bytes) in &stats.domains_response_bytes {
    println!("  {}: {} bytes", domain, bytes);
}

// Status codes
for (status, count) in &stats.response_status_count {
    println!("  {}: {}", status, count);
}

// Per-session breakdown
for (sid, count) in &stats.sessions_requests_count {
    println!("  Session '{}': {} requests", sid, count);
}

// Cache performance (development mode)
println!("Cache hits:         {}", stats.cache_hits);
println!("Cache misses:       {}", stats.cache_misses);
```

`CrawlStats` derives `Serialize` and `Deserialize`, so you can persist it to JSON:

```rust
let json = serde_json::to_string_pretty(&stats)?;
std::fs::write("crawl_stats.json", json)?;
```

### Custom Statistics

Use the `custom_stats` map to track domain-specific metrics:

```rust
// During the crawl (via on_scraped_item or similar)
stats.custom_stats.insert("products_found".into(), serde_json::json!(42));
```

### LogCounter

The `LogCounter` provides a thread-safe tally of log messages by level:

```rust
use scrapling_spider::logging::LogCounter;

let counter = LogCounter::new();

// Increment as log events occur
counter.increment(tracing::Level::INFO);
counter.increment(tracing::Level::WARN);
counter.increment(tracing::Level::ERROR);

// Get the final counts
let counts = counter.counts();
// {"debug": 0, "info": 1, "warning": 1, "error": 1}
```

`LogCounter` uses atomic integers and is safe to share across threads without a mutex. Assign its output to `CrawlStats::log_levels_counter` to include log-level breakdowns in your crawl report.

## Fingerprint Tuning

By default, the scheduler deduplicates requests based on session ID, HTTP method, and URL (with fragments stripped). Three spider methods let you adjust what goes into the fingerprint:

```rust
impl Spider for MySpider {
    // Include request body in fingerprints.
    // Needed when the same URL accepts different POST bodies.
    fn fp_include_kwargs(&self) -> bool { true }

    // Include HTTP headers in fingerprints.
    // Needed when headers change the response (e.g., Accept-Language).
    fn fp_include_headers(&self) -> bool { true }

    // Keep URL fragments in fingerprints.
    // Needed when #fragment changes the content (single-page apps).
    fn fp_keep_fragments(&self) -> bool { true }

    // ...
}
```

## Custom Start Requests

`start_requests()` wraps each URL from `start_urls()` in a plain `Request` by default. Override it to attach priorities, metadata, or callbacks to your initial requests:

```rust
fn start_requests(&self) -> Vec<Request> {
    self.start_urls()
        .into_iter()
        .enumerate()
        .map(|(i, url)| {
            Request::new(url)
                .with_priority(100 - i as i32)
                .with_meta(HashMap::from([
                    ("source".into(), serde_json::json!("seed")),
                ]))
        })
        .collect()
}
```

## Next Steps

- [Getting Started](getting-started.md) -- write your first spider
- [Architecture](architecture.md) -- understand the crawl loop internals
- [Session Management](sessions.md) -- configure HTTP and browser sessions
- [Requests and Responses](requests-responses.md) -- the Request builder and Response API
