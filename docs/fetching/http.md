# HTTP Fetching

The `scrapling-fetch` crate handles HTTP requests with browser-grade TLS fingerprinting, automatic retries, and proxy rotation. It provides two client types: `Fetcher` for stateless requests and `FetcherSession` for persistent sessions with cookie storage.

## Fetcher

`Fetcher` creates a fresh HTTP client for every request. No cookies or connection state carry over between calls. This is the simplest option and works well for parallel scraping or when you want full isolation between requests.

```rust
use scrapling_fetch::Fetcher;

let fetcher = Fetcher::new();
let response = fetcher.get("https://example.com", None).await?;
println!("{}", response.status);
```

### Construction

```rust
// Default settings: Chrome impersonation, 30s timeout, 3 retries, stealth headers
let fetcher = Fetcher::new();

// From a pre-built config
let config = FetcherConfig { timeout_secs: 10, ..Default::default() };
let fetcher = Fetcher::with_config(config);

// From the builder (with validation)
let fetcher = Fetcher::from_builder(
    Fetcher::builder()
        .timeout_secs(10)
        .retries(5)
)?;
```

### HTTP Methods

All methods take a URL and an optional `RequestConfig` for per-request overrides. Pass `None` to use the fetcher defaults.

```rust
let response = fetcher.get("https://api.example.com/data", None).await?;
let response = fetcher.post("https://api.example.com/data", None).await?;
let response = fetcher.put("https://api.example.com/data/1", None).await?;
let response = fetcher.delete("https://api.example.com/data/1", None).await?;
```

## FetcherSession

`FetcherSession` maintains a persistent wreq client with an automatic cookie jar. Cookies set by one response are sent with subsequent requests. Use this for login flows or multi-step interactions that require session state.

```rust
use scrapling_fetch::{FetcherConfig, FetcherSession, RequestConfig};

let config = FetcherConfig::default();
let mut session = FetcherSession::new(config);
session.open()?;

// Login -- cookies from this response are stored automatically
let login_req = RequestConfig {
    json: Some(serde_json::json!({
        "username": "user",
        "password": "pass"
    })),
    ..Default::default()
};
session.post("https://example.com/login", Some(login_req)).await?;

// Subsequent requests include the session cookies
let response = session.get("https://example.com/dashboard", None).await?;

session.close();
```

### Lifecycle

1. `FetcherSession::new(config)` -- creates the session (not yet active)
2. `session.open()` -- builds the underlying HTTP client with cookie storage
3. `session.get()` / `session.post()` / etc. -- make requests
4. `session.close()` -- drops the client and all cookies

The session can be re-opened after closing. `FetcherSession` also implements `Drop`, so the client is cleaned up if the session goes out of scope.

```rust
assert!(!session.is_active());  // not yet opened
session.open()?;
assert!(session.is_active());
session.close();
assert!(!session.is_active());
```

## FetcherConfig

`FetcherConfig` holds the default settings applied to every request. Build one directly or use `FetcherConfigBuilder` for validation.

### Direct Construction

```rust
use scrapling_fetch::{FetcherConfig, FollowRedirects, Impersonate};

let config = FetcherConfig {
    impersonate: Impersonate::Single("firefox".to_string()),
    stealthy_headers: true,
    timeout_secs: 15,
    retries: 5,
    retry_delay_secs: 2,
    follow_redirects: FollowRedirects::All,
    max_redirects: 10,
    verify: true,
    ..Default::default()
};
```

### Builder

The builder validates the config on `.build()` and catches invalid combinations (e.g., setting both a static proxy and a proxy rotator).

```rust
use scrapling_fetch::FetcherConfigBuilder;

let (config, rotator) = FetcherConfigBuilder::new()
    .timeout_secs(10)
    .retries(5)
    .stealthy_headers(true)
    .follow_redirects(FollowRedirects::Safe)
    .header("Accept-Language", "en-US")
    .build()?;
```

### All Config Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `impersonate` | `Impersonate` | `Single("chrome")` | Browser TLS/HTTP2 fingerprint profile |
| `stealthy_headers` | `bool` | `true` | Inject browser-like headers (Referer, Sec-Ch-Ua, etc.) |
| `proxy` | `Option<Proxy>` | `None` | Static proxy for all requests |
| `timeout_secs` | `u64` | `30` | Full request lifecycle timeout |
| `headers` | `HashMap<String, String>` | `{}` | Default headers for every request |
| `retries` | `u32` | `3` | Max retry attempts per request (1 = no retries) |
| `retry_delay_secs` | `u64` | `1` | Fixed delay between retries |
| `follow_redirects` | `FollowRedirects` | `Safe` | Redirect policy |
| `max_redirects` | `usize` | `30` | Max redirects before failure |
| `verify` | `bool` | `true` | Verify TLS certificates |

### Redirect Policies

```rust
use scrapling_fetch::FollowRedirects;

// Don't follow any redirects -- you get the raw 3xx response
FollowRedirects::None

// Follow redirects only for GET and HEAD (default, prevents POST body re-submission)
FollowRedirects::Safe

// Follow all redirects regardless of HTTP method
FollowRedirects::All
```

## RequestConfig

Override any fetcher default for a single request. Every field is `Option` -- when `None`, the fetcher's default is used.

```rust
use scrapling_fetch::RequestConfig;

let req = RequestConfig {
    timeout_secs: Some(60),
    headers: Some(HashMap::from([
        ("X-Custom".to_string(), "value".to_string()),
    ])),
    cookies: Some(HashMap::from([
        ("session".to_string(), "abc123".to_string()),
    ])),
    params: Some(HashMap::from([
        ("page".to_string(), "2".to_string()),
    ])),
    ..Default::default()
};

let response = fetcher.get("https://example.com/search", Some(req)).await?;
```

### Sending Data

```rust
// JSON body (sets Content-Type: application/json automatically)
let req = RequestConfig {
    json: Some(serde_json::json!({"key": "value"})),
    ..Default::default()
};
fetcher.post("https://api.example.com", Some(req)).await?;

// Raw body bytes
let req = RequestConfig {
    data: Some(b"raw bytes here".to_vec()),
    ..Default::default()
};
fetcher.post("https://api.example.com", Some(req)).await?;

// If both `json` and `data` are set, `json` takes precedence.
```

### Authentication

```rust
let req = RequestConfig {
    auth: Some(("username".to_string(), "password".to_string())),
    ..Default::default()
};
fetcher.get("https://api.example.com/protected", Some(req)).await?;
```

### All RequestConfig Fields

| Field | Type | Description |
|---|---|---|
| `headers` | `Option<HashMap<String, String>>` | Per-request headers (merged with defaults) |
| `cookies` | `Option<HashMap<String, String>>` | Cookies serialized into a Cookie header |
| `params` | `Option<HashMap<String, String>>` | URL query parameters |
| `timeout_secs` | `Option<u64>` | Timeout override |
| `follow_redirects` | `Option<FollowRedirects>` | Redirect policy override |
| `max_redirects` | `Option<usize>` | Max redirects override |
| `retries` | `Option<u32>` | Retry count override |
| `retry_delay_secs` | `Option<u64>` | Retry delay override |
| `proxy` | `Option<Proxy>` | Proxy override (bypasses rotator too) |
| `verify` | `Option<bool>` | TLS verification override |
| `impersonate` | `Option<Impersonate>` | Browser fingerprint override |
| `stealthy_headers` | `Option<bool>` | Stealth headers override |
| `data` | `Option<Vec<u8>>` | Raw request body |
| `json` | `Option<Value>` | JSON request body (overrides `data`) |
| `auth` | `Option<(String, String)>` | HTTP basic auth credentials |

## Browser Impersonation

scrapling-rs uses [wreq](https://github.com/nickel-org/wreq) to emulate real browser TLS ClientHello and HTTP/2 settings. Bot-detection services fingerprint these protocol-level details to tell HTTP libraries apart from real browsers. Impersonation makes your requests indistinguishable from genuine browser traffic at the TLS layer.

### Impersonation Strategies

```rust
use scrapling_fetch::Impersonate;

// Impersonate a specific browser (default: latest Chrome)
Impersonate::Single("chrome".to_string())

// Impersonate a specific version
Impersonate::Single("firefox135".to_string())

// Randomly rotate between profiles on each request
Impersonate::Random(vec![
    "chrome".to_string(),
    "firefox".to_string(),
    "safari".to_string(),
])

// No impersonation (uses wreq defaults -- may be detected)
Impersonate::None
```

### Supported Browser Profiles

| Browser | Versions |
|---|---|
| Chrome | `chrome100`, `chrome120`, `chrome124`, `chrome131`, `chrome136`, `chrome140`, `chrome142`, `chrome143`, `chrome144`, `chrome145` (alias: `chrome`) |
| Edge | `edge134`, `edge140`, `edge145` (alias: `edge`) |
| Firefox | `firefox128`, `firefox133`, `firefox135` (alias: `firefox`) |
| Safari | `safari18`, `safari26` (alias: `safari`) |

Unversioned names (e.g., `"chrome"`) resolve to the latest available version.

## Proxy Support

### Static Proxy

Route all requests through a single proxy server.

```rust
use scrapling_fetch::{FetcherConfig, Proxy};

// Simple URL format
let config = FetcherConfig {
    proxy: Some(Proxy::Url("http://proxy.example.com:8080".to_string())),
    ..Default::default()
};

// Structured format with credentials
let config = FetcherConfig {
    proxy: Some(Proxy::Config {
        server: "http://proxy.example.com:8080".to_string(),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
    }),
    ..Default::default()
};
```

### Proxy Rotation

Distribute requests across multiple proxies. The default strategy cycles through them sequentially.

```rust
use scrapling_fetch::{Fetcher, Proxy, ProxyRotator};

let proxies = vec![
    Proxy::Url("http://proxy1.example.com:8080".to_string()),
    Proxy::Url("http://proxy2.example.com:8080".to_string()),
    Proxy::Url("http://proxy3.example.com:8080".to_string()),
];

let rotator = ProxyRotator::new(proxies)?;

let mut fetcher = Fetcher::new();
fetcher.set_proxy_rotator(rotator);

// Each request uses the next proxy in sequence
let r1 = fetcher.get("https://example.com", None).await?; // proxy1
let r2 = fetcher.get("https://example.com", None).await?; // proxy2
let r3 = fetcher.get("https://example.com", None).await?; // proxy3
let r4 = fetcher.get("https://example.com", None).await?; // proxy1 (wraps around)
```

### Custom Rotation Strategy

Implement your own rotation logic by providing a function with the `RotationStrategy` signature.

```rust
use scrapling_fetch::{Proxy, ProxyRotator};

// Random rotation
fn random_rotation(proxies: &[Proxy], _current: usize) -> usize {
    use rand::Rng;
    rand::thread_rng().gen_range(0..proxies.len())
}

let rotator = ProxyRotator::with_strategy(proxies, random_rotation)?;
```

### Per-Request Proxy Override

Override the proxy for a single request, bypassing both the static proxy and the rotator.

```rust
let req = RequestConfig {
    proxy: Some(Proxy::Url("http://special-proxy.example.com:8080".to_string())),
    ..Default::default()
};
fetcher.get("https://example.com", Some(req)).await?;
```

### Detecting Proxy Errors

Use `is_proxy_error` to check whether a failure was proxy-related, which helps decide whether to retry with a different proxy.

```rust
use scrapling_fetch::is_proxy_error;

match fetcher.get("https://example.com", None).await {
    Ok(response) => { /* success */ }
    Err(e) => {
        if is_proxy_error(&e) {
            println!("Proxy failed, try a different one");
        }
    }
}
```

## Cookie and Header Management

### Default Headers

Headers set on `FetcherConfig` are sent with every request. Per-request headers in `RequestConfig` are merged in, with per-request values winning on name collisions.

```rust
let (config, _) = FetcherConfigBuilder::new()
    .header("Accept-Language", "en-US,en;q=0.9")
    .header("X-Custom-Header", "my-scraper")
    .build()?;

let fetcher = Fetcher::with_config(config);
```

### Stealth Headers

When `stealthy_headers` is enabled (the default), the fetcher automatically adds browser-like headers such as `Referer` (set to Google) and various `Sec-Ch-Ua` / `Sec-Fetch-*` headers. These are only added if you have not already set them yourself.

When browser impersonation is active, wreq handles the fingerprint-sensitive headers (User-Agent, etc.) directly. When impersonation is off, the stealth headers module generates realistic values.

### Per-Request Cookies

```rust
let req = RequestConfig {
    cookies: Some(HashMap::from([
        ("session_id".to_string(), "abc123".to_string()),
        ("consent".to_string(), "accepted".to_string()),
    ])),
    ..Default::default()
};

// Cookies are serialized into a single Cookie header: "session_id=abc123; consent=accepted"
fetcher.get("https://example.com", Some(req)).await?;
```

For automatic cookie persistence across requests, use `FetcherSession` instead of `Fetcher`.
