//! Error types for the `scrapling-browser` crate.
//!
//! Every fallible operation in this crate returns [`Result<T>`], which is an alias for
//! `std::result::Result<T, BrowserError>`. The [`BrowserError`] enum covers the major
//! failure categories you can encounter during browser automation: Playwright driver
//! errors, navigation failures, timeouts, page-pool capacity issues, configuration
//! mistakes, and a catch-all `Other` variant.
//!
//! Playwright driver errors are automatically converted via the `From<playwright_rs::Error>`
//! implementation, so the `?` operator works seamlessly in async code that calls into
//! the underlying Playwright bindings.

use std::fmt;

/// Errors that can occur during browser automation operations.
///
/// Each variant carries a human-readable message describing what went wrong.
/// When you match on this enum, the variant tells you the *category* of failure
/// so you can decide whether to retry, reconfigure, or bail out.
#[derive(Debug)]
pub enum BrowserError {
    /// A Playwright driver or protocol error.
    ///
    /// This variant is produced when the underlying Playwright Rust bindings return an
    /// error -- for example, when the browser process crashes, a CDP message fails, or
    /// the driver cannot be started. It is also the target of the automatic
    /// `From<playwright_rs::Error>` conversion.
    Playwright(String),

    /// A page navigation failure.
    ///
    /// Returned when `page.goto()` fails, the page content cannot be extracted after
    /// repeated retries, or the Cloudflare solver encounters an unrecoverable problem.
    /// Check the inner message for details about which URL or step failed.
    Navigation(String),

    /// An operation exceeded its deadline.
    ///
    /// Typically raised when `wait_for_selector` does not find the expected element
    /// within `timeout_ms`. You can increase the timeout in [`BrowserConfig`] or the
    /// per-request [`FetchParams`] to give the page more time to render.
    Timeout(String),

    /// A page-pool capacity or lookup error.
    ///
    /// Returned when you try to register more pages than `max_pages` allows, or when
    /// a page-index lookup fails. This usually means too many concurrent fetches are
    /// in flight for the configured pool size.
    PagePool(String),

    /// An invalid or inconsistent configuration value.
    ///
    /// Raised during [`BrowserConfig::validate`] or [`StealthConfig::validate`] when
    /// a field is out of range (e.g. `max_pages > 50`), mutually exclusive options are
    /// both set (e.g. `proxy` and `proxy_rotator`), or a file path does not exist.
    Config(String),

    /// An uncategorised error.
    ///
    /// Catch-all for errors that do not fit neatly into the other categories.
    /// Inspect the inner message for specifics.
    Other(String),
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Playwright(e) => write!(f, "playwright error: {e}"),
            Self::Navigation(e) => write!(f, "navigation error: {e}"),
            Self::Timeout(e) => write!(f, "timeout: {e}"),
            Self::PagePool(e) => write!(f, "page pool error: {e}"),
            Self::Config(e) => write!(f, "config error: {e}"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BrowserError {}

impl From<playwright_rs::Error> for BrowserError {
    fn from(e: playwright_rs::Error) -> Self {
        Self::Playwright(e.to_string())
    }
}

/// Convenience alias used throughout the crate so that functions can return
/// `Result<T>` without specifying the error type every time. Import this alias
/// alongside [`BrowserError`] when writing code that calls into `scrapling-browser`.
pub type Result<T> = std::result::Result<T, BrowserError>;
