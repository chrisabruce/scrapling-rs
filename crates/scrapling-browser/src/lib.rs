//! Browser automation crate for the scrapling-rs web scraping framework.
//!
//! This crate provides high-level browser automation built on top of Playwright, giving
//! you two session types for fetching fully-rendered web pages:
//!
//! - [`DynamicSession`] -- a standard Playwright-driven browser that executes JavaScript,
//!   waits for network activity to settle, and returns the final DOM. Use this when the
//!   target site does not employ bot-detection.
//!
//! - [`StealthySession`] -- extends `DynamicSession` with anti-detection measures such as
//!   WebRTC leak prevention, canvas fingerprint noise, automation-flag removal, and an
//!   automatic Cloudflare Turnstile solver. Use this when sites actively block headless
//!   browsers.
//!
//! # Architecture overview
//!
//! ```text
//!                  ┌──────────────┐
//!                  │  Your code   │
//!                  └──────┬───────┘
//!                         │ .fetch(url)
//!          ┌──────────────┴──────────────┐
//!          │  DynamicSession / StealthySession  │  (fetcher.rs)
//!          └──────────────┬──────────────┘
//!                         │
//!       ┌─────────────────┼─────────────────┐
//!       ▼                 ▼                  ▼
//!   engine.rs        intercept.rs      page_pool.rs
//!  (launch opts)   (request blocking)  (page tracking)
//!       │                 │
//!       ▼                 ▼
//!   constants.rs     ad_domains.rs
//!  (CLI flags)     (blocklist data)
//! ```
//!
//! Configuration starts with [`BrowserConfig`] (or [`StealthConfig`] for stealth sessions).
//! Per-request overrides are expressed via [`FetchParams`], which are merged with the
//! session-level config into [`ResolvedFetchParams`] before each navigation.
//!
//! After navigation completes, the [`response_factory`] module extracts the page's HTML,
//! status code, headers, and cookies into a unified [`scrapling_fetch::Response`] that the
//! rest of the scrapling pipeline can parse and query.
//!
//! # Quick example
//!
//! ```rust,no_run
//! use scrapling_browser::{BrowserConfig, DynamicSession};
//!
//! # async fn run() -> scrapling_browser::Result<()> {
//! let config = BrowserConfig {
//!     headless: true,
//!     disable_resources: true,
//!     ..Default::default()
//! };
//!
//! let mut session = DynamicSession::new(config)?;
//! session.start().await?;
//!
//! let response = session.fetch("https://example.com", None).await?;
//! println!("status: {}", response.status);
//!
//! session.close().await?;
//! # Ok(())
//! # }
//! ```

pub mod ad_domains;
pub mod config;
pub mod constants;
pub mod engine;
pub mod error;
pub mod fetcher;
pub mod intercept;
pub mod page_pool;
pub mod response_factory;

pub use config::{
    BrowserConfig, CookieParam, FetchParams, ProxyConfig, ResolvedFetchParams, StealthConfig,
    WaitState,
};
pub use error::{BrowserError, Result};
pub use fetcher::{DynamicSession, StealthySession};
pub use page_pool::{PagePool, PageState, PoolStats};
