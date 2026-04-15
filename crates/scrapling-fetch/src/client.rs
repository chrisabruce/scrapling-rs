//! HTTP client implementations for making requests.
//!
//! This module provides two client types for different use cases:
//!
//! - [`Fetcher`] -- A stateless client that creates a fresh wreq client for every
//!   request. This is the simplest option and works well when you do not need to
//!   persist cookies or connection state between requests.
//!
//! - [`FetcherSession`] -- A session-based client that maintains a persistent wreq
//!   client with an automatic cookie jar. Use this when you need to log in to a site
//!   and carry cookies across subsequent requests. Call [`open()`](FetcherSession::open)
//!   before making requests and [`close()`](FetcherSession::close) when done.
//!
//! Both clients support automatic retries, proxy rotation, browser impersonation, and
//! per-request configuration overrides via [`RequestConfig`].

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;
use tracing::{debug, error, warn};
use wreq_util::Emulation;

use crate::config::{
    FetcherConfig, FetcherConfigBuilder, FollowRedirects, Impersonate, ParserConfig,
};
use crate::error::{FetchError, Result};
use crate::fingerprint::{default_user_agent, generate_headers};
use crate::proxy::{Proxy, ProxyRotator};
use crate::response::{Response, build_response_async};

fn merge_headers(
    base: &HashMap<String, String>,
    req: &RequestConfig,
    stealth: bool,
    impersonate_enabled: bool,
) -> HashMap<String, String> {
    let mut headers = base.clone();

    if let Some(req_headers) = &req.headers {
        headers.extend(req_headers.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    let keys_lower: std::collections::HashSet<String> =
        headers.keys().map(|k| k.to_lowercase()).collect();

    match (stealth, impersonate_enabled) {
        (true, _) => {
            if !keys_lower.contains("referer") {
                headers.insert("referer".into(), "https://www.google.com/".into());
            }
            if !impersonate_enabled {
                generate_headers(false)
                    .into_iter()
                    .filter(|(k, _)| !keys_lower.contains(&k.to_lowercase()))
                    .for_each(|(k, v)| {
                        headers.insert(k, v);
                    });
            }
        }
        (false, false) if !keys_lower.contains("user-agent") => {
            headers.insert("User-Agent".into(), default_user_agent());
        }
        _ => {}
    }

    headers
}

/// Maps a human-friendly impersonation name (e.g., `"chrome"`, `"firefox135"`) to
/// the corresponding [`wreq_util::Emulation`] profile. Returns `None` if the name
/// is not recognized. Unversioned names like `"chrome"` resolve to the latest
/// available version.
fn resolve_emulation(name: &str) -> Option<Emulation> {
    match name.to_lowercase().as_str() {
        "chrome" | "chrome145" => Some(Emulation::Chrome145),
        "chrome100" => Some(Emulation::Chrome100),
        "chrome120" => Some(Emulation::Chrome120),
        "chrome124" => Some(Emulation::Chrome124),
        "chrome131" => Some(Emulation::Chrome131),
        "chrome136" => Some(Emulation::Chrome136),
        "chrome140" => Some(Emulation::Chrome140),
        "chrome142" => Some(Emulation::Chrome142),
        "chrome143" => Some(Emulation::Chrome143),
        "chrome144" => Some(Emulation::Chrome144),
        "edge" | "edge145" => Some(Emulation::Edge145),
        "edge140" => Some(Emulation::Edge140),
        "edge134" => Some(Emulation::Edge134),
        "safari" | "safari26" => Some(Emulation::Safari26),
        "safari18" => Some(Emulation::Safari18_5),
        "firefox" | "firefox135" => Some(Emulation::Firefox135),
        "firefox133" => Some(Emulation::Firefox133),
        "firefox128" => Some(Emulation::Firefox128),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Request-level overrides
// ---------------------------------------------------------------------------

/// Per-request configuration overrides that take precedence over [`FetcherConfig`] defaults.
///
/// Every field is `Option` -- when `None`, the corresponding value from the fetcher's
/// [`FetcherConfig`] is used instead. This lets you customize individual requests
/// (e.g., use a different proxy or longer timeout) without affecting the global config.
///
/// Pass this to methods like [`Fetcher::get()`] and [`FetcherSession::post()`].
#[derive(Debug, Default)]
pub struct RequestConfig {
    /// Custom headers for this request. These are merged with the fetcher's default
    /// headers, with per-request values taking precedence on name collisions.
    pub headers: Option<HashMap<String, String>>,
    /// Cookies to send with this request, serialized into a single `Cookie` header.
    /// In a [`FetcherSession`], the session's cookie jar is used in addition to these.
    pub cookies: Option<HashMap<String, String>>,
    /// URL query parameters to append to the request URL.
    pub params: Option<HashMap<String, String>>,
    /// Request timeout override in seconds. When set, this replaces the fetcher's
    /// default timeout for this single request.
    pub timeout_secs: Option<u64>,
    /// Redirect policy override for this request.
    pub follow_redirects: Option<FollowRedirects>,
    /// Maximum redirects override for this request.
    pub max_redirects: Option<usize>,
    /// Retry count override for this request. Set to `Some(1)` to disable retries.
    pub retries: Option<u32>,
    /// Retry delay override in seconds for this request.
    pub retry_delay_secs: Option<u64>,
    /// Proxy override for this request. Overrides both the static proxy and the
    /// proxy rotator for this single request.
    pub proxy: Option<Proxy>,
    /// TLS verification override for this request.
    pub verify: Option<bool>,
    /// Browser impersonation override for this request.
    pub impersonate: Option<Impersonate>,
    /// Stealth headers override for this request.
    pub stealthy_headers: Option<bool>,
    /// Raw request body bytes. Mutually exclusive with `json` -- if both are set,
    /// `json` takes precedence.
    pub data: Option<Vec<u8>>,
    /// JSON request body. Automatically serialized and sent with a
    /// `Content-Type: application/json` header. Takes precedence over `data`.
    pub json: Option<Value>,
    /// HTTP basic authentication credentials as `(username, password)`.
    pub auth: Option<(String, String)>,
}

// ---------------------------------------------------------------------------
// Fetcher — async, creates a new wreq client per request
// ---------------------------------------------------------------------------

/// Stateless async HTTP fetcher that creates a new wreq client per request.
///
/// Because a fresh client is built for each request, there is no shared state between
/// calls -- no cookie jar, no persistent connections. This is the right choice for
/// simple scraping tasks, parallel crawling from different IP addresses, or when you
/// want maximum isolation between requests.
///
/// For login flows or multi-step interactions that need cookies, use [`FetcherSession`].
pub struct Fetcher {
    config: FetcherConfig,
    proxy_rotator: Option<ProxyRotator>,
    parser_config: ParserConfig,
}

impl Fetcher {
    /// Creates a new fetcher with default configuration (Chrome impersonation, 30s
    /// timeout, 3 retries, stealth headers enabled).
    pub fn new() -> Self {
        Self {
            config: FetcherConfig::default(),
            proxy_rotator: None,
            parser_config: ParserConfig::default(),
        }
    }

    /// Creates a new fetcher with the given configuration. Use this when you want
    /// full control over the config without going through the builder.
    pub fn with_config(config: FetcherConfig) -> Self {
        Self {
            config,
            proxy_rotator: None,
            parser_config: ParserConfig::default(),
        }
    }

    /// Returns a new [`FetcherConfigBuilder`] for constructing a validated config.
    /// This is a convenience shortcut for `FetcherConfigBuilder::new()`.
    pub fn builder() -> FetcherConfigBuilder {
        FetcherConfigBuilder::new()
    }

    /// Constructs a fetcher from a completed builder. The builder is consumed and
    /// validated. Returns an error if the builder configuration is invalid (e.g., both
    /// a static proxy and a proxy rotator are set).
    pub fn from_builder(builder: FetcherConfigBuilder) -> Result<Self> {
        let (config, rotator) = builder.build()?;
        Ok(Self {
            config,
            proxy_rotator: rotator,
            parser_config: ParserConfig::default(),
        })
    }

    /// Sets the proxy rotator for distributing requests across proxies. Each request
    /// will use the next proxy from the rotator according to its rotation strategy.
    pub fn set_proxy_rotator(&mut self, rotator: ProxyRotator) {
        self.proxy_rotator = Some(rotator);
    }

    /// Sets the parser configuration for HTML processing. This controls adaptive
    /// parsing behavior on the [`Response`] objects returned by this fetcher.
    pub fn set_parser_config(&mut self, parser_config: ParserConfig) {
        self.parser_config = parser_config;
    }

    /// Returns a reference to the current fetcher configuration. Useful for
    /// inspecting defaults or logging the active settings.
    pub fn config(&self) -> &FetcherConfig {
        &self.config
    }

    /// Sends an HTTP GET request to the given URL. Pass `None` for `req` to use
    /// the fetcher's default configuration, or pass a [`RequestConfig`] to override
    /// specific settings for this request.
    pub async fn get(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("GET", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP POST request to the given URL. Use [`RequestConfig::json`] or
    /// [`RequestConfig::data`] to attach a request body.
    pub async fn post(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("POST", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP PUT request to the given URL. Use [`RequestConfig::json`] or
    /// [`RequestConfig::data`] to attach a request body.
    pub async fn put(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("PUT", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP DELETE request to the given URL. Some APIs accept a body with
    /// DELETE requests -- use [`RequestConfig::data`] or [`RequestConfig::json`] if needed.
    pub async fn delete(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("DELETE", url, req.unwrap_or_default()).await
    }

    async fn request(&self, method: &str, url: &str, req: RequestConfig) -> Result<Response> {
        let max_retries = req.retries.unwrap_or(self.config.retries);
        let retry_delay = req.retry_delay_secs.unwrap_or(self.config.retry_delay_secs);
        let static_proxy = req.proxy.clone();

        let mut last_error: Option<FetchError> = None;

        for attempt in 0..max_retries {
            let proxy = match (&self.proxy_rotator, &static_proxy) {
                (Some(rotator), None) => Some(rotator.get_proxy()),
                _ => static_proxy.clone().or_else(|| self.config.proxy.clone()),
            };

            match self
                .execute_request(method, url, &req, proxy.as_ref())
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    match attempt < max_retries - 1 {
                        true => {
                            warn!(attempt = attempt + 1, error = %e, "request failed, retrying in {retry_delay}s");
                            tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                        }
                        false => {
                            error!(attempts = max_retries, error = %e, "all retries exhausted");
                        }
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(FetchError::MaxRetriesExceeded {
            attempts: max_retries,
            last_error: Box::new(last_error.unwrap_or(FetchError::Other("unknown error".into()))),
        })
    }

    async fn execute_request(
        &self,
        method: &str,
        url: &str,
        req: &RequestConfig,
        proxy: Option<&Proxy>,
    ) -> Result<Response> {
        let stealth = req.stealthy_headers.unwrap_or(self.config.stealthy_headers);
        let impersonate = req.impersonate.as_ref().unwrap_or(&self.config.impersonate);
        let impersonate_selected = impersonate.select();
        let timeout = req.timeout_secs.unwrap_or(self.config.timeout_secs);
        let follow = req.follow_redirects.unwrap_or(self.config.follow_redirects);
        let max_redirects = req.max_redirects.unwrap_or(self.config.max_redirects);
        let verify = req.verify.unwrap_or(self.config.verify);

        let final_headers = merge_headers(
            &self.config.headers,
            req,
            stealth,
            impersonate_selected.is_some(),
        );

        // Build wreq client
        let mut client_builder = wreq::Client::builder().timeout(Duration::from_secs(timeout));

        if !verify {
            client_builder = client_builder.cert_verification(false);
        }

        match follow {
            FollowRedirects::None => {
                client_builder = client_builder.redirect(wreq::redirect::Policy::none());
            }
            FollowRedirects::All | FollowRedirects::Safe => {
                client_builder =
                    client_builder.redirect(wreq::redirect::Policy::limited(max_redirects));
            }
        }

        if let Some(p) = proxy {
            let rp = wreq::Proxy::all(p.server())
                .map_err(|e| FetchError::InvalidProxy(e.to_string()))?;
            client_builder = client_builder.proxy(rp);
        }

        let client = client_builder.build()?;

        // Build request with emulation
        let http_method: wreq::Method = method
            .parse()
            .map_err(|_| FetchError::Other(format!("invalid HTTP method: {method}")))?;

        let mut full_url = url::Url::parse(url)?;
        if let Some(params) = &req.params {
            let mut pairs = full_url.query_pairs_mut();
            params.iter().for_each(|(k, v)| {
                pairs.append_pair(k, v);
            });
        }

        let mut request_builder = client.request(http_method, full_url.as_str());

        // Apply browser emulation
        if let Some(browser_name) = impersonate_selected {
            if let Some(emulation) = resolve_emulation(browser_name) {
                request_builder = request_builder.emulation(emulation);
            }
        }

        // Headers
        for (k, v) in &final_headers {
            request_builder = request_builder.header(k.as_str(), v.as_str());
        }

        // Cookies
        if let Some(cookies) = &req.cookies {
            let cookie_str = cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            request_builder = request_builder.header("cookie", cookie_str);
        }

        // Auth
        if let Some((user, pass)) = &req.auth {
            request_builder = request_builder.basic_auth(user, Some(pass));
        }

        // Body
        if let Some(json_body) = &req.json {
            request_builder = request_builder
                .header("content-type", "application/json")
                .body(serde_json::to_vec(json_body)?);
        } else if let Some(data) = &req.data {
            request_builder = request_builder.body(data.clone());
        }

        let request_headers_map = final_headers;

        debug!(method, url, "sending request via wreq");

        let resp = request_builder.send().await?;

        let mut meta = HashMap::new();
        if let Some(p) = proxy {
            meta.insert("proxy".to_owned(), Value::String(p.server().to_owned()));
        }

        build_response_async(resp, request_headers_map, method, meta).await
    }
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// FetcherSession — persistent client with cookie store
// ---------------------------------------------------------------------------

/// Session-based async HTTP fetcher that reuses a persistent client with cookie storage.
///
/// Unlike [`Fetcher`], this struct maintains a single wreq client across requests,
/// which means cookies set by one response are automatically sent with subsequent
/// requests. This is essential for login flows, CSRF-protected forms, and any
/// multi-step interaction where server-side session state matters.
///
/// Lifecycle: create with [`new()`](Self::new), call [`open()`](Self::open) to start
/// the session, make requests, then call [`close()`](Self::close) (or just drop it).
pub struct FetcherSession {
    config: FetcherConfig,
    proxy_rotator: Option<ProxyRotator>,
    parser_config: ParserConfig,
    client: Option<wreq::Client>,
}

impl FetcherSession {
    /// Creates a new session with the given configuration. The session is not yet
    /// active -- you must call [`open()`](Self::open) before making requests.
    pub fn new(config: FetcherConfig) -> Self {
        Self {
            config,
            proxy_rotator: None,
            parser_config: ParserConfig::default(),
            client: None,
        }
    }

    /// Attaches a proxy rotator to the session. Must be called before
    /// [`open()`](Self::open) since the proxy is configured on the underlying client.
    pub fn with_rotator(mut self, rotator: ProxyRotator) -> Self {
        self.proxy_rotator = Some(rotator);
        self
    }

    /// Sets the parser configuration for the session. Controls how responses from
    /// this session parse and interpret HTML.
    pub fn with_parser_config(mut self, parser_config: ParserConfig) -> Self {
        self.parser_config = parser_config;
        self
    }

    /// Opens the session by creating the underlying HTTP client with a cookie store.
    /// Returns an error if the session is already active. After this call, you can
    /// make requests with [`get()`](Self::get), [`post()`](Self::post), etc.
    pub fn open(&mut self) -> Result<()> {
        if self.client.is_some() {
            return Err(FetchError::SessionAlreadyActive);
        }

        let mut builder = wreq::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .cookie_store(true);

        if !self.config.verify {
            builder = builder.cert_verification(false);
        }

        match self.config.follow_redirects {
            FollowRedirects::None => {
                builder = builder.redirect(wreq::redirect::Policy::none());
            }
            FollowRedirects::All | FollowRedirects::Safe => {
                builder =
                    builder.redirect(wreq::redirect::Policy::limited(self.config.max_redirects));
            }
        }

        if let Some(ref p) = self.config.proxy {
            let rp = wreq::Proxy::all(p.server())
                .map_err(|e| FetchError::InvalidProxy(e.to_string()))?;
            builder = builder.proxy(rp);
        }

        self.client = Some(builder.build()?);
        Ok(())
    }

    /// Closes the session and drops the underlying HTTP client. All cookies and
    /// connection state are discarded. The session can be re-opened with [`open()`](Self::open).
    pub fn close(&mut self) {
        self.client = None;
    }

    /// Returns `true` if the session is currently active (i.e., [`open()`](Self::open)
    /// has been called and [`close()`](Self::close) has not).
    pub fn is_active(&self) -> bool {
        self.client.is_some()
    }

    /// Sends an HTTP GET request using the session client. Cookies from prior
    /// responses are automatically included. Returns an error if the session is not active.
    pub async fn get(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("GET", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP POST request using the session client. Use [`RequestConfig::json`]
    /// or [`RequestConfig::data`] to attach a body.
    pub async fn post(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("POST", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP PUT request using the session client. Use [`RequestConfig::json`]
    /// or [`RequestConfig::data`] to attach a body.
    pub async fn put(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("PUT", url, req.unwrap_or_default()).await
    }

    /// Sends an HTTP DELETE request using the session client.
    pub async fn delete(&self, url: &str, req: Option<RequestConfig>) -> Result<Response> {
        self.request("DELETE", url, req.unwrap_or_default()).await
    }

    async fn request(&self, method: &str, url: &str, req: RequestConfig) -> Result<Response> {
        let client = self.client.as_ref().ok_or(FetchError::SessionNotActive)?;

        let stealth = req.stealthy_headers.unwrap_or(self.config.stealthy_headers);
        let impersonate = req.impersonate.as_ref().unwrap_or(&self.config.impersonate);
        let impersonate_selected = impersonate.select();

        let final_headers = merge_headers(
            &self.config.headers,
            &req,
            stealth,
            impersonate_selected.is_some(),
        );

        let http_method: wreq::Method = method
            .parse()
            .map_err(|_| FetchError::Other(format!("invalid HTTP method: {method}")))?;

        let mut full_url = url::Url::parse(url)?;
        if let Some(params) = &req.params {
            let mut pairs = full_url.query_pairs_mut();
            params.iter().for_each(|(k, v)| {
                pairs.append_pair(k, v);
            });
        }

        let mut request_builder = client.request(http_method, full_url.as_str());

        if let Some(browser_name) = impersonate_selected {
            if let Some(emulation) = resolve_emulation(browser_name) {
                request_builder = request_builder.emulation(emulation);
            }
        }

        for (k, v) in &final_headers {
            request_builder = request_builder.header(k.as_str(), v.as_str());
        }

        if let Some(cookies) = &req.cookies {
            let cookie_str = cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            request_builder = request_builder.header("cookie", cookie_str);
        }

        if let Some((user, pass)) = &req.auth {
            request_builder = request_builder.basic_auth(user, Some(pass));
        }

        if let Some(json_body) = &req.json {
            request_builder = request_builder
                .header("content-type", "application/json")
                .body(serde_json::to_vec(json_body)?);
        } else if let Some(data) = &req.data {
            request_builder = request_builder.body(data.clone());
        }

        debug!(method, url, "sending request via wreq session");

        let resp = request_builder.send().await?;

        build_response_async(resp, final_headers, method, HashMap::new()).await
    }
}

impl Drop for FetcherSession {
    fn drop(&mut self) {
        self.close();
    }
}
