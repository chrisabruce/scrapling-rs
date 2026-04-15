//! Configuration types for the HTTP fetcher.
//!
//! This module contains all the knobs you can turn to control how requests are made.
//! The central type is [`FetcherConfig`], which holds defaults for timeouts, retries,
//! proxies, browser impersonation, and redirect behavior. Use [`FetcherConfigBuilder`]
//! to construct a validated config with a fluent API.
//!
//! [`ParserConfig`] is a separate, smaller struct that controls how the HTML parser
//! behaves (e.g., whether adaptive parsing is enabled).

use std::collections::HashMap;

use crate::proxy::{Proxy, ProxyRotator};

/// Policy for following HTTP redirects.
///
/// This controls whether the client automatically follows 3xx responses. The default
/// is [`Safe`](FollowRedirects::Safe), which only follows redirects for GET and HEAD
/// requests -- this prevents accidentally re-submitting POST bodies to a new URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowRedirects {
    /// Do not follow any redirects. The caller receives the raw 3xx response and is
    /// responsible for handling the `Location` header manually.
    None,
    /// Follow redirects only for safe (non-mutating) HTTP methods like GET and HEAD.
    /// This is the default and usually what you want.
    Safe,
    /// Follow all redirects regardless of HTTP method, including POST and PUT.
    /// Use with caution -- this can re-submit request bodies to unexpected URLs.
    All,
}

/// Configuration for the HTTP fetcher.
///
/// This struct holds the default settings applied to every request made by a
/// [`Fetcher`](crate::Fetcher) or [`FetcherSession`](crate::FetcherSession).
/// Individual requests can override most of these via [`RequestConfig`](crate::RequestConfig).
/// Use [`FetcherConfigBuilder`] for a validated, fluent construction path.
#[derive(Debug, Clone)]
pub struct FetcherConfig {
    /// The browser impersonation profile to use. Controls which TLS and HTTP/2
    /// fingerprint the client presents to the server. Defaults to Chrome.
    pub impersonate: Impersonate,
    /// Whether to inject stealth headers (Referer, Sec-Ch-Ua, etc.) that make
    /// requests look like they come from a real browser. Enabled by default.
    pub stealthy_headers: bool,
    /// An optional static proxy to route all requests through. If you need to
    /// rotate across multiple proxies, use a [`ProxyRotator`](crate::ProxyRotator) instead.
    pub proxy: Option<Proxy>,
    /// Request timeout in seconds. Defaults to 30. Applies to the entire request
    /// lifecycle including DNS resolution, connection, and response body download.
    pub timeout_secs: u64,
    /// Default headers to include with every request. These are merged with
    /// per-request headers, with per-request values taking precedence on conflict.
    pub headers: HashMap<String, String>,
    /// Maximum number of retry attempts per request. Defaults to 3. Set to 1 to
    /// disable retries entirely.
    pub retries: u32,
    /// Delay in seconds between retry attempts. Defaults to 1. This is a fixed
    /// delay, not exponential backoff.
    pub retry_delay_secs: u64,
    /// The redirect-following policy. Defaults to [`FollowRedirects::Safe`].
    pub follow_redirects: FollowRedirects,
    /// Maximum number of redirects to follow before giving up. Defaults to 30.
    /// Only applies when `follow_redirects` is not [`FollowRedirects::None`].
    pub max_redirects: usize,
    /// Whether to verify TLS certificates. Defaults to `true`. Set to `false`
    /// only for testing against self-signed certificates -- never in production.
    pub verify: bool,
}

impl Default for FetcherConfig {
    fn default() -> Self {
        Self {
            impersonate: Impersonate::default(),
            stealthy_headers: true,
            proxy: None,
            timeout_secs: 30,
            headers: HashMap::new(),
            retries: 3,
            retry_delay_secs: 1,
            follow_redirects: FollowRedirects::Safe,
            max_redirects: 30,
            verify: true,
        }
    }
}

/// Browser impersonation strategy for TLS/HTTP fingerprinting.
///
/// Modern bot-detection services fingerprint the TLS ClientHello and HTTP/2 settings
/// to distinguish real browsers from HTTP libraries. This enum controls which browser
/// profile the underlying wreq client emulates. The default is `Single("chrome")`.
#[derive(Debug, Clone)]
pub enum Impersonate {
    /// No browser impersonation. The client uses wreq's default TLS settings, which
    /// may be detected as non-browser traffic by sophisticated bot-detection systems.
    None,
    /// Impersonate a single specific browser profile for all requests. Pass a string
    /// like `"chrome"`, `"firefox"`, or `"safari"` (see [`client::resolve_emulation`](crate::client)
    /// for the full list of supported names).
    Single(String),
    /// Randomly select from a list of browser profiles on each request. This adds
    /// diversity to your fingerprint, which can help avoid detection when scraping
    /// at scale.
    Random(Vec<String>),
}

impl Default for Impersonate {
    fn default() -> Self {
        Self::Single("chrome".to_owned())
    }
}

impl Impersonate {
    /// Returns the browser profile name to use for the current request, or `None`
    /// if impersonation is disabled.
    ///
    /// For [`Impersonate::Random`], a new profile is selected each time this method
    /// is called, so consecutive calls may return different values.
    pub fn select(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Single(s) => Some(s.as_str()),
            Self::Random(list) => {
                if list.is_empty() {
                    None
                } else {
                    use rand::Rng;
                    let idx = rand::thread_rng().gen_range(0..list.len());
                    Some(list[idx].as_str())
                }
            }
        }
    }
}

/// Builder for constructing a [`FetcherConfig`] with validation.
///
/// The builder provides a fluent API for setting configuration options and catches
/// invalid combinations at build time (e.g., setting both a static proxy and a proxy
/// rotator). Call [`build()`](FetcherConfigBuilder::build) to get the validated config.
///
/// ```rust,no_run
/// # use scrapling_fetch::config::*;
/// let (config, rotator) = FetcherConfigBuilder::new()
///     .timeout_secs(10)
///     .retries(5)
///     .follow_redirects(FollowRedirects::All)
///     .build()
///     .unwrap();
/// ```
pub struct FetcherConfigBuilder {
    config: FetcherConfig,
    proxy_rotator: Option<ProxyRotator>,
}

impl FetcherConfigBuilder {
    /// Creates a new builder pre-populated with the same defaults as
    /// [`FetcherConfig::default()`] -- 30s timeout, 3 retries, Chrome impersonation, etc.
    pub fn new() -> Self {
        Self {
            config: FetcherConfig::default(),
            proxy_rotator: None,
        }
    }

    /// Sets the browser impersonation profile. See [`Impersonate`] for the available
    /// strategies (none, single browser, or random rotation).
    pub fn impersonate(mut self, imp: Impersonate) -> Self {
        self.config.impersonate = imp;
        self
    }

    /// Enables or disables stealth header injection. When enabled, the fetcher adds
    /// browser-like headers (Referer, Sec-Fetch-*, etc.) to help bypass bot detection.
    pub fn stealthy_headers(mut self, enabled: bool) -> Self {
        self.config.stealthy_headers = enabled;
        self
    }

    /// Sets a static proxy for all requests. Cannot be combined with
    /// [`proxy_rotator()`](Self::proxy_rotator) -- the builder will return an error on
    /// [`build()`](Self::build) if both are set.
    pub fn proxy(mut self, proxy: Proxy) -> Self {
        self.config.proxy = Some(proxy);
        self
    }

    /// Sets the request timeout in seconds. This covers the entire request lifecycle
    /// including DNS, TLS handshake, and body download.
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.config.timeout_secs = secs;
        self
    }

    /// Adds a single default header that will be sent with every request.
    /// Call multiple times to add several headers. Per-request headers in
    /// [`RequestConfig`](crate::RequestConfig) take precedence over these.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.headers.insert(name.into(), value.into());
        self
    }

    /// Replaces all default headers with the given map. Any headers previously
    /// added with [`header()`](Self::header) are discarded.
    pub fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.config.headers = headers;
        self
    }

    /// Sets the maximum number of retry attempts. A value of 1 means the request is
    /// tried once with no retries. The default is 3.
    pub fn retries(mut self, retries: u32) -> Self {
        self.config.retries = retries;
        self
    }

    /// Sets the fixed delay in seconds between retries. There is no exponential
    /// backoff -- each retry waits exactly this long.
    pub fn retry_delay_secs(mut self, secs: u64) -> Self {
        self.config.retry_delay_secs = secs;
        self
    }

    /// Sets the redirect-following policy. See [`FollowRedirects`] for the options.
    pub fn follow_redirects(mut self, policy: FollowRedirects) -> Self {
        self.config.follow_redirects = policy;
        self
    }

    /// Sets the maximum number of redirects to follow. If this limit is exceeded,
    /// the request fails with an error rather than looping indefinitely.
    pub fn max_redirects(mut self, max: usize) -> Self {
        self.config.max_redirects = max;
        self
    }

    /// Enables or disables TLS certificate verification. Disabling this is a
    /// security risk and should only be used for testing with self-signed certs.
    pub fn verify(mut self, verify: bool) -> Self {
        self.config.verify = verify;
        self
    }

    /// Sets a proxy rotator for distributing requests across multiple proxies.
    /// Cannot be combined with [`proxy()`](Self::proxy) -- the builder will return
    /// an error on [`build()`](Self::build) if both are set.
    pub fn proxy_rotator(mut self, rotator: ProxyRotator) -> Self {
        self.proxy_rotator = Some(rotator);
        self
    }

    /// Validates and builds the configuration, returning a tuple of the config and
    /// an optional proxy rotator. Returns an error if both a static proxy and a proxy
    /// rotator were configured, since those options are mutually exclusive.
    pub fn build(self) -> crate::error::Result<(FetcherConfig, Option<ProxyRotator>)> {
        if self.proxy_rotator.is_some() && self.config.proxy.is_some() {
            return Err(crate::error::FetchError::InvalidProxy(
                "cannot use proxy_rotator together with static proxy".into(),
            ));
        }
        Ok((self.config, self.proxy_rotator))
    }
}

impl Default for FetcherConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the HTML parser.
///
/// Controls optional parsing features that sit on top of the core scrapling selector
/// engine. Currently this is limited to adaptive parsing, which remembers page
/// structure from prior crawls to improve extraction reliability.
#[derive(Debug, Clone, Default)]
pub struct ParserConfig {
    /// Whether to enable adaptive parsing based on prior page structure. When
    /// enabled, the parser stores structural fingerprints of previously-seen pages
    /// and uses them to locate elements even when the HTML layout changes.
    pub adaptive: bool,
    /// The domain to scope adaptive parsing to. This prevents structural data
    /// from one site bleeding into parsing heuristics for another.
    pub adaptive_domain: String,
}
