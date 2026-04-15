# Requests and Responses

`Request` and `Response` are the two types you interact with most in a spider's `parse()` method. A `Request` tells the engine what to fetch next; a `Response` gives you the result to extract data from.

## Request

### Creating Requests

`Request::new()` takes anything that converts to a `String` and sets all other fields to their defaults:

```rust
let req = Request::new("https://example.com/page");
```

Default values: priority `0`, empty session ID (uses the default session), no callback, no metadata, duplicate filtering enabled, retry count `0`.

### Builder Pattern

Customize requests by chaining builder methods:

```rust
let req = Request::new("https://example.com/api/items")
    .with_sid("auth")
    .with_priority(10)
    .with_dont_filter(true)
    .with_meta(HashMap::from([
        ("category".into(), json!("electronics")),
    ]))
    .with_callback("parse_items", Box::new(|response| {
        // custom parse logic for this specific request
        vec![]
    }));
```

Here is what each method does:

| Method | Effect |
|--------|--------|
| `with_sid(id)` | Routes the request to a specific session in the `SessionManager` |
| `with_priority(n)` | Sets scheduling priority. Higher values are fetched first. Use negative values for low-priority work |
| `with_dont_filter(true)` | Bypasses deduplication so the URL can be fetched even if it was seen before |
| `with_meta(map)` | Attaches arbitrary key-value data that travels with the request through the pipeline |
| `with_callback(name, cb)` | Overrides the spider's `parse()` method for this specific request |

### Request Fields

All fields on `Request` are public, so you can read or modify them directly when the builder pattern is not enough:

```rust
let mut req = Request::new("https://example.com");
req.priority = 5;
req.sid = "browser".into();
req.meta.insert("page_num".into(), json!(3));
```

Notable fields:

- `url: String` -- the URL to fetch.
- `sid: String` -- session ID for routing. Empty means "use the default."
- `priority: i32` -- scheduling priority (higher = first).
- `dont_filter: bool` -- skip duplicate detection when `true`.
- `meta: HashMap<String, serde_json::Value>` -- arbitrary metadata.
- `callback: Option<Callback>` -- per-request parse function.
- `callback_name: Option<String>` -- label for the callback, used in debug output.
- `retry_count: u32` -- managed by the engine for blocked-response retries.
- `session_kwargs: HashMap<String, serde_json::Value>` -- extra parameters forwarded to the session fetcher (method, headers, body, etc.).

### Deduplication

The `Scheduler` computes a SHA-1 fingerprint for each request based on its session ID, HTTP method, and URL. Two requests with the same fingerprint are considered duplicates. The second one is silently dropped unless `dont_filter` is `true`.

You can tune what goes into the fingerprint via spider-level settings:

```rust
impl Spider for MySpider {
    // Include request body in the fingerprint (for POST requests)
    fn fp_include_kwargs(&self) -> bool { true }

    // Include HTTP headers in the fingerprint
    fn fp_include_headers(&self) -> bool { true }

    // Keep URL fragments (#section) in the fingerprint
    fn fp_keep_fragments(&self) -> bool { true }

    // ...
}
```

### Extracting the Domain

`Request::domain()` parses the URL and returns the hostname. This is used internally for domain allowlisting and per-domain statistics, but you can call it in your own code:

```rust
let req = Request::new("https://shop.example.com/products");
assert_eq!(req.domain(), "shop.example.com");
```

## SpiderOutput

`parse()` returns `Vec<SpiderOutput>`, where each value is one of two variants:

### SpiderOutput::Item

A scraped data record. Wrap any `serde_json::Value`:

```rust
SpiderOutput::Item(json!({
    "title": "Rust Programming",
    "price": 29.99,
    "in_stock": true,
}))
```

Items pass through the `on_scraped_item()` hook before being stored. The hook can transform the item (return `Some(modified)`) or drop it (return `None`).

### SpiderOutput::FollowRequest

A new URL to crawl. Wrap a `Request`:

```rust
SpiderOutput::FollowRequest(
    Request::new("https://example.com/page/2")
        .with_priority(5)
)
```

Follow requests go through domain checking and deduplication before being added to the scheduler.

### Mixing Items and Follow Requests

A single `parse()` call typically returns both:

```rust
fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    let mut outputs = Vec::new();

    // Extract data from the current page
    for product in response.css("div.product") {
        outputs.push(SpiderOutput::Item(json!({
            "name": product.css("h2").text().first(),
            "price": product.css(".price").text().first(),
        })));
    }

    // Follow pagination links
    for link in response.css("a.page-link") {
        if let Some(href) = link.attr("href") {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
            ));
        }
    }

    outputs
}
```

## Response

The `Response` type comes from `scrapling_fetch` and represents a completed HTTP exchange. It holds the raw body bytes, headers, cookies, status code, and metadata. HTML parsing is lazy -- the DOM is only built the first time you call a method that needs it.

### Status Checking

```rust
fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    if !response.is_success() {
        eprintln!("Failed: {} {}", response.status, response.reason);
        return vec![];
    }
    // ...
}
```

Available status helpers:

| Method | Status Range |
|--------|-------------|
| `is_success()` | 200-299 |
| `is_redirect()` | 300-399 |
| `is_client_error()` | 400-499 |
| `is_server_error()` | 500-599 |

### CSS Selectors

Query the HTML using CSS selectors:

```rust
// Select all matching elements
let titles = response.css("h1.title");

// Chain selectors
let price = response.css("div.product").first()
    .and_then(|el| el.css(".price").first());
```

### Text Extraction

Get the visible text content of the page:

```rust
// Full text handler with manipulation methods
let text = response.text();

// Find elements by their text content
let results = response.find_by_text("Add to cart", true, false, true);
```

`find_by_text()` parameters: `text`, `partial` (substring match), `case_sensitive`, `clean_match` (strip whitespace before comparing).

### URL Resolution

Resolve relative URLs against the response's base URL:

```rust
// response.url() is "https://example.com/catalog/page1"

let abs = response.urljoin("/products/123");
// => "https://example.com/products/123"

let abs = response.follow_url("page2");
// => "https://example.com/catalog/page2"
```

`follow_url()` is a semantic alias for `urljoin()` -- they do the same thing. Use whichever reads better in context.

### Content Conversion

Convert the HTML body to other formats:

```rust
// Convert to Markdown (useful for LLM consumption)
let markdown = response.to_markdown();

// Convert to plain text (strip all HTML)
let plain = response.to_text();
```

### Accessing Response Metadata

```rust
// The final URL after redirects
let url = response.url();

// Response headers
if let Some(ct) = response.headers.get("content-type") {
    println!("Content-Type: {}", ct);
}

// Cookies from the response
for (name, value) in &response.cookies {
    println!("Cookie: {}={}", name, value);
}

// Request headers that were sent
let sent_ua = response.request_headers.get("user-agent");

// Character encoding
let encoding = &response.encoding;

// HTTP method used
let method = &response.method;

// Raw body bytes
let body_len = response.body.len();
```

### The meta Field

The `Response` carries a `meta` map (`HashMap<String, serde_json::Value>`) that you can use to pass data between callbacks:

```rust
fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    let mut outputs = Vec::new();

    for link in response.css("a.category") {
        if let Some(href) = link.attr("href") {
            let category = link.text().first().unwrap_or_default();

            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
                    .with_meta(HashMap::from([
                        ("category".into(), json!(category)),
                    ]))
                    .with_callback("parse_category", Box::new(parse_category))
            ));
        }
    }

    outputs
}

fn parse_category(response: Response) -> Vec<SpiderOutput> {
    let category = response.meta.get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let mut outputs = Vec::new();
    for product in response.css("div.product") {
        outputs.push(SpiderOutput::Item(json!({
            "category": category,
            "name": product.css("h2").text().first(),
        })));
    }
    outputs
}
```

The `meta` from a `Request` is forwarded to the `Response` when the fetch completes, so any data you attach to the request is available in the callback.

## Callbacks

A `Callback` is a `Box<dyn Fn(Response) -> Vec<SpiderOutput> + Send + Sync>`. When attached to a request, it replaces the spider's `parse()` method for that specific response.

Use callbacks when different page types need different parsing logic:

```rust
fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    let mut outputs = Vec::new();

    // Listing pages go to the default parse()
    for link in response.css("a.listing") {
        if let Some(href) = link.attr("href") {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
            ));
        }
    }

    // Detail pages get a custom callback
    for link in response.css("a.detail") {
        if let Some(href) = link.attr("href") {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
                    .with_callback("parse_detail", Box::new(|resp| {
                        vec![SpiderOutput::Item(json!({
                            "title": resp.css("h1").text().first(),
                            "body": resp.to_text(),
                        }))]
                    }))
            ));
        }
    }

    outputs
}
```

Because closures are not cloneable, `Request::copy_without_callback()` exists for when the engine needs to duplicate a request (e.g., for blocked-response retries). The copy preserves all fields except the callback itself.

## Next Steps

- [Getting Started](getting-started.md) -- write your first spider
- [Architecture](architecture.md) -- how requests flow through the system
- [Session Management](sessions.md) -- routing requests to different backends
- [Advanced Features](advanced.md) -- concurrency, proxies, checkpointing, and hooks
