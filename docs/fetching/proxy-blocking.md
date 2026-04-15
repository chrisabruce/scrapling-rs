# Proxy Management and Handling Blocks

When scraping at scale, you'll need proxies to distribute requests and strategies for handling blocked responses. scrapling-rs has built-in support for both.

## Proxy rotation

The `ProxyRotator` distributes requests across a pool of proxies using a configurable strategy.

```rust
use scrapling_fetch::{Proxy, ProxyRotator, Fetcher, FetcherConfigBuilder};

let proxies = vec![
    Proxy::Url("http://proxy1:8080".into()),
    Proxy::Url("http://proxy2:8080".into()),
    Proxy::Config {
        server: "http://proxy3:8080".into(),
        username: Some("user".into()),
        password: Some("pass".into()),
    },
];

let rotator = ProxyRotator::new(proxies).unwrap();

let fetcher = Fetcher::from_builder(
    Fetcher::builder()
        .proxy_rotator(rotator)
        .retries(3)
)?;
```

The default strategy is cyclic (round-robin). Each call to `get_proxy()` returns the next proxy in the list, wrapping around at the end.

### Proxy formats

Two formats are supported, matching the Python API:

```rust
// URL string
Proxy::Url("http://user:pass@proxy.example.com:8080".into())

// Structured config (useful for Playwright-style dict proxies)
Proxy::Config {
    server: "http://proxy.example.com:8080".into(),
    username: Some("user".into()),
    password: Some("pass".into()),
}
```

### Per-request proxy override

You can override the proxy for a single request:

```rust
use scrapling_fetch::RequestConfig;

let req = RequestConfig {
    proxy: Some(Proxy::Url("http://special-proxy:8080".into())),
    ..Default::default()
};
let response = fetcher.get("https://example.com", Some(req)).await?;
```

## Handling blocked requests

### In the spider framework

The spider has built-in block detection. By default, these status codes are considered "blocked":

```
401, 403, 407, 429, 444, 500, 502, 503, 504
```

Override `is_blocked()` for custom detection:

```rust
fn is_blocked(&self, response: &Response) -> bool {
    response.status == 403 || response.body.len() < 100
}
```

Blocked requests are automatically retried with decremented priority (up to `max_blocked_retries`).

### Proxy error detection

The `is_proxy_error()` function checks whether an error is proxy-related by matching against known error message patterns:

```rust
use scrapling_fetch::is_proxy_error;

// Detects: "connection refused", "connection reset", "net::err_proxy",
//          "net::err_tunnel", "connection timed out", "failed to connect",
//          "could not resolve proxy"
```

The fetch client uses this internally to decide whether to rotate to the next proxy on failure.

## Ad and domain blocking

The browser crate includes a blocklist of 3,527 ad and tracking domains. Enable it with `block_ads`:

```rust
use scrapling_browser::BrowserConfig;

let config = BrowserConfig {
    block_ads: true,  // blocks doubleclick.net, google-analytics.com, etc.
    blocked_domains: HashSet::from(["custom-tracker.com".into()]),
    ..Default::default()
};
```

Domain matching is suffix-based: blocking `tracker.com` also blocks `sub.tracker.com` and `ads.sub.tracker.com`.
