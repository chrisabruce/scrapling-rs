# Session Management

The `SessionManager` lets your spider register multiple HTTP backends under string identifiers. Each `Request` carries an optional session ID (`sid`) that tells the engine which backend to use for that fetch. This is how you mix stateless HTTP fetchers with cookie-persisting sessions, route certain pages through different proxy configurations, or combine HTTP and browser-based fetching in a single crawl.

## Session Types

There are two session variants in the `Session` enum:

| Variant | Type | Cookie Handling | Use Case |
|---------|------|-----------------|----------|
| `Session::Fetcher` | `scrapling_fetch::Fetcher` | None -- each request is independent | Public pages, APIs, stateless scraping |
| `Session::FetcherSession` | `scrapling_fetch::FetcherSession` | Automatic -- cookies persist across requests | Login flows, CSRF tokens, session-dependent sites |

## Default Behavior

If you do not override `configure_sessions()`, the spider registers a single stateless `Fetcher` under the ID `"default"`:

```rust
fn configure_sessions(&self, manager: &mut SessionManager) {
    let fetcher = scrapling_fetch::Fetcher::new();
    let _ = manager.add("default", Session::Fetcher(fetcher), true);
}
```

This is fine for most scraping tasks where you do not need persistent cookies.

## Configuring Custom Sessions

Override `configure_sessions()` to register your own sessions. The `add()` method takes three arguments: a session ID, a `Session` variant, and a boolean indicating whether this session should be the default.

```rust
use scrapling_spider::session::{Session, SessionManager};
use scrapling_fetch::{Fetcher, FetcherSession, FetcherConfigBuilder};

impl Spider for MySpider {
    // ...

    fn configure_sessions(&self, manager: &mut SessionManager) {
        // A plain HTTP fetcher for public pages
        let public = Fetcher::new();
        manager.add("public", Session::Fetcher(public), true).unwrap();

        // A session-based fetcher for authenticated pages
        let auth = FetcherSession::new();
        manager.add("auth", Session::FetcherSession(auth), false).unwrap();
    }
}
```

Rules for `add()`:

- The first session added automatically becomes the default, even if `default` is `false`.
- Passing `default: true` explicitly sets that session as the default, overriding any previous default.
- Session IDs must be unique. Adding a duplicate ID returns an error.
- At least one session must be registered. The engine returns an error at construction time if the manager is empty.

## Routing Requests to Sessions

Use `Request::with_sid()` to route a request to a specific session:

```rust
fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    let mut outputs = Vec::new();

    // This request uses the default session
    outputs.push(SpiderOutput::FollowRequest(
        Request::new("https://example.com/public-page")
    ));

    // This request uses the "auth" session
    outputs.push(SpiderOutput::FollowRequest(
        Request::new("https://example.com/dashboard")
            .with_sid("auth")
    ));

    outputs
}
```

When a request's `sid` is empty (the default), the engine uses whatever session is marked as the default in the manager. When `sid` is set, the engine looks up that exact ID. If the ID does not exist, the fetch fails with a `SpiderError::Session` and the request's `on_error` hook fires.

## Session-Based Fetcher with Cookies

`FetcherSession` maintains an internal cookie jar that persists across requests. This is essential for sites that require login:

```rust
fn configure_sessions(&self, manager: &mut SessionManager) {
    let session = FetcherSession::new();
    manager.add("default", Session::FetcherSession(session), true).unwrap();
}

fn start_requests(&self) -> Vec<Request> {
    // First request logs in -- the session will carry the auth cookie forward
    vec![Request::new("https://example.com/login")
        .with_callback("handle_login", Box::new(|response| {
            // After login, the session cookie is stored automatically.
            // Now follow a link to an authenticated page.
            vec![SpiderOutput::FollowRequest(
                Request::new("https://example.com/dashboard")
            )]
        }))]
}
```

## Mixing HTTP and Browser Sessions

You can register both HTTP fetchers and browser-based sessions in the same spider. Use session IDs to route JavaScript-heavy pages through the browser while keeping simple pages on fast HTTP fetchers:

```rust
fn configure_sessions(&self, manager: &mut SessionManager) {
    // Fast HTTP fetcher for static pages
    let http = Fetcher::new();
    manager.add("http", Session::Fetcher(http), true).unwrap();

    // Browser session for JavaScript-rendered pages
    let browser = FetcherSession::new();  // configured for browser mode
    manager.add("browser", Session::FetcherSession(browser), false).unwrap();
}

fn parse(&self, response: Response) -> Vec<SpiderOutput> {
    let mut outputs = Vec::new();

    // Static listing pages go through HTTP
    for link in response.css("a.listing-link") {
        if let Some(href) = link.attr("href") {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
                    .with_sid("http")
            ));
        }
    }

    // Detail pages with dynamic content go through the browser
    for link in response.css("a.detail-link") {
        if let Some(href) = link.attr("href") {
            outputs.push(SpiderOutput::FollowRequest(
                Request::new(response.follow_url(&href))
                    .with_sid("browser")
            ));
        }
    }

    outputs
}
```

## Inspecting Registered Sessions

The `SessionManager` exposes a few utility methods:

```rust
let manager: &SessionManager = /* from engine internals */;

// List all registered session IDs
let ids = manager.session_ids();  // e.g., ["http", "browser"]

// Check if a session exists
if manager.contains("auth") {
    // ...
}

// Get the default session ID
let default_id = manager.default_session_id()?;  // e.g., "http"

// Total number of sessions
let count = manager.len();
```

## Per-Session Statistics

After a crawl, `CrawlStats` includes a `sessions_requests_count` map showing how many requests went through each session:

```rust
let stats = engine.crawl().await?;

for (sid, count) in &stats.sessions_requests_count {
    println!("Session '{}': {} requests", sid, count);
}
// Output:
// Session 'http': 142 requests
// Session 'browser': 23 requests
```

This is useful for tuning your session strategy -- if most requests go through the browser when they could use plain HTTP, you are leaving performance on the table.

## Next Steps

- [Getting Started](getting-started.md) -- write your first spider
- [Architecture](architecture.md) -- understand how sessions fit into the crawl loop
- [Requests and Responses](requests-responses.md) -- the Request builder pattern and Response API
- [Advanced Features](advanced.md) -- proxies, concurrency, and more
