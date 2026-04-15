//! Crawl output types: statistics, scraped items, and the final result.
//!
//! After a crawl completes (or is paused), the engine produces a [`CrawlResult`]
//! containing:
//!
//! - [`CrawlStats`] -- counters for requests, responses, cache hits, blocked
//!   retries, bytes transferred, and more. Stats are also broken down per domain
//!   and per session.
//! - [`ItemList`] -- the collected scraped items as JSON values, with convenience
//!   methods for serializing to `.json` or `.jsonl` files.
//! - A `paused` flag indicating whether the crawl was interrupted by a pause
//!   signal rather than running to natural completion.
//!
//! `CrawlStats` is `Serialize`/`Deserialize` so you can persist it alongside
//! your scraped data for post-crawl analysis.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Aggregate statistics collected during a crawl run.
///
/// The crawler engine populates this struct as it processes requests. After the
/// crawl finishes, you can inspect it via [`CrawlerEngine::stats`](crate::spider::CrawlerEngine::stats)
/// or from the returned [`CrawlResult`]. All counters start at zero and are
/// incremented atomically during the crawl loop.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrawlStats {
    /// Total number of requests dispatched.
    pub requests_count: u64,
    /// Maximum number of concurrent requests allowed.
    pub concurrent_requests: u32,
    /// Maximum number of concurrent requests per domain.
    pub concurrent_requests_per_domain: u32,
    /// Number of requests that failed with an error.
    pub failed_requests_count: u64,
    /// Number of requests rejected because their domain was not allowed.
    pub offsite_requests_count: u64,
    /// Number of requests blocked by robots.txt rules.
    pub robots_disallowed_count: u64,
    /// Number of responses served from the cache.
    pub cache_hits: u64,
    /// Number of responses that were not found in the cache.
    pub cache_misses: u64,
    /// Total bytes received across all responses.
    pub response_bytes: u64,
    /// Number of items successfully scraped.
    pub items_scraped: u64,
    /// Number of items dropped by the item pipeline.
    pub items_dropped: u64,
    /// Unix timestamp when the crawl started.
    pub start_time: f64,
    /// Unix timestamp when the crawl ended.
    pub end_time: f64,
    /// Configured delay in seconds between consecutive requests.
    pub download_delay: f64,
    /// Number of requests that received a blocked status code.
    pub blocked_requests_count: u64,
    /// User-defined custom statistics.
    pub custom_stats: HashMap<String, serde_json::Value>,
    /// Count of responses grouped by HTTP status code.
    pub response_status_count: HashMap<String, u64>,
    /// Total bytes received grouped by domain.
    pub domains_response_bytes: HashMap<String, u64>,
    /// Number of requests dispatched per session.
    pub sessions_requests_count: HashMap<String, u64>,
    /// List of proxy addresses used during the crawl.
    pub proxies: Vec<String>,
    /// Count of log messages grouped by level.
    pub log_levels_counter: HashMap<String, u64>,
}

impl CrawlStats {
    /// Returns the wall-clock duration of the crawl in seconds, computed as
    /// `end_time - start_time`. Both timestamps are Unix epoch seconds recorded
    /// at the start and end of [`CrawlerEngine::crawl`](crate::spider::CrawlerEngine::crawl).
    pub fn elapsed_seconds(&self) -> f64 {
        self.end_time - self.start_time
    }

    /// Returns the average number of requests completed per second over the
    /// entire crawl. Returns 0.0 if the crawl duration was zero (e.g., an
    /// instant crawl with no network calls).
    pub fn requests_per_second(&self) -> f64 {
        let elapsed = self.elapsed_seconds();
        if elapsed == 0.0 {
            0.0
        } else {
            self.requests_count as f64 / elapsed
        }
    }

    /// Increments the counter for the given HTTP status code. Status codes are
    /// stored under keys like `"status_200"` or `"status_404"` in the
    /// `response_status_count` map, making it easy to spot error patterns.
    pub fn increment_status(&mut self, status: u16) {
        let key = format!("status_{status}");
        *self.response_status_count.entry(key).or_insert(0) += 1;
    }

    /// Adds `count` bytes to both the global `response_bytes` total and the
    /// per-domain counter in `domains_response_bytes`. This is called by the
    /// engine after every successful fetch so you can identify bandwidth-heavy
    /// domains.
    pub fn increment_response_bytes(&mut self, domain: &str, count: u64) {
        self.response_bytes += count;
        *self
            .domains_response_bytes
            .entry(domain.to_owned())
            .or_insert(0) += count;
    }

    /// Increments the total `requests_count` and the per-session counter in
    /// `sessions_requests_count`. The engine calls this before every fetch so
    /// you can see how load is distributed across sessions.
    pub fn increment_requests_count(&mut self, sid: &str) {
        self.requests_count += 1;
        *self
            .sessions_requests_count
            .entry(sid.to_owned())
            .or_insert(0) += 1;
    }
}

/// A collection of scraped JSON items with serialization helpers.
///
/// `ItemList` wraps a `Vec<serde_json::Value>` and adds convenience methods for
/// writing the collected data to disk as JSON or JSON Lines. It implements
/// `IntoIterator`, `Index`, and the standard `len` / `is_empty` API so you can
/// treat it like a regular collection.
#[derive(Debug, Default)]
pub struct ItemList(Vec<serde_json::Value>);

impl ItemList {
    /// Creates an empty item list. This is equivalent to `ItemList::default()`
    /// and is what the crawler engine uses at the start of every crawl run.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Appends a JSON item to the list. The engine calls this for every item
    /// that passes through [`Spider::on_scraped_item`](crate::spider::Spider::on_scraped_item)
    /// without being dropped.
    pub fn push(&mut self, item: serde_json::Value) {
        self.0.push(item);
    }

    /// Returns the number of items in the list.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the list contains no items.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the items.
    pub fn iter(&self) -> std::slice::Iter<'_, serde_json::Value> {
        self.0.iter()
    }

    /// Writes all items to a JSON file at `path`, optionally pretty-printed.
    /// Parent directories are created automatically if they do not exist. Pass
    /// `indent: true` for human-readable output or `false` for compact output.
    pub fn to_json(&self, path: &Path, indent: bool) -> std::io::Result<()> {
        path.parent().map(std::fs::create_dir_all).transpose()?;
        let data = match indent {
            true => serde_json::to_vec_pretty(&self.0),
            false => serde_json::to_vec(&self.0),
        }
        .unwrap_or_default();
        std::fs::write(path, data)
    }

    /// Writes all items to a JSON Lines file (one JSON object per line).
    /// This format is convenient for streaming ingestion into data pipelines
    /// because each line is a self-contained JSON document. Parent directories
    /// are created automatically.
    pub fn to_jsonl(&self, path: &Path) -> std::io::Result<()> {
        path.parent().map(std::fs::create_dir_all).transpose()?;
        let content = self
            .0
            .iter()
            .map(|item| serde_json::to_string(item).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(path, content)
    }
}

impl IntoIterator for ItemList {
    type Item = serde_json::Value;
    type IntoIter = std::vec::IntoIter<serde_json::Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ItemList {
    type Item = &'a serde_json::Value;
    type IntoIter = std::slice::Iter<'a, serde_json::Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl std::ops::Index<usize> for ItemList {
    type Output = serde_json::Value;

    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

/// The final output of a crawl run, bundling together statistics, scraped items,
/// and a flag indicating whether the crawl ran to completion or was paused.
///
/// You obtain a `CrawlResult` by calling [`CrawlerEngine::crawl`](crate::spider::CrawlerEngine::crawl).
/// If `paused` is `true`, the engine saved a checkpoint to disk and you can
/// resume later by creating a new engine pointed at the same `crawldir`.
pub struct CrawlResult {
    /// The aggregate crawl statistics for the entire run, including request
    /// counts, byte totals, cache hit/miss ratios, and per-domain breakdowns.
    pub stats: CrawlStats,
    /// The collected scraped items. Use [`ItemList::to_json`] or
    /// [`ItemList::to_jsonl`] to persist them to disk.
    pub items: ItemList,
    /// Whether the crawl was paused (via [`CrawlerEngine::request_pause`](crate::spider::CrawlerEngine::request_pause))
    /// rather than completing naturally. When `true`, a checkpoint was saved and
    /// the crawl can be resumed.
    pub paused: bool,
}

impl CrawlResult {
    /// Returns `true` if the crawl ran to completion (was not paused). This is
    /// the inverse of `self.paused` and exists as a convenience for readability
    /// in conditional checks.
    pub fn completed(&self) -> bool {
        !self.paused
    }

    /// Returns the number of scraped items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if no items were scraped.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
