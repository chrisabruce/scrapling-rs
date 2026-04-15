# Anti-Detection with StealthySession

`StealthySession` is the hardened counterpart to `DynamicSession`. It uses the same Playwright-based browser automation but launches Chromium with 99+ stealth flags that strip automation indicators, and adds optional countermeasures for WebRTC, canvas fingerprinting, and WebGL. It also includes a built-in Cloudflare Turnstile solver.

## When to Use StealthySession

Use `StealthySession` when your target site actively blocks automated browsers. Common signals:

- You get Cloudflare "Just a moment..." challenge pages
- Responses return 403 or empty content despite valid URLs
- The site uses DataDome, Akamai Bot Manager, PerimeterX, or similar services
- `DynamicSession` works initially but gets blocked after a few requests

If the site does not employ bot detection, use `DynamicSession` -- it is simpler and starts faster.

## Basic Usage

`StealthySession` follows the same lifecycle as `DynamicSession`: construct, start, fetch, close.

```rust
use scrapling_browser::{StealthConfig, StealthySession};

let config = StealthConfig {
    block_webrtc: true,
    hide_canvas: true,
    solve_cloudflare: true,
    base: BrowserConfig {
        headless: true,
        block_ads: true,
        ..Default::default()
    },
    ..Default::default()
};

let mut session = StealthySession::new(config)?;
session.start().await?;

let response = session.fetch("https://protected-site.com", None).await?;
println!("Status: {}", response.status);

session.close().await?;
```

## StealthConfig

`StealthConfig` wraps a `BrowserConfig` with four anti-detection options. All standard browser settings (timeout, proxy, headers, cookies, callbacks, etc.) go in the `base` field.

```rust
use scrapling_browser::{BrowserConfig, StealthConfig};

let config = StealthConfig {
    // Anti-detection options
    allow_webgl: true,       // default: true
    hide_canvas: false,      // default: false
    block_webrtc: false,     // default: false
    solve_cloudflare: false, // default: false

    // Standard browser settings
    base: BrowserConfig {
        headless: true,
        network_idle: true,
        timeout_ms: 60_000.0,
        ..Default::default()
    },
};
```

### Anti-Detection Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `allow_webgl` | `bool` | `true` | Allow WebGL rendering. Set `false` to disable WebGL entirely, reducing fingerprint surface. Some fingerprinters read GPU renderer strings via WebGL. |
| `hide_canvas` | `bool` | `false` | Inject noise into canvas pixel data. Thwarts canvas fingerprinting by making `toDataURL()` and `getImageData()` return slightly different results each time. |
| `block_webrtc` | `bool` | `false` | Block non-proxied UDP connections. Without this, WebRTC can reveal your real IP even when using a proxy. Enable whenever you use a proxy. |
| `solve_cloudflare` | `bool` | `false` | Automatically detect and solve Cloudflare Turnstile challenges after navigation. Raises the timeout to at least 60 seconds. |

### Validation

`StealthConfig::validate()` is called automatically during `StealthySession::new()`. It:

1. Validates the underlying `BrowserConfig` (same rules as `DynamicSession`)
2. Raises `timeout_ms` to at least 60,000 ms when `solve_cloudflare` is enabled

## Stealth Chromium Flags

`StealthySession` launches Chromium with two layers of flags on top of the standard defaults:

### Default Flags (Always Applied)

Applied to both `DynamicSession` and `StealthySession`. These disable crash reporting, info bars, first-run wizards, and other features that slow down startup.

```
--no-pings
--no-first-run
--disable-infobars
--disable-breakpad
--no-service-autorun
--homepage=about:blank
--password-store=basic
--disable-hang-monitor
--no-default-browser-check
--disable-session-crashed-bubble
--disable-search-engine-choice-screen
```

### Stealth Flags (StealthySession Only)

A comprehensive set of 55+ flags that remove automation indicators and reduce the browser's fingerprint. Key categories:

**Automation concealment:**
- `--disable-blink-features=AutomationControlled` -- removes `navigator.webdriver` flag
- `--test-type` -- suppresses automation-related UI elements

**Fingerprint reduction:**
- `--force-color-profile=srgb` -- reports a standard color profile
- `--font-render-hinting=none` -- normalizes font rendering
- `--blink-settings=primaryHoverType=2,...` -- reports standard pointer/hover capabilities

**Feature disabling:**
- `--disable-sync`, `--disable-translate`, `--disable-voice-input` -- removes features that automated browsers would not use
- `--disable-background-networking` -- prevents background requests that could reveal automation
- `--disable-client-side-phishing-detection` -- removes Google Safe Browsing probes

**Performance:**
- `--disable-dev-shm-usage` -- avoids shared memory issues in containers
- `--aggressive-cache-discard` -- prevents cache from growing unbounded

### Harmful Flag Filtering

Even if you accidentally include automation-revealing flags in `extra_flags`, they are silently stripped before launch:

```
--enable-automation
--disable-popup-blocking
--disable-component-update
--disable-default-apps
--disable-extensions
```

### Stealth Context Emulation

In stealth mode, the browser context is configured to mimic a real desktop Chrome session:

| Setting | Value |
|---|---|
| Screen resolution | 1920 x 1080 |
| Viewport | 1920 x 1080 |
| Device pixel ratio | 2.0 (Retina) |
| Color scheme | Dark |
| Mobile device | No |
| Touch support | No |
| HTTPS errors | Ignored |
| Permissions | Geolocation, notifications (pre-granted) |

## Cloudflare Turnstile Solver

When `solve_cloudflare` is enabled, the session automatically detects and handles Cloudflare challenge pages after each navigation.

### How It Works

1. After the page loads and the network settles, the solver inspects the page content
2. It identifies the challenge type from Cloudflare's internal markers
3. It applies the appropriate solving strategy
4. If the challenge is not resolved, the fetch still returns -- it does not error out

### Challenge Types

The solver handles four types of Cloudflare Turnstile challenges:

| Type | Detection | Strategy |
|---|---|---|
| **Non-interactive** | Page contains `cType: 'non-interactive'` | Polls the page title for up to 60 seconds, waiting for "Just a moment..." to disappear |
| **Managed** | Page contains `cType: 'managed'` | Locates the Turnstile iframe and clicks it, retrying up to 10 times |
| **Interactive** | Page contains `cType: 'interactive'` | Same as managed -- locates and clicks the iframe |
| **Embedded** | Page contains `challenges.cloudflare.com/turnstile/v` | Same as managed -- locates and clicks the iframe |

### Usage

```rust
// Enable globally for all fetches
let config = StealthConfig {
    solve_cloudflare: true,
    base: BrowserConfig {
        // Timeout is auto-raised to 60s when solve_cloudflare is true
        ..Default::default()
    },
    ..Default::default()
};

let mut session = StealthySession::new(config)?;
session.start().await?;
let response = session.fetch("https://cloudflare-protected.com", None).await?;
```

```rust
// Enable per-request via FetchParams
use scrapling_browser::FetchParams;

let config = StealthConfig::default();
let mut session = StealthySession::new(config)?;
session.start().await?;

// Only solve Cloudflare on this specific request
let params = FetchParams {
    solve_cloudflare: Some(true),
    ..Default::default()
};
let response = session.fetch("https://cloudflare-protected.com", Some(params)).await?;
```

If `solve_cloudflare` is set to `true` in both `StealthConfig` and `FetchParams`, the config-level setting takes precedence (it is always enabled).

## Ad Blocking

Enable `block_ads` on the base `BrowserConfig` to block requests to 3,527 known advertising and tracking domains. The blocklist is sourced from Peter Lowe's ad server list and is compiled into the binary.

```rust
let config = StealthConfig {
    base: BrowserConfig {
        block_ads: true,
        ..Default::default()
    },
    ..Default::default()
};
```

You can also add custom domains to block alongside the built-in list:

```rust
use std::collections::HashSet;

let config = StealthConfig {
    base: BrowserConfig {
        block_ads: true,
        blocked_domains: HashSet::from([
            "custom-tracker.example.com".to_string(),
        ]),
        ..Default::default()
    },
    ..Default::default()
};
```

Domain blocking is suffix-based. Adding `"ads.example.com"` also blocks `"sub.ads.example.com"`.

## DNS-over-HTTPS

Encrypt DNS queries from the browser by routing them through Cloudflare's `1.1.1.1` resolver.

```rust
let config = StealthConfig {
    base: BrowserConfig {
        dns_over_https: true,
        ..Default::default()
    },
    ..Default::default()
};
```

This adds the `--dns-over-https-templates` Chromium flag pointing at Cloudflare's resolver endpoint. It prevents DNS-level monitoring and can help avoid DNS-based blocking.

## Complete Example

A fully configured stealth session with proxy rotation, Cloudflare solving, and resource blocking:

```rust
use std::collections::HashSet;
use scrapling_browser::{BrowserConfig, CookieParam, StealthConfig, StealthySession, WaitState};
use scrapling_fetch::{Proxy, ProxyRotator};

let rotator = ProxyRotator::new(vec![
    Proxy::Url("http://proxy1.example.com:8080".to_string()),
    Proxy::Url("http://proxy2.example.com:8080".to_string()),
])?;

let config = StealthConfig {
    allow_webgl: false,
    hide_canvas: true,
    block_webrtc: true,
    solve_cloudflare: true,
    base: BrowserConfig {
        headless: true,
        network_idle: true,
        disable_resources: true,
        block_ads: true,
        dns_over_https: true,
        proxy_rotator: Some(rotator),
        timeout_ms: 60_000.0,
        retries: 5,
        wait_selector: Some("#main-content".to_string()),
        wait_selector_state: WaitState::Visible,
        cookies: vec![
            CookieParam {
                name: "consent".to_string(),
                value: "accepted".to_string(),
                domain: Some(".example.com".to_string()),
                path: Some("/".to_string()),
                url: None,
            },
        ],
        ..Default::default()
    },
};

let mut session = StealthySession::new(config)?;
session.start().await?;

let response = session.fetch("https://heavily-protected.com", None).await?;

if response.is_success() {
    let items = response.css(".product-card");
    for item in items.iter() {
        let name = item.css(".name").first().map(|n| n.text());
        let price = item.css(".price").first().map(|p| p.text());
        println!("{:?}: {:?}", name, price);
    }
}

session.close().await?;
```
