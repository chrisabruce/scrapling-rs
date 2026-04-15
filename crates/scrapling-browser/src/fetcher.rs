//! High-level browser session types that drive page fetching.
//!
//! This is the main module most users interact with. It provides two session types:
//!
//! - [`DynamicSession`] -- a standard browser automation session. It launches (or
//!   connects to) a Chromium instance, navigates to URLs, waits for the page to
//!   stabilise, and returns a [`scrapling_fetch::Response`] containing the rendered
//!   HTML, status code, headers, and cookies. Use this for sites that render content
//!   with JavaScript but do not actively block bots.
//!
//! - [`StealthySession`] -- an anti-detection session that wraps `DynamicSession`
//!   behaviour with stealth Chromium flags, optional WebRTC/canvas/WebGL
//!   countermeasures, and an automatic Cloudflare Turnstile solver. Use this for
//!   sites protected by Cloudflare, DataDome, or similar services.
//!
//! # Lifecycle
//!
//! Both sessions follow the same three-step lifecycle:
//!
//! 1. **Construct** -- `::new(config)` validates the configuration.
//! 2. **Start** -- `.start().await` launches the browser and creates a context.
//! 3. **Fetch** -- `.fetch(url, params).await` navigates and returns a response.
//!    You can call `fetch` multiple times on the same session.
//! 4. **Close** -- `.close().await` tears down the browser and driver.
//!
//! Each `fetch` call automatically retries up to `config.retries` times with a
//! configurable delay between attempts.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tracing::{debug, error, info, warn};

use crate::config::{BrowserConfig, FetchParams, ResolvedFetchParams, StealthConfig, WaitState};
use crate::engine::{build_launch_options, launch_playwright};
use crate::error::{BrowserError, Result};
use crate::intercept::should_block_request;
use crate::page_pool::PagePool;
use crate::response_factory;

use scrapling_fetch::Response;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn setup_page(
    page: &playwright_rs::Page,
    timeout_ms: f64,
    extra_headers: &HashMap<String, String>,
    disable_resources: bool,
    blocked_domains: &HashSet<String>,
) -> Result<()> {
    page.set_default_timeout(timeout_ms).await;
    page.set_default_navigation_timeout(timeout_ms).await;

    if !extra_headers.is_empty() {
        page.set_extra_http_headers(extra_headers.clone()).await?;
    }

    if disable_resources || !blocked_domains.is_empty() {
        let disable = disable_resources;
        let domains = blocked_domains.clone();

        page.route("**/*", move |route| {
            let domains = domains.clone();
            async move {
                let request = route.request();
                let resource_type = request.resource_type();
                let req_url = request.url();

                if should_block_request(resource_type, req_url, disable, &domains) {
                    route.abort(Some("blockedbyclient")).await
                } else {
                    route.continue_(None).await
                }
            }
        })
        .await?;
    }

    Ok(())
}

async fn wait_for_stability(
    page: &playwright_rs::Page,
    load_dom: bool,
    network_idle: bool,
) -> Result<()> {
    let _ = page
        .wait_for_load_state(Some(playwright_rs::WaitUntil::Load))
        .await;

    if load_dom {
        let _ = page
            .wait_for_load_state(Some(playwright_rs::WaitUntil::DomContentLoaded))
            .await;
    }

    if network_idle {
        let _ = page
            .wait_for_load_state(Some(playwright_rs::WaitUntil::NetworkIdle))
            .await;
    }

    Ok(())
}

async fn wait_for_selector(
    page: &playwright_rs::Page,
    selector: &str,
    _state: WaitState,
    timeout_ms: f64,
) -> Result<()> {
    let locator = page.locator(selector).await;
    let wait_opts = playwright_rs::protocol::WaitForOptions::builder()
        .timeout(timeout_ms)
        .build();
    locator
        .wait_for(Some(wait_opts))
        .await
        .map_err(|e| BrowserError::Timeout(format!("wait_for_selector({selector}): {e}")))?;
    Ok(())
}

async fn initialize_context(
    context: &playwright_rs::BrowserContext,
    config: &BrowserConfig,
) -> Result<()> {
    if let Some(ref script_path) = config.init_script {
        let script = std::fs::read_to_string(script_path)
            .map_err(|e| BrowserError::Config(format!("failed to read init_script: {e}")))?;
        context.add_init_script(&script).await?;
    }

    if !config.cookies.is_empty() {
        let pw_cookies: Vec<playwright_rs::protocol::Cookie> = config
            .cookies
            .iter()
            .map(|c| playwright_rs::protocol::Cookie {
                name: c.name.clone(),
                value: c.value.clone(),
                domain: c.domain.clone().unwrap_or_default(),
                path: c.path.clone().unwrap_or_else(|| "/".into()),
                expires: -1.0,
                http_only: false,
                secure: false,
                same_site: None,
            })
            .collect();
        context.add_cookies(&pw_cookies).await?;
    }

    Ok(())
}

fn make_goto_options(timeout_ms: f64, network_idle: bool) -> playwright_rs::GotoOptions {
    let mut opts =
        playwright_rs::GotoOptions::new().timeout(Duration::from_millis(timeout_ms as u64));
    if network_idle {
        opts = opts.wait_until(playwright_rs::WaitUntil::NetworkIdle);
    }
    opts
}

// ---------------------------------------------------------------------------
// DynamicSession — standard Playwright browser automation
// ---------------------------------------------------------------------------

/// Standard Playwright browser automation session without stealth measures.
///
/// `DynamicSession` manages the full lifecycle of a Chromium browser: launching the
/// Playwright driver, creating a browser context with cookies and init scripts,
/// navigating to pages, waiting for load events, and extracting the rendered HTML
/// into a [`Response`]. It supports static proxies, rotating proxies, CDP connections,
/// resource blocking, domain blocking, and custom page callbacks.
///
/// For sites with bot detection, use [`StealthySession`] instead.
pub struct DynamicSession {
    config: BrowserConfig,
    playwright: Option<playwright_rs::Playwright>,
    browser: Option<playwright_rs::Browser>,
    context: Option<playwright_rs::BrowserContext>,
    page_pool: PagePool,
    is_alive: bool,
}

impl DynamicSession {
    /// Create a new `DynamicSession` from the given configuration, validating it upfront.
    /// The browser is *not* launched yet -- call [`start`](Self::start) to do that.
    /// Returns an error if the configuration fails validation.
    pub fn new(mut config: BrowserConfig) -> Result<Self> {
        config.validate()?;
        let max_pages = config.max_pages;
        Ok(Self {
            config,
            playwright: None,
            browser: None,
            context: None,
            page_pool: PagePool::new(max_pages),
            is_alive: false,
        })
    }

    /// Launch the browser and create the initial browser context.
    ///
    /// Depending on the configuration this will either launch a new Chromium process,
    /// connect to an existing one via CDP, or launch with a rotating proxy provider.
    /// Init scripts and cookies from the config are applied to the context.
    /// You must call this before calling [`fetch`](Self::fetch).
    pub async fn start(&mut self) -> Result<()> {
        let pw = launch_playwright().await?;
        let chromium = pw.chromium();
        let launch_opts = build_launch_options(&self.config, false, &[]);

        if let Some(ref cdp_url) = self.config.cdp_url {
            let browser = chromium.connect_over_cdp(cdp_url, None).await?;
            if !self.config.has_proxy_rotator() {
                let ctx = browser.new_context().await?;
                initialize_context(&ctx, &self.config).await?;
                self.context = Some(ctx);
            }
            self.browser = Some(browser);
        } else if self.config.has_proxy_rotator() {
            let browser = chromium.launch_with_options(launch_opts).await?;
            self.browser = Some(browser);
        } else {
            let browser = chromium.launch_with_options(launch_opts).await?;
            let ctx = browser.new_context().await?;
            initialize_context(&ctx, &self.config).await?;
            self.context = Some(ctx);
            self.browser = Some(browser);
        }

        self.playwright = Some(pw);
        self.is_alive = true;
        info!("DynamicSession started");
        Ok(())
    }

    /// Navigate to `url`, wait for stability, and return the page response with retries.
    ///
    /// Pass an optional [`FetchParams`] to override session-level settings for this
    /// single request. The method retries up to `config.retries` times on failure,
    /// sleeping `config.retry_delay_secs` between attempts.
    pub async fn fetch(&self, url: &str, params: Option<FetchParams>) -> Result<Response> {
        if !self.is_alive {
            return Err(BrowserError::Config("session not started".into()));
        }

        let resolved = params.unwrap_or_default().merge_with_config(&self.config);
        let mut last_error = None;

        for attempt in 0..self.config.retries {
            match self.do_fetch(url, &resolved).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt < self.config.retries - 1 {
                        warn!(attempt = attempt + 1, error = %e, "fetch failed, retrying");
                        tokio::time::sleep(Duration::from_secs_f64(self.config.retry_delay_secs))
                            .await;
                    } else {
                        error!(attempts = self.config.retries, "all retries exhausted");
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(BrowserError::Other("unknown error".into())))
    }

    async fn do_fetch(&self, url: &str, params: &ResolvedFetchParams) -> Result<Response> {
        let page = self.get_page().await?;

        setup_page(
            &page,
            params.timeout_ms,
            &params.extra_headers,
            params.disable_resources,
            &params.blocked_domains,
        )
        .await?;

        if let Some(ref cb) = self.config.page_setup {
            cb(page.clone()).await?;
        }

        let goto_opts = make_goto_options(params.timeout_ms, params.network_idle);

        debug!(url = %url, "navigating");
        let nav_response = page.goto(url, Some(goto_opts)).await?;

        wait_for_stability(&page, params.load_dom, params.network_idle).await?;

        if let Some(ref cb) = self.config.page_action {
            cb(page.clone()).await?;
        }

        if let Some(ref selector) = params.wait_selector {
            wait_for_selector(
                &page,
                selector,
                params.wait_selector_state,
                params.timeout_ms,
            )
            .await?;
        }

        if params.wait_ms > 0 {
            tokio::time::sleep(Duration::from_millis(params.wait_ms)).await;
        }

        let response = response_factory::from_browser_page(
            &page,
            nav_response.as_ref(),
            nav_response.as_ref(),
            HashMap::new(),
            Vec::new(),
        )
        .await?;

        page.close().await?;
        info!(status = response.status, url = url, "fetch complete");
        Ok(response)
    }

    async fn get_page(&self) -> Result<playwright_rs::Page> {
        if let Some(ref ctx) = self.context {
            ctx.new_page().await.map_err(Into::into)
        } else if let Some(ref browser) = self.browser {
            let ctx = browser.new_context().await?;
            ctx.new_page().await.map_err(Into::into)
        } else {
            Err(BrowserError::Config("no browser available".into()))
        }
    }

    /// Close the browser context, browser, and Playwright driver.
    ///
    /// This shuts down resources in reverse order: context, then browser, then
    /// driver. After calling this, [`is_alive`](Self::is_alive) returns `false`
    /// and any subsequent [`fetch`](Self::fetch) calls will fail.
    pub async fn close(&mut self) -> Result<()> {
        if let Some(ctx) = self.context.take() {
            let _ = ctx.close().await;
        }
        if let Some(browser) = self.browser.take() {
            let _ = browser.close().await;
        }
        self.playwright = None;
        self.is_alive = false;
        info!("DynamicSession closed");
        Ok(())
    }

    /// Returns `true` if the session has been started and not yet closed.
    /// Use this to guard against calling `fetch` on a session that has not been
    /// started or has already been shut down.
    pub fn is_alive(&self) -> bool {
        self.is_alive
    }

    /// Return a snapshot of the page pool's current statistics.
    /// See [`PoolStats`](crate::page_pool::PoolStats) for what is included.
    pub fn pool_stats(&self) -> crate::page_pool::PoolStats {
        self.page_pool.stats()
    }
}

// ---------------------------------------------------------------------------
// StealthySession — anti-detection browser automation with Cloudflare solver
// ---------------------------------------------------------------------------

/// Anti-detection browser automation session with optional Cloudflare challenge solving.
///
/// `StealthySession` is the stealth counterpart to [`DynamicSession`]. It uses the
/// same Playwright infrastructure but launches Chromium with additional flags that
/// remove automation indicators, block WebRTC IP leaks, inject canvas noise, and
/// disable WebGL. It also includes a built-in Cloudflare Turnstile solver that
/// detects non-interactive, managed, interactive, and embedded challenges and
/// attempts to click through them automatically.
pub struct StealthySession {
    config: StealthConfig,
    playwright: Option<playwright_rs::Playwright>,
    browser: Option<playwright_rs::Browser>,
    context: Option<playwright_rs::BrowserContext>,
    page_pool: PagePool,
    is_alive: bool,
}

impl StealthySession {
    /// Create a new `StealthySession` from the given configuration, validating it upfront.
    /// The browser is *not* launched yet -- call [`start`](Self::start) to do that.
    /// Returns an error if the configuration fails validation.
    pub fn new(mut config: StealthConfig) -> Result<Self> {
        config.validate()?;
        let max_pages = config.base.max_pages;
        Ok(Self {
            config,
            playwright: None,
            browser: None,
            context: None,
            page_pool: PagePool::new(max_pages),
            is_alive: false,
        })
    }

    /// Launch the browser with stealth flags and create the initial browser context.
    ///
    /// This works like [`DynamicSession::start`] but additionally applies the
    /// stealth CLI flags from [`StealthConfig::extra_stealth_args`] and the
    /// full [`constants::STEALTH_ARGS`] list. You must call this before calling
    /// [`fetch`](Self::fetch).
    pub async fn start(&mut self) -> Result<()> {
        let pw = launch_playwright().await?;
        let chromium = pw.chromium();
        let extra = self.config.extra_stealth_args();
        let launch_opts = build_launch_options(&self.config.base, true, &extra);

        if let Some(ref cdp_url) = self.config.base.cdp_url {
            let browser = chromium.connect_over_cdp(cdp_url, None).await?;
            if !self.config.base.has_proxy_rotator() {
                let ctx = browser.new_context().await?;
                initialize_context(&ctx, &self.config.base).await?;
                self.context = Some(ctx);
            }
            self.browser = Some(browser);
        } else if self.config.base.has_proxy_rotator() {
            let browser = chromium.launch_with_options(launch_opts).await?;
            self.browser = Some(browser);
        } else {
            let browser = chromium.launch_with_options(launch_opts).await?;
            let ctx = browser.new_context().await?;
            initialize_context(&ctx, &self.config.base).await?;
            self.context = Some(ctx);
            self.browser = Some(browser);
        }

        self.playwright = Some(pw);
        self.is_alive = true;
        info!("StealthySession started");
        Ok(())
    }

    /// Navigate to `url` with stealth measures, solve challenges if enabled, and return the response.
    ///
    /// If `solve_cloudflare` is enabled (either in the config or the per-request params),
    /// the Cloudflare Turnstile solver runs after navigation. The method retries up to
    /// `config.base.retries` times on failure.
    pub async fn fetch(&self, url: &str, params: Option<FetchParams>) -> Result<Response> {
        if !self.is_alive {
            return Err(BrowserError::Config("session not started".into()));
        }

        let mut resolved = params
            .unwrap_or_default()
            .merge_with_config(&self.config.base);
        if self.config.solve_cloudflare {
            resolved.solve_cloudflare = true;
        }

        let mut last_error = None;

        for attempt in 0..self.config.base.retries {
            match self.do_fetch(url, &resolved).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt < self.config.base.retries - 1 {
                        warn!(attempt = attempt + 1, error = %e, "stealth fetch failed, retrying");
                        tokio::time::sleep(Duration::from_secs_f64(
                            self.config.base.retry_delay_secs,
                        ))
                        .await;
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(BrowserError::Other("unknown error".into())))
    }

    async fn do_fetch(&self, url: &str, params: &ResolvedFetchParams) -> Result<Response> {
        let page = self.get_page().await?;

        setup_page(
            &page,
            params.timeout_ms,
            &params.extra_headers,
            params.disable_resources,
            &params.blocked_domains,
        )
        .await?;

        if let Some(ref cb) = self.config.base.page_setup {
            cb(page.clone()).await?;
        }

        let goto_opts = make_goto_options(params.timeout_ms, params.network_idle);

        debug!(url = %url, "stealth navigating");
        let nav_response = page.goto(url, Some(goto_opts)).await?;

        wait_for_stability(&page, params.load_dom, params.network_idle).await?;

        if params.solve_cloudflare {
            self.cloudflare_solver(&page).await?;
        }

        if let Some(ref cb) = self.config.base.page_action {
            cb(page.clone()).await?;
        }

        if let Some(ref selector) = params.wait_selector {
            wait_for_selector(
                &page,
                selector,
                params.wait_selector_state,
                params.timeout_ms,
            )
            .await?;
        }

        if params.wait_ms > 0 {
            tokio::time::sleep(Duration::from_millis(params.wait_ms)).await;
        }

        let response = response_factory::from_browser_page(
            &page,
            nav_response.as_ref(),
            nav_response.as_ref(),
            HashMap::new(),
            Vec::new(),
        )
        .await?;

        page.close().await?;
        info!(
            status = response.status,
            url = url,
            "stealth fetch complete"
        );
        Ok(response)
    }

    async fn get_page(&self) -> Result<playwright_rs::Page> {
        if let Some(ref ctx) = self.context {
            ctx.new_page().await.map_err(Into::into)
        } else if let Some(ref browser) = self.browser {
            let ctx = browser.new_context().await?;
            ctx.new_page().await.map_err(Into::into)
        } else {
            Err(BrowserError::Config("no browser available".into()))
        }
    }

    /// Detect and solve Cloudflare Turnstile challenges.
    ///
    /// Inspects the current page content for Cloudflare challenge markers. For
    /// non-interactive challenges it polls the page title for up to 60 seconds. For
    /// managed/interactive/embedded challenges it searches for the Turnstile iframe
    /// and clicks it, retrying up to 10 times with 2-second delays.
    async fn cloudflare_solver(&self, page: &playwright_rs::Page) -> Result<()> {
        let _ = page
            .wait_for_load_state(Some(playwright_rs::WaitUntil::NetworkIdle))
            .await;
        tokio::time::sleep(Duration::from_secs(2)).await;

        let content = page
            .content()
            .await
            .map_err(|e| BrowserError::Navigation(format!("cloudflare solver: {e}")))?;

        let Some(challenge) = detect_cloudflare_challenge(&content) else {
            debug!("no Cloudflare challenge detected");
            return Ok(());
        };

        info!(challenge = %challenge, "Cloudflare challenge detected");

        match challenge.as_str() {
            "non-interactive" => {
                for _ in 0..30 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let title = page.title().await.unwrap_or_default();
                    if !title.contains("Just a moment") {
                        debug!("Cloudflare non-interactive challenge resolved");
                        return Ok(());
                    }
                }
                warn!("Cloudflare non-interactive challenge did not resolve");
            }
            "managed" | "interactive" | "embedded" => {
                let selectors = [
                    "iframe[src*='challenges.cloudflare.com']",
                    "#turnstile-wrapper iframe",
                    ".cf-turnstile iframe",
                ];

                for _ in 0..10 {
                    for selector in &selectors {
                        let locator = page.locator(selector).await;
                        if let Ok(count) = locator.count().await {
                            if count > 0 {
                                debug!(selector, "found Cloudflare iframe, clicking");
                                let _ = locator.first().click(None).await;
                                tokio::time::sleep(Duration::from_secs(3)).await;

                                let new_content = page.content().await.unwrap_or_default();
                                if detect_cloudflare_challenge(&new_content).is_none() {
                                    info!("Cloudflare challenge solved");
                                    return Ok(());
                                }
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                warn!("Cloudflare challenge could not be solved after retries");
            }
            _ => {}
        }

        Ok(())
    }

    /// Close the browser context, browser, and Playwright driver.
    ///
    /// This shuts down resources in reverse order: context, then browser, then
    /// driver. After calling this, [`is_alive`](Self::is_alive) returns `false`
    /// and any subsequent [`fetch`](Self::fetch) calls will fail.
    pub async fn close(&mut self) -> Result<()> {
        if let Some(ctx) = self.context.take() {
            let _ = ctx.close().await;
        }
        if let Some(browser) = self.browser.take() {
            let _ = browser.close().await;
        }
        self.playwright = None;
        self.is_alive = false;
        info!("StealthySession closed");
        Ok(())
    }

    /// Returns `true` if the session has been started and not yet closed.
    /// Use this to guard against calling `fetch` on a session that has not been
    /// started or has already been shut down.
    pub fn is_alive(&self) -> bool {
        self.is_alive
    }

    /// Return a snapshot of the page pool's current statistics.
    /// See [`PoolStats`](crate::page_pool::PoolStats) for what is included.
    pub fn pool_stats(&self) -> crate::page_pool::PoolStats {
        self.page_pool.stats()
    }
}

fn detect_cloudflare_challenge(content: &str) -> Option<String> {
    if content.contains("cType: 'non-interactive'") {
        return Some("non-interactive".into());
    }
    if content.contains("cType: 'managed'") {
        return Some("managed".into());
    }
    if content.contains("cType: 'interactive'") {
        return Some("interactive".into());
    }
    if content.contains("challenges.cloudflare.com/turnstile/v") {
        return Some("embedded".into());
    }
    None
}
