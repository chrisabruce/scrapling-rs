//! HTTP fetching layer for the scrapling-rs web scraping framework.
//!
//! This crate handles everything between "I have a URL" and "I have a parsed HTML
//! response." It builds on top of [`wreq`] (a TLS-fingerprint-aware HTTP client) to
//! make requests that look like they come from real browsers, automatically retries
//! on failure, rotates proxies, and wraps the result in a [`Response`] that lazily
//! parses the HTML body into a scrapling [`Selector`](scrapling::selector::Selector).
//!
//! # Crate architecture
//!
//! The crate is organized into the following modules:
//!
//! | Module | Purpose |
//! |---|---|
//! | [`client`] | The two main entry points: [`Fetcher`] (stateless, one client per request) and [`FetcherSession`] (persistent client with cookie jar). Also defines [`RequestConfig`] for per-request overrides. |
//! | [`config`] | Configuration types shared across the crate: [`FetcherConfig`], its builder, [`Impersonate`] strategy, [`FollowRedirects`] policy, and [`ParserConfig`]. |
//! | [`error`] | A single [`FetchError`] enum and a [`Result`] type alias that every fallible function in this crate returns. |
//! | [`fingerprint`] | Generates realistic browser headers (User-Agent, Sec-Ch-Ua, etc.) so that requests survive bot-detection checks. |
//! | [`proxy`] | [`Proxy`] specification, [`ProxyRotator`] for cycling through a pool of proxies, and helpers like [`is_proxy_error`]. |
//! | [`response`] | The [`Response`] struct -- holds status, headers, cookies, and the raw body bytes. Provides lazy HTML parsing, CSS queries, and Markdown/text conversion. |
//! | [`status`] | A lookup table that maps HTTP status codes to their standard reason phrases. |
//!
//! # Quick start
//!
//! ```rust,ignore
//! use scrapling_fetch::Fetcher;
//!
//! #[tokio::main]
//! async fn main() -> scrapling_fetch::Result<()> {
//!     let fetcher = Fetcher::new();
//!     let response = fetcher.get("https://example.com", None).await?;
//!     let titles = response.css("title");
//!     if let Some(title) = titles.first() {
//!         println!("{}", title.text());
//!     }
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod fingerprint;
pub mod proxy;
pub mod response;
pub mod status;

pub use client::{Fetcher, FetcherSession, RequestConfig};
pub use config::{FetcherConfig, FetcherConfigBuilder, FollowRedirects, Impersonate, ParserConfig};
pub use error::{FetchError, Result};
pub use proxy::{Proxy, ProxyRotator, cyclic_rotation, is_proxy_error};
pub use response::Response;
pub use status::status_text;
