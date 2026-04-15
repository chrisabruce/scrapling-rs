//! Structured error types for the scrapling core crate.
//!
//! All fallible operations in the library return [`Result<T>`], which is an
//! alias for `std::result::Result<T, Error>`. The [`Error`] enum has a variant
//! for each failure domain (parsing, selectors, encoding, etc.) and implements
//! automatic conversion from upstream error types via `#[from]`.
//!
//! # Storage errors
//!
//! The [`Error::Storage`] variant is only available when the `storage` Cargo
//! feature is enabled (it is on by default). When disabled, any code path
//! that would produce a storage error is statically removed.

use thiserror::Error;

/// All errors produced by the scrapling core crate.
///
/// Each variant maps to a specific failure domain. Variants that wrap
/// upstream errors (e.g. [`regex::Error`], [`serde_json::Error`]) use
/// `#[from]` so the `?` operator converts them automatically.
///
/// # Examples
///
/// ```rust
/// use scrapling::Error;
///
/// fn example() -> scrapling::Result<()> {
///     let re = regex::Regex::new("[invalid")?; // auto-converts to Error::Regex
///     Ok(())
/// }
///
/// let err = example().unwrap_err();
/// assert!(matches!(err, Error::Regex(_)));
/// ```
#[derive(Debug, Error)]
pub enum Error {
    /// Failure while parsing an HTML document (e.g. unrecoverable encoding
    /// issues, empty input when a document was expected).
    #[error("HTML parse error: {0}")]
    Parse(String),

    /// A CSS selector string could not be compiled or evaluated.
    #[error("CSS selector error: {0}")]
    CssSelector(String),

    /// An XPath expression could not be compiled or evaluated.
    #[error("XPath error: {0}")]
    XPath(String),

    /// Character encoding detection or conversion failed.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// A regular expression could not be compiled.
    ///
    /// Automatically converted from [`regex::Error`].
    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    /// JSON serialization or deserialization failed.
    ///
    /// Automatically converted from [`serde_json::Error`].
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A URL string could not be parsed.
    ///
    /// Automatically converted from [`url::ParseError`].
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    /// A SQLite storage operation failed.
    ///
    /// Only available when the `storage` feature is enabled.
    /// Automatically converted from [`rusqlite::Error`].
    #[cfg(feature = "storage")]
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    /// Catch-all for errors that don't fit another variant.
    #[error("{0}")]
    Other(String),
}

/// Convenience alias used throughout the crate.
///
/// All public functions that can fail return `Result<T>` instead of
/// `std::result::Result<T, Error>` for brevity.
pub type Result<T> = std::result::Result<T, Error>;
