//! # scrapling
//!
//! A fast, adaptive web scraping toolkit for Rust — a feature-for-feature port
//! of the Python [scrapling](https://github.com/camoufox/scrapling) library.
//!
//! ## Crate overview
//!
//! This is the **core** crate. It provides:
//!
//! - **[`TextHandler`]** / **[`TextHandlers`]** — enriched string types with
//!   regex extraction, HTML entity decoding, whitespace cleaning, and JSON
//!   parsing. Every method that transforms a string returns a new `TextHandler`
//!   so the enriched type is preserved through chains of operations.
//!
//! - **[`AttributesHandler`]** — a read-only map of HTML element attributes
//!   whose values are `TextHandler`s, giving callers regex and cleaning methods
//!   directly on attribute values.
//!
//! - **[`Error`]** / **[`Result`]** — a structured error enum covering parsing,
//!   selector, encoding, regex, JSON, URL, and (optionally) storage failures.
//!
//! - **[`utils`]** — low-level text cleaning helpers (`clean_spaces`,
//!   `clean_whitespace`, `flatten`) used internally and available for
//!   downstream crates.
//!
//! - **[`selector`]** — HTML parsing, CSS selection with `::text`/`::attr()`
//!   pseudo-elements, DOM navigation, and selector generation.
//!
//! - **[`translator`]** — CSS-to-XPath translation with pseudo-element
//!   support and LRU caching.
//!
//! - **[`storage`]** — persistent element storage trait with a SQLite backend
//!   for adaptive element relocation.
//!
//! - **[`adaptive`]** — structural similarity scoring and element relocation
//!   engine (12-factor scoring algorithm).
//!
//! ## Feature flags
//!
//! | Flag | Default | What it enables |
//! |------|---------|-----------------|
//! | `storage` | **yes** | SQLite-backed persistent element storage via `rusqlite`. |
//!
//! ## Quick start
//!
//! ```rust
//! use scrapling::{TextHandler, TextHandlers, AttributesHandler};
//!
//! // TextHandler wraps a String with extra powers
//! let price = TextHandler::new("Item costs $42.99 today");
//! let matches = price.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
//! assert_eq!(matches[0].as_ref(), "42.99");
//!
//! // AttributesHandler gives read-only access to element attributes
//! let attrs = AttributesHandler::new([
//!     ("class".to_owned(), "price-tag".to_owned()),
//!     ("data-currency".to_owned(), "USD".to_owned()),
//! ]);
//! assert_eq!(attrs["class"].as_ref(), "price-tag");
//! ```

#![warn(missing_docs)]

pub mod adaptive;
pub mod attributes;
pub mod error;
pub mod selector;
pub mod shell;
pub mod storage;
pub mod text;
pub mod translator;
pub mod utils;

// Re-export primary types at crate root for ergonomic imports.
pub use attributes::AttributesHandler;
pub use error::{Error, Result};
pub use selector::ParseOptions;
pub use text::{TextHandler, TextHandlers};
