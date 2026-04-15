//! Error types for the spider crate.
//!
//! All fallible operations in `scrapling-spider` return [`Result<T>`], which is an
//! alias for `std::result::Result<T, SpiderError>`. The [`SpiderError`] enum has a
//! variant for each subsystem (fetch, browser, session, checkpoint, robots.txt, and
//! configuration) so callers can pattern-match on the source of a failure and decide
//! how to handle it -- for example, retrying on transient fetch errors while
//! immediately aborting on configuration mistakes.
//!
//! Both `scrapling_fetch::FetchError` and `scrapling_browser::BrowserError` implement
//! `Into<SpiderError>`, so the `?` operator works seamlessly when calling into those
//! crates from spider code.

use std::fmt;

/// The central error type for everything that can go wrong during a crawl.
///
/// Each variant wraps either a structured error from a downstream crate or a
/// human-readable `String` describing the problem. You will typically encounter
/// this type through the [`Result`] alias rather than constructing it directly.
#[derive(Debug)]
pub enum SpiderError {
    /// A configuration validation error, raised when spider settings are invalid
    /// (for example, a negative checkpoint interval). The string describes what
    /// was wrong with the configuration.
    Config(String),
    /// An error originating from the HTTP fetch layer (`scrapling-fetch`). This
    /// wraps the underlying `FetchError` so you can inspect network-level details
    /// such as connection timeouts or DNS failures.
    Fetch(scrapling_fetch::FetchError),
    /// An error originating from the browser automation layer (`scrapling-browser`).
    /// This wraps the underlying `BrowserError`, which covers headless-browser
    /// launch failures, page navigation errors, and similar issues.
    Browser(scrapling_browser::BrowserError),
    /// A session management error, raised when a requested session ID does not
    /// exist or when a duplicate session is registered. Check the contained message
    /// for the list of available session IDs.
    Session(String),
    /// A checkpoint save or restore error, raised when the crawler cannot write or
    /// read its state snapshot on disk. Common causes include missing directories
    /// and permission problems.
    Checkpoint(String),
    /// A robots.txt parsing or enforcement error. In practice this variant is
    /// rarely surfaced because the robots.txt manager degrades gracefully (treating
    /// unparseable files as "allow all"), but it exists for explicit error paths.
    RobotsTxt(String),
    /// A catch-all error for uncategorized failures that do not fit into any other
    /// variant. Use this sparingly; prefer a more specific variant when one applies.
    Other(String),
}

impl fmt::Display for SpiderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(e) => write!(f, "config error: {e}"),
            Self::Fetch(e) => write!(f, "fetch error: {e}"),
            Self::Browser(e) => write!(f, "browser error: {e}"),
            Self::Session(e) => write!(f, "session error: {e}"),
            Self::Checkpoint(e) => write!(f, "checkpoint error: {e}"),
            Self::RobotsTxt(e) => write!(f, "robots.txt error: {e}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SpiderError {}

impl From<scrapling_fetch::FetchError> for SpiderError {
    fn from(e: scrapling_fetch::FetchError) -> Self {
        Self::Fetch(e)
    }
}

impl From<scrapling_browser::BrowserError> for SpiderError {
    fn from(e: scrapling_browser::BrowserError) -> Self {
        Self::Browser(e)
    }
}

/// A convenience alias so every function in this crate can write `Result<T>`
/// instead of `std::result::Result<T, SpiderError>`. This is re-exported from
/// the crate root, so downstream code can use `scrapling_spider::Result` directly.
pub type Result<T> = std::result::Result<T, SpiderError>;
