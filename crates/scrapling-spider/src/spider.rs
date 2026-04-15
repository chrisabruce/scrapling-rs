//! The `Spider` trait and `CrawlerEngine` -- the heart of scrapling-spider.
//!
//! This module contains the two most important types in the crate:
//!
//! - **[`Spider`]** -- a trait that defines the *what* of a crawl: which URLs to
//!   start from, how to parse responses, which domains to stay within, and
//!   various tuning knobs (concurrency, download delay, retry limits, etc.).
//!
//! - **[`CrawlerEngine`]** -- a struct that implements the *how*. It owns the
//!   scheduler, session manager, robots.txt manager, response cache, and
//!   checkpoint manager, and runs the main crawl loop. You create an engine with
//!   [`CrawlerEngine::new`], then call [`CrawlerEngine::crawl`] to start
//!   fetching.
//!
//! ## Crawl loop overview
//!
//! 1. The engine checks for a checkpoint on disk; if one exists it restores the
//!    pending queue and seen set from the snapshot.
//! 2. If not resuming, it calls [`Spider::start_requests`] and enqueues them.
//! 3. In a loop, it dequeues the highest-priority request, checks domain
//!    restrictions and robots.txt, looks up the dev-mode cache, fetches the URL
//!    via the session manager, handles blocked-response retries, invokes the
//!    request's callback (or the spider's `parse`), collects scraped items, and
//!    enqueues any follow-up requests.
//! 4. The loop ends when the queue is empty, or when a pause/force-stop signal
//!    is received. On pause, a checkpoint is saved; on normal completion, the
//!    checkpoint file is cleaned up.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use tracing::{debug, error, info, warn};

use scrapling_fetch::Response;

use crate::cache::ResponseCacheManager;
use crate::checkpoint::{CheckpointData, CheckpointManager};
use crate::error::{Result, SpiderError};
use crate::request::{Request, SpiderOutput};
use crate::result::{CrawlStats, ItemList};
use crate::robotstxt::RobotsTxtManager;
use crate::scheduler::Scheduler;
use crate::session::{Session, SessionManager};

/// HTTP status codes considered as "blocked" responses that trigger retries.
///
/// When the response status matches one of these codes, the engine treats the
/// request as blocked and retries it (with lower priority and `dont_filter`
/// enabled) up to [`Spider::max_blocked_retries`] times.
const BLOCKED_CODES: &[u16] = &[401, 403, 407, 429, 444, 500, 502, 503, 504];

/// The Spider trait -- implement this to define a web crawler.
///
/// At minimum, implement [`name`](Spider::name), [`start_urls`](Spider::start_urls),
/// and [`parse`](Spider::parse). Everything else has sensible defaults: one
/// default HTTP session, concurrency of 4, no download delay, no domain
/// restrictions, and robots.txt ignored.
///
/// Override the `configure_sessions`, `allowed_domains`, `on_scraped_item`, and
/// other hook methods to customize behavior without touching the engine.
pub trait Spider {
    /// Returns the unique name of this spider. Used in log messages and as a
    /// human-readable identifier. There are no uniqueness constraints enforced
    /// at runtime, but you should keep names distinct to avoid confusion in logs.
    fn name(&self) -> &str;

    /// Returns the initial URLs to crawl. These are converted into
    /// [`Request`] objects by [`start_requests`](Spider::start_requests) and
    /// enqueued into the scheduler at the beginning of the crawl.
    fn start_urls(&self) -> Vec<String>;

    /// Parses a response and returns scraped items and/or follow-up requests.
    ///
    /// This is the core of your spider: extract data from the response body and
    /// return [`SpiderOutput::Item`] values, and/or discover new links and
    /// return [`SpiderOutput::FollowRequest`] values to continue crawling. If a
    /// request has a per-request callback attached, that callback is called
    /// instead of this method.
    fn parse(&self, response: Response) -> Vec<SpiderOutput>;

    /// Returns the set of allowed domains. Requests targeting domains not in
    /// this set are silently dropped and counted in `offsite_requests_count`.
    /// An empty set (the default) means all domains are allowed. Include only
    /// the base domain (e.g., `"example.com"`); subdomains are matched
    /// automatically.
    fn allowed_domains(&self) -> HashSet<String> {
        HashSet::new()
    }
    /// Returns the maximum number of concurrent requests the engine will
    /// dispatch at any given time. Increase this to speed up crawls on sites
    /// that can handle the load; decrease it to be more polite. The default
    /// is 4.
    fn concurrent_requests(&self) -> u32 {
        4
    }
    /// Returns the maximum number of concurrent requests per domain. Set this
    /// to limit the load on any single host while still allowing high overall
    /// concurrency across multiple domains. The default is 0, meaning unlimited
    /// (constrained only by `concurrent_requests`).
    fn concurrent_requests_per_domain(&self) -> u32 {
        0
    }
    /// Returns the delay in seconds between consecutive requests. The engine
    /// sleeps for this duration after dispatching each request. Use this to
    /// throttle your crawl and avoid overloading target servers. The default
    /// is 0.0 (no delay).
    fn download_delay(&self) -> f64 {
        0.0
    }
    /// Returns the maximum number of retries for responses flagged as
    /// "blocked" by [`is_blocked`](Spider::is_blocked). After this many
    /// retries, the engine gives up and logs a warning. Each retry is enqueued
    /// with lower priority and `dont_filter` set to `true`. The default is 3.
    fn max_blocked_retries(&self) -> u32 {
        3
    }
    /// Returns whether the engine should obey robots.txt rules. When `true`,
    /// the engine fetches and caches each domain's `robots.txt` and skips URLs
    /// that are disallowed. The default is `false` (robots.txt is ignored).
    fn robots_txt_obey(&self) -> bool {
        false
    }
    /// Returns whether development mode is enabled. When `true`, the engine
    /// caches every response to disk and serves cached responses on subsequent
    /// runs, eliminating network I/O while you iterate on parse logic. Not
    /// intended for production. The default is `false`.
    fn development_mode(&self) -> bool {
        false
    }
    /// Returns the directory used for the development response cache. If
    /// `None` (the default), the engine falls back to `.scrapling_cache` in
    /// the current working directory. Only relevant when
    /// [`development_mode`](Spider::development_mode) is `true`.
    fn development_cache_dir(&self) -> Option<PathBuf> {
        None
    }
    /// Returns whether to include session kwargs (method, body, etc.) in
    /// request fingerprints. Enable this when your spider makes POST requests
    /// or passes different parameters to the same URL, so that each unique
    /// combination is treated as a distinct request. The default is `false`.
    fn fp_include_kwargs(&self) -> bool {
        false
    }
    /// Returns whether to keep URL fragments (the `#section` part) in request
    /// fingerprints. By default fragments are stripped, so `page#a` and
    /// `page#b` are treated as the same URL. Set to `true` if fragment
    /// differences are meaningful for your target site.
    fn fp_keep_fragments(&self) -> bool {
        false
    }
    /// Returns whether to include HTTP headers in request fingerprints. Enable
    /// this when you send different headers to the same URL and want each
    /// header combination to be treated as a distinct request. The default is
    /// `false`.
    fn fp_include_headers(&self) -> bool {
        false
    }

    /// Configures the session manager with fetcher sessions for this spider.
    ///
    /// The default implementation registers a single stateless `Fetcher` under
    /// the ID `"default"`. Override this to add authenticated sessions,
    /// proxy-routed fetchers, or browser-based sessions.
    fn configure_sessions(&self, manager: &mut SessionManager) {
        let fetcher = scrapling_fetch::Fetcher::new();
        let _ = manager.add("default", Session::Fetcher(fetcher), true);
    }

    /// Builds the initial set of requests from the start URLs. The default
    /// implementation wraps each URL from [`start_urls`](Spider::start_urls)
    /// in a plain [`Request`]. Override this if you need custom priorities,
    /// metadata, or callbacks on your initial requests.
    fn start_requests(&self) -> Vec<Request> {
        self.start_urls().into_iter().map(Request::new).collect()
    }

    /// Called once when the crawl starts. The `resuming` flag is `true` if the
    /// engine is restoring state from a checkpoint rather than starting fresh.
    /// Use this hook for one-time setup like initializing external connections
    /// or logging the crawl start.
    fn on_start(&self, _resuming: bool) {}

    /// Called once when the crawl finishes or is paused. Use this for teardown
    /// tasks like flushing buffers or closing database connections.
    fn on_close(&self) {}

    /// Called when a request fails with an error (network timeout, DNS failure,
    /// etc.). Override this to implement custom error handling such as logging
    /// to an external service or adding the URL to a retry list.
    fn on_error(&self, _request: &Request, _error: &SpiderError) {}

    /// Called for each scraped item before it is added to the
    /// [`ItemList`](crate::result::ItemList). Return `Some(item)` to keep the
    /// item (optionally transforming it), or `None` to drop it. Dropped items
    /// are counted in [`CrawlStats::items_dropped`](crate::result::CrawlStats::items_dropped).
    fn on_scraped_item(&self, item: serde_json::Value) -> Option<serde_json::Value> {
        Some(item)
    }

    /// Returns `true` if the response indicates the request was blocked by the
    /// server. The default implementation checks the status code against a
    /// built-in list of codes commonly associated with rate limiting and access
    /// denial (401, 403, 407, 429, 444, 500, 502, 503, 504). Override this to
    /// add site-specific detection (e.g., checking for CAPTCHA pages in the
    /// response body).
    fn is_blocked(&self, response: &Response) -> bool {
        BLOCKED_CODES.contains(&response.status)
    }
}

/// The crawler engine -- orchestrates the entire crawl loop.
///
/// `CrawlerEngine` is the runtime counterpart to the [`Spider`] trait. It owns
/// all the infrastructure (scheduler, sessions, cache, checkpoint, robots.txt)
/// and drives the fetch-parse-enqueue cycle. Create one with [`new`](CrawlerEngine::new),
/// then call [`crawl`](CrawlerEngine::crawl) to start processing.
///
/// The engine supports graceful pause via [`request_pause`](CrawlerEngine::request_pause):
/// the first call initiates a graceful wind-down (waiting for in-flight
/// requests to finish), and a second call triggers an immediate force stop.
pub struct CrawlerEngine<'a> {
    spider: &'a dyn Spider,
    session_manager: SessionManager,
    scheduler: Scheduler,
    stats: CrawlStats,
    robots_manager: Option<RobotsTxtManager>,
    cache_manager: Option<ResponseCacheManager>,
    checkpoint_manager: Option<CheckpointManager>,
    items: ItemList,
    allowed_domains: HashSet<String>,
    active_tasks: u32,
    pause_requested: bool,
    force_stop: bool,
    /// Whether the crawl is currently in a paused state.
    pub paused: bool,
    last_checkpoint_time: Instant,
    item_sender: Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
}

impl<'a> CrawlerEngine<'a> {
    /// Creates a new crawler engine for the given spider with optional
    /// checkpoint support.
    ///
    /// Pass a `crawldir` path to enable pause/resume checkpointing, or `None`
    /// to disable it. `interval_secs` controls how often auto-checkpoints are
    /// saved during the crawl (0.0 disables periodic saves). The spider's
    /// `configure_sessions` method is called immediately to populate the
    /// session manager; an error is returned if no sessions are registered.
    pub fn new(
        spider: &'a dyn Spider,
        crawldir: Option<PathBuf>,
        interval_secs: f64,
    ) -> Result<Self> {
        let mut session_manager = SessionManager::new();
        spider.configure_sessions(&mut session_manager);

        if session_manager.is_empty() {
            return Err(SpiderError::Session("no sessions configured".into()));
        }

        let scheduler = Scheduler::new(
            spider.fp_include_kwargs(),
            spider.fp_include_headers(),
            spider.fp_keep_fragments(),
        );

        let robots_manager = if spider.robots_txt_obey() {
            Some(RobotsTxtManager::new())
        } else {
            None
        };

        let cache_manager = if spider.development_mode() {
            let dir = spider
                .development_cache_dir()
                .unwrap_or_else(|| PathBuf::from(".scrapling_cache"));
            Some(ResponseCacheManager::new(dir))
        } else {
            None
        };

        let checkpoint_manager = crawldir
            .map(|dir| CheckpointManager::new(dir, interval_secs))
            .transpose()?;

        Ok(Self {
            spider,
            session_manager,
            scheduler,
            stats: CrawlStats::default(),
            robots_manager,
            cache_manager,
            checkpoint_manager,
            items: ItemList::new(),
            allowed_domains: spider.allowed_domains(),
            active_tasks: 0,
            pause_requested: false,
            force_stop: false,
            paused: false,
            last_checkpoint_time: Instant::now(),
            item_sender: None,
        })
    }

    fn is_domain_allowed(&self, request: &Request) -> bool {
        if self.allowed_domains.is_empty() {
            return true;
        }
        let domain = request.domain();
        self.allowed_domains
            .iter()
            .any(|allowed| domain == *allowed || domain.ends_with(&format!(".{allowed}")))
    }

    /// Requests a graceful pause of the crawl. On the first call, the engine
    /// waits for all in-flight requests to finish before saving a checkpoint
    /// and exiting the loop. Calling this a second time triggers a force stop
    /// that abandons in-flight requests immediately.
    pub fn request_pause(&mut self) {
        if self.pause_requested {
            self.force_stop = true;
            warn!("force stop requested");
        } else {
            self.pause_requested = true;
            info!("graceful pause requested");
        }
    }

    /// Runs the main crawl loop and returns aggregate statistics when finished.
    ///
    /// This is the primary entry point for executing a crawl. The method blocks
    /// (asynchronously) until the scheduler is empty and all tasks are done, or
    /// until a pause/force-stop is requested. On success it returns the final
    /// [`CrawlStats`]; check `self.paused` to determine whether the crawl
    /// completed or was interrupted.
    pub async fn crawl(&mut self) -> Result<CrawlStats> {
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        self.stats = CrawlStats {
            start_time: start,
            concurrent_requests: self.spider.concurrent_requests(),
            concurrent_requests_per_domain: self.spider.concurrent_requests_per_domain(),
            download_delay: self.spider.download_delay(),
            ..Default::default()
        };
        self.items = ItemList::new();
        self.pause_requested = false;
        self.force_stop = false;
        self.paused = false;
        self.last_checkpoint_time = Instant::now();

        // Attempt checkpoint restore
        let mut resuming = false;
        if let Some(ref cm) = self.checkpoint_manager {
            if let Ok(Some(cp)) = cm.load() {
                info!(
                    urls = cp.request_urls.len(),
                    seen = cp.seen_fingerprints.len(),
                    "restoring from checkpoint"
                );
                for url in &cp.request_urls {
                    let req = Request::new(url.clone());
                    self.scheduler.enqueue(req);
                }
                resuming = true;
            }
        }

        self.spider.on_start(resuming);

        // Prefetch robots.txt
        if let Some(ref mut rm) = self.robots_manager {
            let urls = self.spider.start_urls();
            let sid = self
                .session_manager
                .default_session_id()
                .unwrap_or("default")
                .to_owned();
            rm.prefetch(&urls, &sid, &self.session_manager).await;
        }

        // Enqueue start requests
        if !resuming {
            let requests = self.spider.start_requests();
            for req in requests {
                if self.is_domain_allowed(&req) {
                    self.scheduler.enqueue(req);
                } else {
                    self.stats.offsite_requests_count += 1;
                }
            }
        }

        // Main crawl loop
        let max_concurrent = self.spider.concurrent_requests();
        let delay = self.spider.download_delay();

        loop {
            if self.pause_requested {
                if self.active_tasks == 0 || self.force_stop {
                    self.save_checkpoint();
                    self.paused = true;
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                continue;
            }

            // Periodic checkpoint
            if self.should_checkpoint() {
                self.save_checkpoint();
            }

            if self.scheduler.is_empty() && self.active_tasks == 0 {
                break;
            }

            if self.scheduler.is_empty() {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                continue;
            }

            if self.active_tasks >= max_concurrent {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                continue;
            }

            if let Some(request) = self.scheduler.dequeue() {
                self.active_tasks += 1;

                if delay > 0.0 {
                    tokio::time::sleep(tokio::time::Duration::from_secs_f64(delay)).await;
                }

                self.process_request(request).await;
                self.active_tasks -= 1;
            }
        }

        self.spider.on_close();

        if !self.paused {
            if let Some(ref cm) = self.checkpoint_manager {
                let _ = cm.cleanup();
            }
        }

        let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        self.stats.end_time = end;

        info!(
            items = self.items.len(),
            requests = self.stats.requests_count,
            elapsed = format!("{:.2}s", self.stats.elapsed_seconds()),
            "crawl complete"
        );

        Ok(self.stats.clone())
    }

    async fn process_request(&mut self, request: Request) {
        // Robots.txt check
        if let Some(ref mut rm) = self.robots_manager {
            let sid = if request.sid.is_empty() {
                self.session_manager
                    .default_session_id()
                    .unwrap_or("default")
                    .to_owned()
            } else {
                request.sid.clone()
            };
            if !rm
                .can_fetch(&request.url, &sid, &self.session_manager)
                .await
            {
                self.stats.robots_disallowed_count += 1;
                debug!(url = %request.url, "disallowed by robots.txt");
                return;
            }
        }

        // Cache check
        if let Some(ref cm) = self.cache_manager {
            if let Some(fp) = request.fingerprint() {
                if let Some(cached) = cm.get(fp) {
                    self.stats.cache_hits += 1;
                    self.run_callbacks(&request, cached).await;
                    return;
                }
                self.stats.cache_misses += 1;
            }
        }

        // Fetch
        let sid = if request.sid.is_empty() {
            self.session_manager
                .default_session_id()
                .unwrap_or("default")
                .to_owned()
        } else {
            request.sid.clone()
        };

        self.stats.increment_requests_count(&sid);

        let response = match self.session_manager.fetch(&request).await {
            Ok(resp) => resp,
            Err(e) => {
                self.stats.failed_requests_count += 1;
                error!(url = %request.url, error = %e, "fetch failed");
                self.spider.on_error(&request, &e);
                return;
            }
        };

        self.stats.increment_status(response.status);
        self.stats
            .increment_response_bytes(&request.domain(), response.body.len() as u64);

        // Cache response
        if let Some(ref cm) = self.cache_manager {
            if let Some(fp) = request.fingerprint() {
                let _ = cm.put(fp, &response, "GET");
            }
        }

        // Blocked check
        if self.spider.is_blocked(&response) {
            self.stats.blocked_requests_count += 1;
            if request.retry_count < self.spider.max_blocked_retries() {
                let mut retry = request.copy_without_callback();
                retry.retry_count += 1;
                retry.priority -= 1;
                retry.dont_filter = true;
                debug!(url = %retry.url, retry = retry.retry_count, "retrying blocked request");
                self.scheduler.enqueue(retry);
            } else {
                warn!(url = %request.url, "max blocked retries exceeded");
            }
            return;
        }

        self.run_callbacks(&request, response).await;
    }

    async fn run_callbacks(&mut self, request: &Request, response: Response) {
        let outputs = if let Some(ref callback) = request.callback {
            callback(response)
        } else {
            self.spider.parse(response)
        };

        for output in outputs {
            match output {
                SpiderOutput::Item(item) => {
                    if let Some(processed) = self.spider.on_scraped_item(item) {
                        self.stats.items_scraped += 1;
                        if let Some(ref tx) = self.item_sender {
                            let _ = tx.send(processed.clone());
                        }
                        self.items.push(processed);
                    } else {
                        self.stats.items_dropped += 1;
                    }
                }
                SpiderOutput::FollowRequest(req) => {
                    if self.is_domain_allowed(&req) {
                        self.scheduler.enqueue(req);
                    } else {
                        self.stats.offsite_requests_count += 1;
                    }
                }
            }
        }
    }

    fn should_checkpoint(&self) -> bool {
        let Some(ref cm) = self.checkpoint_manager else {
            return false;
        };
        if cm.interval_secs == 0.0 {
            return false;
        }
        self.last_checkpoint_time.elapsed().as_secs_f64() >= cm.interval_secs
    }

    fn save_checkpoint(&mut self) {
        let Some(ref cm) = self.checkpoint_manager else {
            return;
        };

        let (requests, seen) = self.scheduler.snapshot();
        let data = CheckpointData {
            request_urls: requests.iter().map(|r| r.url.clone()).collect(),
            seen_fingerprints: seen.iter().cloned().collect(),
        };

        if let Err(e) = cm.save(&data) {
            error!(error = %e, "failed to save checkpoint");
        }
        self.last_checkpoint_time = Instant::now();
    }

    /// Returns a reference to the collected scraped items. You can call this
    /// during or after the crawl to inspect what has been scraped so far.
    pub fn items(&self) -> &ItemList {
        &self.items
    }

    /// Returns a reference to the current crawl statistics. Like `items()`,
    /// this is available both during and after the crawl for monitoring
    /// progress.
    pub fn stats(&self) -> &CrawlStats {
        &self.stats
    }

    /// Creates a streaming receiver that yields items as they are scraped.
    ///
    /// Call this before [`crawl()`](CrawlerEngine::crawl) to get an unbounded
    /// receiver. Each item passes through `on_scraped_item()` and is sent
    /// to both the receiver and the internal `ItemList`.
    ///
    /// This is the Rust equivalent of Python's `async for item in spider.stream()`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut engine = CrawlerEngine::new(&spider, None, 0.0)?;
    /// let mut rx = engine.stream();
    ///
    /// // Spawn the crawl in the background
    /// let crawl_handle = tokio::spawn(async move {
    ///     engine.crawl().await
    /// });
    ///
    /// // Process items as they arrive
    /// while let Some(item) = rx.recv().await {
    ///     println!("Got item: {}", item);
    /// }
    ///
    /// let stats = crawl_handle.await??;
    /// ```
    pub fn stream(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<serde_json::Value> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.item_sender = Some(tx);
        rx
    }
}
