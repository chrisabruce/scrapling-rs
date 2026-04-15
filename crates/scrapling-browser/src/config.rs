//! Configuration types for browser automation sessions.
//!
//! This module contains every knob you can turn when launching a browser through
//! scrapling-browser. The primary entry point is [`BrowserConfig`], which controls
//! how the browser is launched, how pages are navigated, and what requests are
//! blocked. For anti-detection scenarios, [`StealthConfig`] wraps a `BrowserConfig`
//! and adds stealth-specific options such as canvas noise and WebRTC blocking.
//!
//! Most fields on `BrowserConfig` have sensible defaults (see the [`Default`] impl),
//! so you typically only override the two or three settings you care about:
//!
//! ```rust
//! use scrapling_browser::BrowserConfig;
//!
//! let config = BrowserConfig {
//!     headless: true,
//!     disable_resources: true,
//!     block_ads: true,
//!     ..Default::default()
//! };
//! ```
//!
//! When you need to tweak behaviour on a per-request basis without rebuilding the
//! entire config, create a [`FetchParams`] with only the fields you want to override.
//! The session's `fetch` method merges those overrides with the base config into a
//! [`ResolvedFetchParams`] struct that carries the final, concrete values used for
//! that single navigation.
//!
//! # Key types
//!
//! | Type | Purpose |
//! |------|---------|
//! | [`BrowserConfig`] | Session-level browser settings (headless, proxy, timeouts, etc.) |
//! | [`StealthConfig`] | Anti-detection wrapper around `BrowserConfig` |
//! | [`FetchParams`] | Optional per-request overrides |
//! | [`ResolvedFetchParams`] | Fully resolved values after merging `FetchParams` + `BrowserConfig` |
//! | [`ProxyConfig`] | Static proxy server credentials |
//! | [`CookieParam`] | A cookie to inject before navigation |
//! | [`WaitState`] | Required DOM state of a wait selector |
//! | [`PageCallback`] | Async closure invoked on a page at setup or post-navigation |
//! | [`StealthContextOptions`] | Viewport / device emulation values for stealth mode |

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use crate::ad_domains::AD_DOMAINS;
use crate::error::{BrowserError, Result};

/// Browser session configuration -- the central struct that controls how the
/// Playwright browser is launched and how pages are navigated.
///
/// This mirrors the Python `PlaywrightConfig` from the original scrapling library.
/// Every field has a default value (see [`Default`]), so you only need to set the
/// fields relevant to your use case. Call [`validate`](BrowserConfig::validate)
/// before passing the config to a session; sessions call it automatically during
/// construction.
pub struct BrowserConfig {
    /// Maximum number of concurrent browser pages in the pool.
    /// Must be between 1 and 50 inclusive. Higher values allow more parallel fetches
    /// but consume more memory. Defaults to `1`.
    pub max_pages: u32,

    /// Whether to launch the browser in headless mode.
    /// Set to `false` when debugging to see the browser window. Defaults to `true`.
    pub headless: bool,

    /// Block heavyweight resource types (images, fonts, stylesheets) when `true`.
    /// This significantly speeds up page loads when you only need the HTML/DOM.
    /// The exact list of blocked types is defined in [`constants::EXTRA_RESOURCES`].
    /// Defaults to `false`.
    pub disable_resources: bool,

    /// Wait for the network-idle event after navigation.
    /// Useful for SPAs that fetch data after the initial document load, but slows
    /// down fetches on pages with persistent connections (e.g. WebSocket heartbeats).
    /// Defaults to `false`.
    pub network_idle: bool,

    /// Wait for the `DOMContentLoaded` event after navigation.
    /// This is faster than `network_idle` and sufficient for most server-rendered
    /// pages. Defaults to `true`.
    pub load_dom: bool,

    /// Optional CSS selector to wait for before returning the page content.
    /// Use this when the data you need is rendered asynchronously by JavaScript
    /// and you know a specific element that signals the content is ready.
    pub wait_selector: Option<String>,

    /// Required state of the wait selector before proceeding.
    /// For example, `WaitState::Visible` waits until the element is both present
    /// and visible on screen. Defaults to [`WaitState::Attached`].
    pub wait_selector_state: WaitState,

    /// Cookies to inject into the browser context before navigation.
    /// Useful for authenticated scraping -- set session cookies here to skip login flows.
    pub cookies: Vec<CookieParam>,

    /// Prepend a Google search navigation to warm the browser session.
    /// Some bot-detection systems check the browser's navigation history; visiting
    /// Google first can make the session appear more natural. Defaults to `true`.
    pub google_search: bool,

    /// Extra delay in milliseconds to sleep after page load stabilisation.
    /// Use this as a last resort when `wait_selector` and `network_idle` are not
    /// enough. Defaults to `0` (no extra delay).
    pub wait_ms: u64,

    /// IANA timezone identifier to emulate in the browser context (e.g. `"America/New_York"`).
    /// Setting this makes the browser's `Intl` APIs and `Date` objects report the
    /// chosen timezone, which can help avoid location-based bot detection.
    pub timezone_id: Option<String>,

    /// Static proxy server configuration.
    /// Mutually exclusive with `proxy_rotator` -- set one or the other, not both.
    pub proxy: Option<ProxyConfig>,

    /// Rotating proxy provider that supplies a fresh proxy per request.
    /// Mutually exclusive with `proxy` -- set one or the other, not both.
    /// Useful when you need a different IP for each fetch to avoid rate limits.
    pub proxy_rotator: Option<scrapling_fetch::ProxyRotator>,

    /// Additional HTTP headers sent with every request.
    /// These are applied via Playwright's `set_extra_http_headers` and will override
    /// headers of the same name that the browser would normally send.
    pub extra_headers: HashMap<String, String>,

    /// Navigation and action timeout in milliseconds.
    /// Applies to `page.goto()`, selector waits, and other timed operations.
    /// Defaults to `30_000.0` (30 seconds).
    pub timeout_ms: f64,

    /// Path to a JavaScript file evaluated in every new page context.
    /// The script runs before any page code, making it ideal for overriding
    /// `navigator` properties or injecting polyfills. The file must exist on disk.
    pub init_script: Option<String>,

    /// Path to a persistent user-data directory for the browser profile.
    /// When set, the browser stores cookies, local storage, and cache across
    /// sessions, which can help maintain login state between runs.
    pub user_data_dir: Option<String>,

    /// Locale string (e.g. `"en-US"`) to emulate in the browser context.
    /// Affects `navigator.language`, `Accept-Language` headers, and date/number
    /// formatting in JavaScript.
    pub locale: Option<String>,

    /// Launch with the system-installed Chrome instead of bundled Chromium.
    /// The system Chrome may have a different fingerprint than Chromium and may
    /// pass more bot-detection checks. Defaults to `false`.
    pub real_chrome: bool,

    /// WebSocket URL for connecting to an existing Chrome DevTools Protocol endpoint.
    /// Must start with `ws://` or `wss://`. When set, the session attaches to a
    /// running browser instead of launching a new one.
    pub cdp_url: Option<String>,

    /// Custom User-Agent string to set on the browser context.
    /// When `None`, the browser uses its built-in default user agent.
    pub useragent: Option<String>,

    /// Extra command-line flags passed to the browser process.
    /// These are appended after the default and stealth flags. Harmful
    /// automation-revealing flags are automatically filtered out.
    pub extra_flags: Vec<String>,

    /// Set of domain names whose requests will be blocked.
    /// Blocking is suffix-based: adding `"ads.example.com"` also blocks
    /// `"sub.ads.example.com"`. See [`intercept::is_domain_blocked`] for details.
    pub blocked_domains: HashSet<String>,

    /// Merge the built-in ad-domain blocklist into `blocked_domains` when `true`.
    /// The blocklist contains roughly 3,500 known ad and tracker domains sourced
    /// from Peter Lowe's list. Defaults to `false`.
    pub block_ads: bool,

    /// Number of retry attempts for each fetch operation.
    /// Must be between 1 and 10 inclusive. On failure, the session waits
    /// `retry_delay_secs` between attempts. Defaults to `3`.
    pub retries: u32,

    /// Delay in seconds between retry attempts.
    /// Applies when a fetch fails and there are retries remaining.
    /// Defaults to `1.0`.
    pub retry_delay_secs: f64,

    /// URL pattern to capture matching XHR/fetch responses.
    /// When set, the session intercepts network responses whose URL matches this
    /// pattern and includes them in the response. Useful for extracting API data
    /// that the page fetches via AJAX.
    pub capture_xhr: Option<String>,

    /// Path to a custom browser executable.
    /// Use this to point at a specific Chrome/Chromium binary instead of the one
    /// bundled with Playwright. The file must exist on disk.
    pub executable_path: Option<String>,

    /// Enable DNS-over-HTTPS via Cloudflare's resolver.
    /// Adds the `--dns-over-https-templates` Chromium flag pointing at Cloudflare's
    /// `1.1.1.1` DNS endpoint, encrypting DNS queries from the browser process.
    /// Defaults to `false`.
    pub dns_over_https: bool,

    /// Arbitrary key-value configuration forwarded to the selector engine.
    /// This map is passed through to scrapling's selector/parsing layer and can
    /// control how CSS selectors and smart matching behave.
    pub selector_config: HashMap<String, serde_json::Value>,

    /// Async callback invoked on each page immediately after creation.
    /// Use this to perform custom setup like adding request interceptors, injecting
    /// scripts, or configuring page-level settings before navigation begins.
    pub page_setup: Option<PageCallback>,

    /// Async callback invoked on each page after navigation completes.
    /// Use this to perform post-navigation actions like clicking buttons, filling
    /// forms, or scrolling to trigger lazy-loaded content before the HTML is captured.
    pub page_action: Option<PageCallback>,
}

/// Async callback that receives a Playwright page reference.
///
/// This type alias defines the signature for [`BrowserConfig::page_setup`] and
/// [`BrowserConfig::page_action`] callbacks. The closure receives a cloned
/// `playwright_rs::Page` and must return a pinned, `Send` future that resolves
/// to `Result<()>`. Because the closure itself must be `Send + Sync`, it can be
/// shared safely across threads.
pub type PageCallback = Box<
    dyn Fn(
            playwright_rs::Page,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::error::Result<()>> + Send>>
        + Send
        + Sync,
>;

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            max_pages: 1,
            headless: true,
            disable_resources: false,
            network_idle: false,
            load_dom: true,
            wait_selector: None,
            wait_selector_state: WaitState::Attached,
            cookies: Vec::new(),
            google_search: true,
            wait_ms: 0,
            timezone_id: None,
            proxy: None,
            proxy_rotator: None,
            extra_headers: HashMap::new(),
            timeout_ms: 30_000.0,
            init_script: None,
            user_data_dir: None,
            locale: None,
            real_chrome: false,
            cdp_url: None,
            useragent: None,
            extra_flags: Vec::new(),
            blocked_domains: HashSet::new(),
            block_ads: false,
            retries: 3,
            retry_delay_secs: 1.0,
            capture_xhr: None,
            executable_path: None,
            dns_over_https: false,
            selector_config: HashMap::new(),
            page_setup: None,
            page_action: None,
        }
    }
}

impl std::fmt::Debug for BrowserConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserConfig")
            .field("headless", &self.headless)
            .field("timeout_ms", &self.timeout_ms)
            .field("retries", &self.retries)
            .field("max_pages", &self.max_pages)
            .finish_non_exhaustive()
    }
}

impl BrowserConfig {
    /// Validate configuration invariants and populate derived fields.
    ///
    /// This method checks that numeric fields are within acceptable ranges, that
    /// mutually exclusive options are not both set, that file paths exist on disk,
    /// and that the CDP URL (if any) uses a WebSocket scheme. When `block_ads` is
    /// `true`, it also merges the built-in ad-domain list into `blocked_domains`.
    ///
    /// You do not usually need to call this yourself -- [`DynamicSession::new`] and
    /// [`StealthySession::new`] call it automatically during construction.
    pub fn validate(&mut self) -> Result<()> {
        if !(1..=50).contains(&self.max_pages) {
            return Err(BrowserError::Config("max_pages must be 1..50".into()));
        }
        if !(1..=10).contains(&self.retries) {
            return Err(BrowserError::Config("retries must be 1..10".into()));
        }
        if self.proxy.is_some() && self.proxy_rotator.is_some() {
            return Err(BrowserError::Config(
                "cannot use proxy and proxy_rotator together".into(),
            ));
        }
        if let Some(ref cdp) = self.cdp_url {
            if !cdp.starts_with("ws://") && !cdp.starts_with("wss://") {
                return Err(BrowserError::Config(
                    "cdp_url must start with ws:// or wss://".into(),
                ));
            }
        }
        if let Some(ref path) = self.init_script {
            if !Path::new(path).is_file() {
                return Err(BrowserError::Config(format!(
                    "init_script not found: {path}"
                )));
            }
        }
        if let Some(ref path) = self.executable_path {
            if !Path::new(path).is_file() {
                return Err(BrowserError::Config(format!(
                    "executable_path not found: {path}"
                )));
            }
        }
        if self.block_ads {
            for domain in AD_DOMAINS {
                self.blocked_domains.insert((*domain).to_owned());
            }
        }
        Ok(())
    }

    /// Returns `true` if a rotating proxy provider is configured.
    /// When a rotator is present the session creates a fresh browser context per
    /// request so each navigation can use a different proxy address.
    pub fn has_proxy_rotator(&self) -> bool {
        self.proxy_rotator.is_some()
    }

    /// Returns `true` if the session will connect via Chrome DevTools Protocol.
    /// CDP mode attaches to a running browser rather than launching a new process,
    /// which is useful for connecting to remote or containerised browsers.
    pub fn is_cdp(&self) -> bool {
        self.cdp_url.is_some()
    }
}

/// Stealth browser configuration -- extends [`BrowserConfig`] with anti-detection options.
///
/// Use this instead of a bare `BrowserConfig` when scraping sites that employ bot
/// detection (e.g. Cloudflare, DataDome, PerimeterX). The stealth layer adds
/// Chromium CLI flags to block WebRTC leaks, disable WebGL, inject canvas noise,
/// and optionally solve Cloudflare Turnstile challenges automatically.
#[derive(Debug)]
pub struct StealthConfig {
    /// Underlying browser configuration shared with non-stealth sessions.
    /// All standard settings (timeout, proxy, headers, etc.) live here.
    pub base: BrowserConfig,

    /// Allow WebGL rendering (disable to reduce fingerprint surface).
    /// Some fingerprinting services read WebGL renderer strings to identify the
    /// GPU and driver version. Set to `false` to disable WebGL entirely.
    /// Defaults to `true`.
    pub allow_webgl: bool,

    /// Inject noise into canvas image data to thwart canvas fingerprinting.
    /// When enabled, small random perturbations are applied to pixel data returned
    /// by `toDataURL()` and `getImageData()`, making the canvas fingerprint
    /// non-deterministic. Defaults to `false`.
    pub hide_canvas: bool,

    /// Disable non-proxied UDP to prevent WebRTC IP leaks.
    /// Without this flag, WebRTC can reveal the machine's real IP address even
    /// when a proxy is configured. Enable this whenever you use a proxy.
    /// Defaults to `false`.
    pub block_webrtc: bool,

    /// Automatically detect and attempt to solve Cloudflare Turnstile challenges.
    /// When enabled the session inspects the page after navigation and, if a
    /// Cloudflare challenge page is detected, attempts to click through it.
    /// The timeout is automatically raised to at least 60 seconds. Defaults to `false`.
    pub solve_cloudflare: bool,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            base: BrowserConfig::default(),
            allow_webgl: true,
            hide_canvas: false,
            block_webrtc: false,
            solve_cloudflare: false,
        }
    }
}

impl StealthConfig {
    /// Validate the stealth configuration and its underlying `BrowserConfig`.
    /// If `solve_cloudflare` is enabled and the timeout is below 60 seconds, the
    /// timeout is automatically raised to 60 seconds to give the solver enough time.
    pub fn validate(&mut self) -> Result<()> {
        self.base.validate()?;
        if self.solve_cloudflare && self.base.timeout_ms < 60_000.0 {
            self.base.timeout_ms = 60_000.0;
        }
        Ok(())
    }

    /// Build additional Chromium command-line flags required by stealth options.
    /// These flags are appended to the default and stealth args in
    /// [`engine::build_launch_options`] and control WebRTC, canvas, and WebGL behaviour.
    pub fn extra_stealth_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.block_webrtc {
            args.push("--webrtc-ip-handling-policy=disable_non_proxied_udp".into());
        }
        if self.hide_canvas {
            args.push("--fingerprinting-canvas-image-data-noise".into());
        }
        if !self.allow_webgl {
            args.push("--disable-webgl".into());
            args.push("--disable-webgl2".into());
        }
        args
    }

    /// Return default stealth context options (viewport, device emulation, permissions).
    /// These mimic a typical desktop Chrome session at 1920x1080 with a 2x device pixel
    /// ratio, dark colour scheme, and pre-granted geolocation/notification permissions.
    pub fn context_options(&self) -> StealthContextOptions {
        StealthContextOptions {
            color_scheme: "dark".into(),
            device_scale_factor: 2.0,
            screen_width: 1920,
            screen_height: 1080,
            viewport_width: 1920,
            viewport_height: 1080,
            is_mobile: false,
            has_touch: false,
            ignore_https_errors: true,
            permissions: vec!["geolocation".into(), "notifications".into()],
        }
    }
}

/// Browser-context options tuned for stealth emulation.
///
/// These values are applied when creating a Playwright browser context in stealth
/// mode. They describe a plausible desktop browsing environment to make the
/// automated session harder to distinguish from a real user.
#[derive(Debug, Clone)]
pub struct StealthContextOptions {
    /// Preferred color scheme (e.g. `"dark"` or `"light"`).
    /// Bot detectors sometimes check whether this matches the OS setting.
    pub color_scheme: String,

    /// Device pixel ratio to emulate.
    /// A value of `2.0` simulates a Retina/HiDPI display, which is typical for
    /// modern laptops and helps avoid a low-DPR fingerprint.
    pub device_scale_factor: f64,

    /// Emulated screen width in pixels.
    /// Combined with `screen_height`, this sets the reported `screen.width` value
    /// in JavaScript.
    pub screen_width: u32,

    /// Emulated screen height in pixels.
    /// Combined with `screen_width`, this sets the reported `screen.height` value
    /// in JavaScript.
    pub screen_height: u32,

    /// Viewport width in pixels.
    /// This is the visible content area width and affects CSS media queries and
    /// responsive layouts.
    pub viewport_width: u32,

    /// Viewport height in pixels.
    /// This is the visible content area height and affects CSS media queries and
    /// responsive layouts.
    pub viewport_height: u32,

    /// Emulate a mobile device when `true`.
    /// Sets `navigator.userAgentData.mobile` and related hints. Should normally
    /// be `false` for stealth desktop sessions.
    pub is_mobile: bool,

    /// Emulate touch support when `true`.
    /// Sets `navigator.maxTouchPoints` and enables touch events. Should normally
    /// be `false` for stealth desktop sessions.
    pub has_touch: bool,

    /// Accept invalid TLS certificates when `true`.
    /// Useful for scraping internal or staging sites with self-signed certificates.
    /// Defaults to `true` in stealth mode.
    pub ignore_https_errors: bool,

    /// Browser permissions to grant automatically.
    /// Pre-granting permissions like `"geolocation"` and `"notifications"` prevents
    /// permission prompt dialogs that could stall automation.
    pub permissions: Vec<String>,
}

/// Static proxy server connection parameters.
///
/// Use this when you have a single proxy endpoint that all requests should route
/// through. For rotating proxies (a different IP per request), use
/// [`scrapling_fetch::ProxyRotator`] via [`BrowserConfig::proxy_rotator`] instead.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Proxy server URL (e.g. `"http://proxy.example.com:8080"`).
    /// Supports HTTP, HTTPS, and SOCKS5 schemes depending on the proxy provider.
    pub server: String,

    /// Optional proxy authentication username.
    /// Required only when the proxy server demands credentials.
    pub username: Option<String>,

    /// Optional proxy authentication password.
    /// Required only when the proxy server demands credentials.
    pub password: Option<String>,
}

/// Required DOM state of a selector before the wait is satisfied.
///
/// Used with [`BrowserConfig::wait_selector_state`] and
/// [`FetchParams::wait_selector_state`] to control what "ready" means for the
/// element you are waiting on. For example, `Visible` is stricter than `Attached`
/// because the element must also have non-zero dimensions and not be hidden by CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitState {
    /// The element is present in the DOM (may or may not be visible).
    /// This is the least restrictive state and the default.
    Attached,

    /// The element is present in the DOM *and* visible on screen.
    /// "Visible" means the element has non-zero bounding box dimensions and is not
    /// hidden via `display: none`, `visibility: hidden`, or `opacity: 0`.
    Visible,

    /// The element is present in the DOM but *not* visible.
    /// Useful when you need to wait for an element to be hidden (e.g. a loading
    /// spinner disappearing).
    Hidden,

    /// The element has been removed from the DOM entirely.
    /// Useful when you need to wait for a transient element (e.g. a modal overlay)
    /// to go away before capturing the page content.
    Detached,
}

/// A cookie to inject into the browser context before navigation.
///
/// Cookies are added via Playwright's `context.add_cookies()` before the first
/// `page.goto()`. You can use this for session cookies, authentication tokens,
/// or consent flags that skip cookie banners.
#[derive(Debug, Clone)]
pub struct CookieParam {
    /// Cookie name (e.g. `"session_id"`).
    pub name: String,

    /// Cookie value (e.g. `"abc123"`).
    pub value: String,

    /// Domain the cookie is scoped to (e.g. `".example.com"`).
    /// When `None`, the cookie is associated with the URL's domain.
    pub domain: Option<String>,

    /// URL path the cookie is scoped to (e.g. `"/api"`).
    /// Defaults to `"/"` when `None`.
    pub path: Option<String>,

    /// Full URL used to infer domain and path when they are omitted.
    /// Provide this when you want Playwright to derive domain and path automatically.
    pub url: Option<String>,
}

/// Per-fetch parameter overrides -- a subset of [`BrowserConfig`] that can be changed
/// on a per-request basis.
///
/// Every field is `Option` -- when `None`, the value falls back to the session's
/// `BrowserConfig`. Pass a `FetchParams` to [`DynamicSession::fetch`] or
/// [`StealthySession::fetch`] to override specific settings for a single navigation
/// without modifying the session-wide configuration.
#[derive(Debug, Clone, Default)]
pub struct FetchParams {
    /// Override the Google-search warm-up flag for this request.
    pub google_search: Option<bool>,
    /// Override the navigation timeout in milliseconds.
    pub timeout_ms: Option<f64>,
    /// Override the post-load sleep delay in milliseconds.
    pub wait_ms: Option<u64>,
    /// Override the extra HTTP headers for this request.
    pub extra_headers: Option<HashMap<String, String>>,
    /// Override the resource-blocking flag for this request.
    pub disable_resources: Option<bool>,
    /// Override the network-idle wait flag for this request.
    pub network_idle: Option<bool>,
    /// Override the DOM-content-loaded wait flag for this request.
    pub load_dom: Option<bool>,
    /// CSS selector to wait for before returning, overriding the config default.
    pub wait_selector: Option<String>,
    /// Required state of the wait selector, overriding the config default.
    pub wait_selector_state: Option<WaitState>,
    /// Override the set of blocked domains for this request.
    pub blocked_domains: Option<HashSet<String>>,
    /// Enable Cloudflare challenge solving for this request.
    pub solve_cloudflare: Option<bool>,
    /// Override selector-engine configuration for this request.
    pub selector_config: Option<HashMap<String, serde_json::Value>>,
}

impl FetchParams {
    /// Merge these optional overrides with the base `BrowserConfig` to produce resolved values.
    ///
    /// For each field, if the `FetchParams` value is `Some`, it wins; otherwise the
    /// corresponding `BrowserConfig` value is used. The result is a [`ResolvedFetchParams`]
    /// with no `Option` fields, ready for immediate use during navigation.
    pub fn merge_with_config(&self, config: &BrowserConfig) -> ResolvedFetchParams {
        ResolvedFetchParams {
            google_search: self.google_search.unwrap_or(config.google_search),
            timeout_ms: self.timeout_ms.unwrap_or(config.timeout_ms),
            wait_ms: self.wait_ms.unwrap_or(config.wait_ms),
            extra_headers: self
                .extra_headers
                .clone()
                .unwrap_or_else(|| config.extra_headers.clone()),
            disable_resources: self.disable_resources.unwrap_or(config.disable_resources),
            network_idle: self.network_idle.unwrap_or(config.network_idle),
            load_dom: self.load_dom.unwrap_or(config.load_dom),
            wait_selector: self
                .wait_selector
                .clone()
                .or_else(|| config.wait_selector.clone()),
            wait_selector_state: self
                .wait_selector_state
                .unwrap_or(config.wait_selector_state),
            blocked_domains: self
                .blocked_domains
                .clone()
                .unwrap_or_else(|| config.blocked_domains.clone()),
            solve_cloudflare: self.solve_cloudflare.unwrap_or(false),
        }
    }
}

/// Fully resolved fetch parameters produced by merging [`FetchParams`] with [`BrowserConfig`].
///
/// Unlike `FetchParams` (which is all `Option`s), every field here has a concrete
/// value. This struct is constructed internally by [`FetchParams::merge_with_config`]
/// and consumed by the session's navigation logic. You will not normally create one
/// yourself.
#[derive(Debug, Clone)]
pub struct ResolvedFetchParams {
    /// Whether to prepend a Google-search warm-up navigation.
    pub google_search: bool,
    /// Navigation timeout in milliseconds.
    pub timeout_ms: f64,
    /// Post-load sleep delay in milliseconds.
    pub wait_ms: u64,
    /// Extra HTTP headers to send with the request.
    pub extra_headers: HashMap<String, String>,
    /// Block heavyweight resource types when `true`.
    pub disable_resources: bool,
    /// Wait for the network-idle event after navigation.
    pub network_idle: bool,
    /// Wait for `DOMContentLoaded` after navigation.
    pub load_dom: bool,
    /// CSS selector to wait for before returning page content.
    pub wait_selector: Option<String>,
    /// Required state of the wait selector.
    pub wait_selector_state: WaitState,
    /// Domains whose requests should be blocked.
    pub blocked_domains: HashSet<String>,
    /// Attempt to solve Cloudflare challenges when `true`.
    pub solve_cloudflare: bool,
}
