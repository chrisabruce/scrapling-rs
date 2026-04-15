//! # scrapling-spider
//!
//! The web crawling engine for the scrapling-rs framework. This crate provides the
//! orchestration layer that ties together HTTP fetching, request scheduling, response
//! caching, robots.txt compliance, and checkpoint-based pause/resume into a single
//! crawl loop.
//!
//! ## Architecture
//!
//! A crawl is driven by two main abstractions:
//!
//! - **[`Spider`]** -- a trait you implement to define *what* to crawl. You supply
//!   start URLs, a `parse` function that extracts data and follow-up links from each
//!   response, and optional configuration knobs (concurrency, download delay, allowed
//!   domains, etc.).
//!
//! - **[`CrawlerEngine`]** -- the runtime that executes the crawl loop. It owns a
//!   [`Scheduler`] (priority queue with deduplication), a [`SessionManager`] (named
//!   HTTP sessions), and optional managers for robots.txt, response caching, and
//!   checkpointing. You create an engine, hand it your `Spider`, and call
//!   [`CrawlerEngine::crawl`].
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`spider`] | The `Spider` trait and `CrawlerEngine` crawl loop |
//! | [`request`] | `Request`, `Callback`, and `SpiderOutput` types |
//! | [`result`] | `CrawlResult`, `CrawlStats`, and `ItemList` output types |
//! | [`scheduler`] | Priority-queue scheduler with fingerprint deduplication |
//! | [`session`] | `Session` / `SessionManager` for named HTTP backends |
//! | [`cache`] | Filesystem-based HTTP response cache for dev mode |
//! | [`checkpoint`] | Pause/resume support via JSON snapshots on disk |
//! | [`robotstxt`] | Fetching and enforcing robots.txt rules per domain |
//! | [`error`] | `SpiderError` enum and `Result` alias |
//! | [`logging`] | Thread-safe log-level counter for crawl statistics |
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use scrapling_spider::{Spider, CrawlerEngine, Request, SpiderOutput};
//! use scrapling_fetch::Response;
//!
//! struct MyScraper;
//!
//! impl Spider for MyScraper {
//!     fn name(&self) -> &str { "my_scraper" }
//!     fn start_urls(&self) -> Vec<String> {
//!         vec!["https://example.com".into()]
//!     }
//!     fn parse(&self, response: Response) -> Vec<SpiderOutput> {
//!         // Extract data or follow links here
//!         vec![]
//!     }
//! }
//!
//! # async fn run() {
//! let spider = MyScraper;
//! let mut engine = CrawlerEngine::new(&spider, None, 0.0).unwrap();
//! let stats = engine.crawl().await.unwrap();
//! println!("Scraped {} items", stats.items_scraped);
//! # }
//! ```

pub mod cache;
pub mod checkpoint;
pub mod error;
pub mod logging;
pub mod request;
pub mod result;
pub mod robotstxt;
pub mod scheduler;
pub mod session;
pub mod spider;

pub use error::{Result, SpiderError};
pub use request::{Callback, Request, SpiderOutput};
pub use result::{CrawlResult, CrawlStats, ItemList};
pub use scheduler::Scheduler;
pub use session::{Session, SessionManager};
pub use spider::{CrawlerEngine, Spider};
