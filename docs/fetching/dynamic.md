# Browser Automation with DynamicSession

`DynamicSession` launches a real Chromium browser via Playwright, navigates to pages, executes JavaScript, waits for the DOM to stabilize, and returns the rendered HTML as a `Response`. Use it when the content you need is generated client-side.

## When to Use DynamicSession

- The page relies on JavaScript to render content (React, Vue, Angular, etc.)
- Data loads via AJAX calls after the initial page load
- You need to interact with the page (click buttons, fill forms, scroll)
- The site does not employ aggressive bot detection (use `StealthySession` for that)

If the page works with JavaScript disabled, use `Fetcher` instead -- it is faster and uses fewer resources.

## Session Lifecycle

Every `DynamicSession` follows four steps:

```rust
use scrapling_browser::{BrowserConfig, DynamicSession};

// 1. Construct -- validates config, does NOT launch the browser
let config = BrowserConfig {
    headless: true,
    ..Default::default()
};
let mut session = DynamicSession::new(config)?;

// 2. Start -- launches Chromium and creates a browser context
session.start().await?;

// 3. Fetch -- navigate, wait, extract (repeat as needed)
let response = session.fetch("https://example.com", None).await?;
println!("Status: {}", response.status);

let page2 = session.fetch("https://example.com/other", None).await?;

// 4. Close -- tears down browser and Playwright driver
session.close().await?;
```

You can call `fetch` multiple times on the same session. Each call opens a new page, navigates, extracts the response, and closes the page.

Check whether a session is running with `session.is_alive()`.

## BrowserConfig

`BrowserConfig` controls everything about how the browser launches and behaves. Most fields have sensible defaults, so you only override what you need.

```rust
use scrapling_browser::BrowserConfig;

let config = BrowserConfig {
    headless: true,
    network_idle: true,
    disable_resources: true,
    timeout_ms: 15_000.0,
    ..Default::default()
};
```

### All Config Fields

#### Browser Launch

| Field | Type | Default | Description |
|---|---|---|---|
| `headless` | `bool` | `true` | Run without a visible browser window |
| `real_chrome` | `bool` | `false` | Use system-installed Chrome instead of bundled Chromium |
| `executable_path` | `Option<String>` | `None` | Path to a custom browser binary |
| `extra_flags` | `Vec<String>` | `[]` | Extra Chromium CLI flags |
| `user_data_dir` | `Option<String>` | `None` | Persistent browser profile directory |

#### Navigation and Waiting

| Field | Type | Default | Description |
|---|---|---|---|
| `load_dom` | `bool` | `true` | Wait for DOMContentLoaded after navigation |
| `network_idle` | `bool` | `false` | Wait for network activity to settle |
| `wait_selector` | `Option<String>` | `None` | CSS selector to wait for before returning |
| `wait_selector_state` | `WaitState` | `Attached` | Required state of the wait selector |
| `wait_ms` | `u64` | `0` | Extra sleep (ms) after page stabilization |
| `timeout_ms` | `f64` | `30000.0` | Navigation and action timeout (ms) |
| `google_search` | `bool` | `true` | Warm up with a Google navigation first |

#### Resource Control

| Field | Type | Default | Description |
|---|---|---|---|
| `disable_resources` | `bool` | `false` | Block images, fonts, stylesheets, media |
| `block_ads` | `bool` | `false` | Block requests to ~3,527 known ad/tracker domains |
| `blocked_domains` | `HashSet<String>` | `{}` | Custom domains to block (suffix-matched) |
| `capture_xhr` | `Option<String>` | `None` | URL pattern to capture XHR/fetch responses |

#### Network and Proxy

| Field | Type | Default | Description |
|---|---|---|---|
| `proxy` | `Option<ProxyConfig>` | `None` | Static proxy server |
| `proxy_rotator` | `Option<ProxyRotator>` | `None` | Rotating proxy provider (different IP per fetch) |
| `extra_headers` | `HashMap<String, String>` | `{}` | Headers sent with every request |
| `dns_over_https` | `bool` | `false` | Route DNS through Cloudflare's 1.1.1.1 |

#### Context and Identity

| Field | Type | Default | Description |
|---|---|---|---|
| `cookies` | `Vec<CookieParam>` | `[]` | Cookies injected before navigation |
| `useragent` | `Option<String>` | `None` | Custom User-Agent string |
| `timezone_id` | `Option<String>` | `None` | IANA timezone (e.g., `"America/New_York"`) |
| `locale` | `Option<String>` | `None` | Locale string (e.g., `"en-US"`) |
| `init_script` | `Option<String>` | `None` | Path to JS file evaluated in every page context |

#### Reliability

| Field | Type | Default | Description |
|---|---|---|---|
| `retries` | `u32` | `3` | Max fetch attempts (1-10) |
| `retry_delay_secs` | `f64` | `1.0` | Delay between retries |
| `max_pages` | `u32` | `1` | Max concurrent pages in the pool (1-50) |

### Wait States

Control what "ready" means for the wait selector.

```rust
use scrapling_browser::WaitState;

// Element exists in the DOM (default)
WaitState::Attached

// Element exists AND is visible on screen
WaitState::Visible

// Element exists but is NOT visible (e.g., loading spinner hidden)
WaitState::Hidden

// Element has been removed from the DOM entirely
WaitState::Detached
```

### Pre-Injecting Cookies

Skip login flows by injecting session cookies directly.

```rust
use scrapling_browser::{BrowserConfig, CookieParam};

let config = BrowserConfig {
    cookies: vec![
        CookieParam {
            name: "session_id".to_string(),
            value: "abc123".to_string(),
            domain: Some(".example.com".to_string()),
            path: Some("/".to_string()),
            url: None,
        },
    ],
    ..Default::default()
};
```

## Page Callbacks

Callbacks let you run custom logic on each page at two points in the fetch lifecycle.

### page_setup

Runs immediately after a page is created, before navigation. Use it to add request interceptors, inject scripts, or configure page settings.

```rust
use scrapling_browser::BrowserConfig;

let config = BrowserConfig {
    page_setup: Some(Box::new(|page| {
        Box::pin(async move {
            // Add a custom request header via page-level routing
            page.route("**/*", |route| async move {
                route.continue_(None).await
            }).await?;
            Ok(())
        })
    })),
    ..Default::default()
};
```

### page_action

Runs after navigation completes and the page has stabilized, but before the HTML is captured. Use it to click buttons, fill forms, scroll, or trigger lazy-loaded content.

```rust
let config = BrowserConfig {
    page_action: Some(Box::new(|page| {
        Box::pin(async move {
            // Click a "Load More" button
            let btn = page.locator("button.load-more").await;
            btn.click(None).await?;

            // Wait for new content to appear
            let items = page.locator(".item").await;
            items.wait_for(None).await?;

            Ok(())
        })
    })),
    ..Default::default()
};
```

Both callbacks receive a cloned `playwright_rs::Page` and must return a pinned, `Send` future. The closure itself must be `Send + Sync`.

## FetchParams (Per-Request Overrides)

Override session-level settings for a single `fetch` call without changing the `BrowserConfig`.

```rust
use scrapling_browser::FetchParams;

let params = FetchParams {
    network_idle: Some(true),
    timeout_ms: Some(60_000.0),
    wait_selector: Some("#data-table".to_string()),
    wait_selector_state: Some(WaitState::Visible),
    disable_resources: Some(true),
    ..Default::default()
};

let response = session.fetch("https://example.com/slow-page", Some(params)).await?;
```

### All FetchParams Fields

| Field | Type | Description |
|---|---|---|
| `google_search` | `Option<bool>` | Override Google warm-up navigation |
| `timeout_ms` | `Option<f64>` | Override navigation timeout |
| `wait_ms` | `Option<u64>` | Override post-load sleep |
| `extra_headers` | `Option<HashMap<String, String>>` | Override extra headers |
| `disable_resources` | `Option<bool>` | Override resource blocking |
| `network_idle` | `Option<bool>` | Override network-idle wait |
| `load_dom` | `Option<bool>` | Override DOM-content-loaded wait |
| `wait_selector` | `Option<String>` | Override wait selector |
| `wait_selector_state` | `Option<WaitState>` | Override wait selector state |
| `blocked_domains` | `Option<HashSet<String>>` | Override blocked domains |
| `solve_cloudflare` | `Option<bool>` | Enable Cloudflare solving for this request |
| `selector_config` | `Option<HashMap<String, Value>>` | Override selector engine config |

`FetchParams` is merged with the session's `BrowserConfig` into a `ResolvedFetchParams` struct before each navigation. Every `Option::Some` value wins over the config default.

## CDP Connection (Remote Browsers)

Connect to a running Chrome instance via the Chrome DevTools Protocol instead of launching a new browser. This is useful for remote browsers, Docker containers, or shared browser pools.

```rust
let config = BrowserConfig {
    cdp_url: Some("ws://localhost:9222".to_string()),
    ..Default::default()
};

let mut session = DynamicSession::new(config)?;
session.start().await?; // Connects to the running browser instead of launching one
```

The CDP URL must start with `ws://` or `wss://`. The session validates this during construction.

## Real Chrome Mode

Use the system-installed Chrome instead of the Chromium binary bundled with Playwright. System Chrome may have a different fingerprint and can pass more bot-detection checks.

```rust
let config = BrowserConfig {
    real_chrome: true,
    ..Default::default()
};
```

You can also point to a specific browser binary:

```rust
let config = BrowserConfig {
    executable_path: Some("/usr/bin/google-chrome-stable".to_string()),
    ..Default::default()
};
```

The path is validated during config validation. If the file does not exist, construction fails with an error.

## Blocking Resources

Speed up page loads by blocking resources you do not need.

```rust
use std::collections::HashSet;

let config = BrowserConfig {
    // Block images, fonts, stylesheets, media
    disable_resources: true,

    // Block known ad and tracker domains (~3,527 domains)
    block_ads: true,

    // Block specific domains (suffix-matched)
    blocked_domains: HashSet::from([
        "analytics.example.com".to_string(),
        "tracker.example.com".to_string(),
    ]),
    ..Default::default()
};
```

Domain blocking is suffix-based: adding `"ads.example.com"` also blocks `"sub.ads.example.com"`.

## Waiting for Dynamic Content

Choose the right waiting strategy for your target page.

```rust
// Fast: wait for DOMContentLoaded only (default)
let config = BrowserConfig {
    load_dom: true,
    ..Default::default()
};

// Thorough: wait for all network activity to settle
let config = BrowserConfig {
    network_idle: true,
    ..Default::default()
};

// Precise: wait for a specific element to appear
let config = BrowserConfig {
    wait_selector: Some("#content-loaded".to_string()),
    wait_selector_state: WaitState::Visible,
    ..Default::default()
};

// Last resort: fixed delay after stabilization
let config = BrowserConfig {
    wait_ms: 2000,
    ..Default::default()
};
```

These options stack. For example, you can enable both `network_idle` and `wait_selector` -- the session waits for network idle first, then waits for the selector.

## Proxy Configuration

### Static Proxy

```rust
use scrapling_browser::ProxyConfig;

let config = BrowserConfig {
    proxy: Some(ProxyConfig {
        server: "http://proxy.example.com:8080".to_string(),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
    }),
    ..Default::default()
};
```

### Rotating Proxy

When a proxy rotator is set, the session creates a fresh browser context per request so each navigation can use a different proxy address.

```rust
use scrapling_fetch::{Proxy, ProxyRotator};

let rotator = ProxyRotator::new(vec![
    Proxy::Url("http://proxy1.example.com:8080".to_string()),
    Proxy::Url("http://proxy2.example.com:8080".to_string()),
])?;

let config = BrowserConfig {
    proxy_rotator: Some(rotator),
    ..Default::default()
};
```

Static proxy and proxy rotator are mutually exclusive. Setting both causes a validation error.
