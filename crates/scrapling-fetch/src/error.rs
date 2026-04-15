//! Error types for the scrapling-fetch crate.
//!
//! Every fallible operation in this crate returns [`Result<T>`], which is an alias
//! for `std::result::Result<T, FetchError>`. The [`FetchError`] enum covers the full
//! range of things that can go wrong -- from low-level network failures through to
//! proxy misconfiguration and session lifecycle mistakes.
//!
//! All variants implement the standard [`Error`](std::error::Error) trait with proper
//! `source()` chaining, so you can inspect the underlying cause when needed.

use std::fmt;

/// Errors that can occur during HTTP fetching operations.
///
/// This is the single error type for the entire crate. It wraps errors from
/// underlying libraries (wreq, url, serde_json) and adds scrapling-specific
/// variants for proxy, session, and retry failures.
#[derive(Debug)]
pub enum FetchError {
    /// An error from the underlying wreq HTTP client, such as a connection failure,
    /// timeout, or TLS handshake error. Inspect the inner [`wreq::Error`] for details.
    Request(wreq::Error),
    /// The URL string could not be parsed. This typically happens when a relative URL
    /// is passed where an absolute one is expected, or when the scheme is missing.
    Url(url::ParseError),
    /// A JSON serialization or deserialization error. This occurs when a
    /// [`RequestConfig::json`](crate::RequestConfig::json) body cannot be serialized,
    /// or when response JSON is malformed.
    Json(serde_json::Error),
    /// The proxy configuration is invalid. The contained string describes what went
    /// wrong -- for example, a duplicate proxy in a rotator or a malformed proxy URL.
    InvalidProxy(String),
    /// A request was attempted on a [`FetcherSession`](crate::FetcherSession) that
    /// has not been opened yet. Call [`open()`](crate::FetcherSession::open) first.
    SessionNotActive,
    /// [`open()`](crate::FetcherSession::open) was called on a session that is
    /// already active. Close the existing session before opening a new one.
    SessionAlreadyActive,
    /// All retry attempts have been exhausted without a successful response. The
    /// `last_error` field contains the error from the final attempt so you can
    /// diagnose the root cause.
    MaxRetriesExceeded {
        /// The total number of attempts made.
        attempts: u32,
        /// The error from the final attempt.
        last_error: Box<FetchError>,
    },
    /// The response has no associated request metadata. This can happen when a
    /// `Response` is constructed manually rather than by the fetcher.
    NoRequest,
    /// A catch-all error with a descriptive message for situations not covered by
    /// the other variants (e.g., an invalid HTTP method string).
    Other(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "HTTP request error: {e}"),
            Self::Url(e) => write!(f, "URL parse error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::InvalidProxy(msg) => write!(f, "invalid proxy: {msg}"),
            Self::SessionNotActive => write!(f, "no active session"),
            Self::SessionAlreadyActive => write!(f, "session already active"),
            Self::MaxRetriesExceeded {
                attempts,
                last_error,
            } => write!(f, "failed after {attempts} attempts: {last_error}"),
            Self::NoRequest => write!(f, "response has no associated request (not from a spider)"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Url(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::MaxRetriesExceeded { last_error, .. } => Some(last_error.as_ref()),
            _ => None,
        }
    }
}

impl From<wreq::Error> for FetchError {
    fn from(e: wreq::Error) -> Self {
        Self::Request(e)
    }
}

impl From<url::ParseError> for FetchError {
    fn from(e: url::ParseError) -> Self {
        Self::Url(e)
    }
}

impl From<serde_json::Error> for FetchError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// A convenience result type alias that pins the error type to [`FetchError`].
///
/// Every public function in this crate that can fail returns this type, so callers
/// only need to handle one error enum.
pub type Result<T> = std::result::Result<T, FetchError>;
