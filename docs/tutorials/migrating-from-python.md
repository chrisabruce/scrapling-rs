# Migrating from Python Scrapling

If you've used the Python version of Scrapling, the Rust port will feel familiar. The core concepts are the same, the API is intentionally similar, and most patterns translate directly. This guide covers the key differences.

## Parsing

### Python
```python
from scrapling import Selector

page = Selector("<html><body><h1>Hello</h1></body></html>")
titles = page.css("h1")
print(titles[0].text)
```

### Rust
```rust
use scrapling::selector::Selector;

let page = Selector::from_html("<html><body><h1>Hello</h1></body></html>");
let titles = page.css("h1");
println!("{}", titles[0].text());
```

Key differences:
- `Selector("html")` becomes `Selector::from_html("html")`
- `.text` property becomes `.text()` method
- Indexing returns a reference, not a copy

## CSS selectors

The selector syntax is identical. Both support `::text` and `::attr()` pseudo-elements.

| Python | Rust |
|--------|------|
| `page.css("div.item")` | `page.css("div.item")` |
| `page.css("a::attr(href)")` | `page.css("a::attr(href)")` |
| `page.css("p::text")` | `page.css("p::text")` |

## Attributes

### Python
```python
attrs = element.attrib
print(attrs["class"])
print(attrs.get("id", "default"))
config = attrs["data-config"].json()
```

### Rust
```rust
let attrs = element.attrib();
println!("{}", attrs["class"]);
println!("{}", attrs.get("id").map(|v| v.as_ref()).unwrap_or("default"));
let config: serde_json::Value = attrs["data-config"].json().unwrap();
```

## Adaptive scraping

### Python
```python
page = Selector(html, adaptive=True, storage=storage)
results = page.css("#target", adaptive=True, auto_save=True)
```

### Rust
```rust
let page = Selector::from_html(html);
let results = page.css_adaptive("#target", &storage, true, true, None, 0.0);
```

The Rust version uses a separate `css_adaptive()` method instead of adding parameters to `css()`. This keeps the common case (non-adaptive) simple.

## HTTP fetching

### Python
```python
from scrapling import Fetcher

async with FetcherSession(impersonate="chrome") as session:
    response = await session.get("https://example.com")
    print(response.status)
```

### Rust
```rust
use scrapling_fetch::{Fetcher, FetcherConfig, Impersonate};

let fetcher = Fetcher::with_config(FetcherConfig {
    impersonate: Impersonate::Single("chrome".into()),
    ..Default::default()
});
let response = fetcher.get("https://example.com", None).await?;
println!("{}", response.status);
```

### Impersonation differences

Python uses `curl_cffi` for TLS fingerprint impersonation. Rust uses `wreq` (a BoringSSL-based HTTP client with 135+ browser profiles). The browser profile names are the same (`"chrome"`, `"firefox"`, `"safari"`, `"edge"`).

## Spider framework

### Python
```python
class MySpider(Spider):
    name = "my_spider"
    start_urls = ["https://example.com"]

    async def parse(self, response):
        for item in response.css(".product"):
            yield {"name": item.css("h3::text").get()}
```

### Rust
```rust
struct MySpider;

impl Spider for MySpider {
    fn name(&self) -> &str { "my_spider" }
    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".into()]
    }
    fn parse(&self, response: Response) -> Vec<SpiderOutput> {
        response.css(".product").iter().map(|item| {
            SpiderOutput::Item(serde_json::json!({
                "name": item.css("h3::text").first().unwrap().text().as_ref()
            }))
        }).collect()
    }
}
```

Key differences:
- Python uses `yield` (async generators). Rust returns `Vec<SpiderOutput>`.
- Python's class attributes become trait methods with defaults.
- Rust uses `SpiderOutput::Item(json)` and `SpiderOutput::FollowRequest(request)` instead of yielding dicts and Request objects.

## Streaming

### Python
```python
async for item in spider.stream():
    print(item)
```

### Rust
```rust
let mut rx = engine.stream();
// spawn engine.crawl() in background
while let Some(item) = rx.recv().await {
    println!("{}", item);
}
```

## Error handling

Python raises exceptions. Rust returns `Result` types. Use `?` for propagation.

```rust
let response = fetcher.get(url, None).await?;  // propagates FetchError
let data: Value = response.text().json()?;      // propagates serde error
```

## What's the same

- CSS/XPath selector syntax
- Adaptive element relocation algorithm (12-factor similarity)
- Browser impersonation profiles
- Cloudflare Turnstile solver
- Spider lifecycle hooks
- Proxy rotation
- Checkpoint/resume
- robots.txt compliance
- MCP server tools
