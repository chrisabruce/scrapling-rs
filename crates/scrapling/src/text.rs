//! Enriched string types for web scraping.
//!
//! This module provides [`TextHandler`] and [`TextHandlers`], which wrap
//! `String` and `Vec<TextHandler>` respectively and add scraping-specific
//! methods: regex extraction, HTML entity decoding, whitespace cleaning, and
//! JSON parsing.
//!
//! These types are the Rust equivalent of Python scrapling's
//! `TextHandler(str)` and `TextHandlers(list[TextHandler])` from
//! `core/custom_types.py`.
//!
//! # Design
//!
//! `TextHandler` implements [`Deref<Target = str>`](std::ops::Deref) so all
//! standard `&str` methods are available without wrapping. Methods that
//! transform the string (e.g. [`to_uppercase_text`](TextHandler::to_uppercase_text),
//! [`split_text`](TextHandler::split_text)) return a new `TextHandler` /
//! `TextHandlers` so the enriched type is preserved through chains.
//!
//! `TextHandlers` implements [`Deref<Target = Vec<TextHandler>>`](std::ops::Deref)
//! for the same reason, and adds batch regex operations that fan out to every
//! element and flatten the results.
//!
//! Both types are `Serialize` / `Deserialize` via `#[serde(transparent)]`,
//! meaning they serialise as plain strings / arrays of strings in JSON.
//!
//! # Examples
//!
//! ```
//! use scrapling::TextHandler;
//!
//! let html = TextHandler::new("Price: &#36;42.99 &amp; tax");
//! let cleaned = html.clean(true);              // decode entities + normalise whitespace
//! assert!(cleaned.contains("$42.99"));
//! assert!(cleaned.contains("& tax"));
//!
//! let nums = cleaned.re(r"\d+\.\d+", false, false, true).unwrap();
//! assert_eq!(nums[0].as_ref(), "42.99");
//! ```

use std::fmt;
use std::ops::Deref;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::utils::{clean_whitespace, flatten};

// ---------------------------------------------------------------------------
// TextHandler
// ---------------------------------------------------------------------------

/// An enriched string type that wraps [`String`] and adds regex extraction,
/// HTML entity decoding, whitespace cleaning, and JSON parsing.
///
/// This is the Rust equivalent of Python scrapling's `TextHandler(str)`.
///
/// All standard `&str` methods are available through
/// [`Deref<Target = str>`](Deref). Methods that produce a new string return
/// `TextHandler` (not `String`) so the enriched type is preserved through
/// chained operations.
///
/// # Construction
///
/// ```
/// use scrapling::TextHandler;
///
/// let a = TextHandler::new("hello");
/// let b = TextHandler::from("hello");          // From<&str>
/// let c: TextHandler = "hello".into();         // Into<TextHandler>
/// let d: TextHandler = String::from("hello").into(); // From<String>
/// ```
///
/// # Regex
///
/// [`re()`](TextHandler::re) runs a regex against the string and returns all
/// matches (or capture groups, if present) as a [`TextHandlers`] collection.
/// [`re_first()`](TextHandler::re_first) returns just the first match.
///
/// ```
/// # use scrapling::TextHandler;
/// let t = TextHandler::new("foo 12 bar 34");
/// let matches = t.re(r"\d+", false, false, true).unwrap();
/// assert_eq!(matches.len(), 2);
/// assert_eq!(matches[0].as_ref(), "12");
/// ```
///
/// # Cleaning
///
/// [`clean()`](TextHandler::clean) normalises whitespace (tabs, newlines,
/// carriage returns all become spaces, consecutive spaces are collapsed) and
/// trims leading/trailing whitespace. Optionally decodes HTML entities first.
///
/// ```
/// # use scrapling::TextHandler;
/// let t = TextHandler::new("  hello\t\tworld\n\nfoo  ");
/// assert_eq!(t.clean(false).as_ref(), "hello world foo");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextHandler(String);

impl TextHandler {
    /// Create a new `TextHandler` from anything that converts into a [`String`].
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Consume `self` and return the inner [`String`].
    pub fn into_inner(self) -> String {
        self.0
    }

    // -- Regex ---------------------------------------------------------------

    /// Apply a regex to the string and return all matches as [`TextHandlers`].
    ///
    /// If the pattern contains capture groups, the groups (excluding group 0)
    /// are collected and flattened. If there are no groups, the full match is
    /// returned for each hit.
    ///
    /// # Parameters
    ///
    /// - `regex` — a regular expression pattern string.
    /// - `replace_entities` — if `true`, decode HTML character entities
    ///   (e.g. `&amp;` → `&`) in each match before returning.
    /// - `clean_match` — if `true`, normalise whitespace in the input
    ///   (via [`clean(false)`](TextHandler::clean)) before matching.
    /// - `case_sensitive` — if `false`, the pattern is compiled with the
    ///   case-insensitive flag (`(?i)`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Regex`](crate::Error::Regex) if the pattern is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::TextHandler;
    /// let t = TextHandler::new("price: $42.99 and $10.50");
    /// let m = t.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
    /// assert_eq!(m[0].as_ref(), "42.99");
    /// assert_eq!(m[1].as_ref(), "10.50");
    /// ```
    pub fn re(
        &self,
        regex: &str,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<TextHandlers> {
        let pattern = compile_regex(regex, case_sensitive)?;
        let input = if clean_match {
            self.clean(false)
        } else {
            self.clone()
        };

        let handlers = pattern
            .captures_iter(&input)
            .flat_map(|caps| {
                if caps.len() > 1 {
                    caps.iter()
                        .skip(1)
                        .flatten()
                        .map(|m| m.as_str().to_owned())
                        .collect::<Vec<_>>()
                } else {
                    vec![caps[0].to_owned()]
                }
            })
            .map(|s| {
                if replace_entities {
                    TextHandler::new(htmlize::unescape(&s).into_owned())
                } else {
                    TextHandler::new(s)
                }
            })
            .collect();

        Ok(TextHandlers::new(handlers))
    }

    /// Return `true` if the regex matches anywhere in the string.
    ///
    /// This is a fast path that avoids allocating match results.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Regex`](crate::Error::Regex) if the pattern is invalid.
    pub fn re_matches(&self, regex: &str, case_sensitive: bool) -> Result<bool> {
        let pattern = compile_regex(regex, case_sensitive)?;
        let input: &str = self;
        Ok(pattern.is_match(input))
    }

    /// Apply the regex and return the first match, or `default` if none.
    ///
    /// Parameters have the same meaning as [`re()`](TextHandler::re).
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::TextHandler;
    /// let t = TextHandler::new("order #42 confirmed");
    /// let n = t.re_first(r"\d+", None, false, false, true).unwrap();
    /// assert_eq!(n.unwrap().as_ref(), "42");
    /// ```
    pub fn re_first(
        &self,
        regex: &str,
        default: Option<TextHandler>,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<Option<TextHandler>> {
        let results = self.re(regex, replace_entities, clean_match, case_sensitive)?;
        Ok(results.first().cloned().or(default))
    }

    // -- Cleaning ------------------------------------------------------------

    /// Normalise whitespace and (optionally) decode HTML entities.
    ///
    /// Processing steps:
    /// 1. If `remove_entities` is `true`, decode HTML character entity
    ///    references (e.g. `&amp;` → `&`, `&#x27;` → `'`).
    /// 2. Replace `\t`, `\n`, and `\r` with spaces.
    /// 3. Collapse consecutive spaces into a single space.
    /// 4. Trim leading and trailing whitespace.
    ///
    /// The cleaning table matches Python's `TextHandler.clean()`:
    /// `str.maketrans("\t\r\n", "   ")`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::TextHandler;
    /// let raw = TextHandler::new("  hello\t\tworld\r\n  ");
    /// assert_eq!(raw.clean(false).as_ref(), "hello world");
    /// ```
    pub fn clean(&self, remove_entities: bool) -> TextHandler {
        let data = if remove_entities {
            htmlize::unescape(self.as_ref()).into_owned()
        } else {
            self.0.clone()
        };
        TextHandler::new(clean_whitespace(&data).trim().to_owned())
    }

    // -- JSON ----------------------------------------------------------------

    /// Deserialize the string content as JSON into any type `T`.
    ///
    /// Uses [`serde_json::from_str`] under the hood.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Json`](crate::Error::Json) if the string is not valid
    /// JSON or cannot be deserialized into `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::TextHandler;
    /// let t = TextHandler::new(r#"{"name": "Rust", "year": 2015}"#);
    /// let v: serde_json::Value = t.json().unwrap();
    /// assert_eq!(v["name"], "Rust");
    /// ```
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_str(self).map_err(Error::from)
    }

    // -- String transforms that preserve TextHandler -------------------------

    /// Return an uppercased copy, preserving the `TextHandler` type.
    pub fn to_uppercase_text(&self) -> TextHandler {
        TextHandler::new(self.0.to_uppercase())
    }

    /// Return a lowercased copy, preserving the `TextHandler` type.
    pub fn to_lowercase_text(&self) -> TextHandler {
        TextHandler::new(self.0.to_lowercase())
    }

    /// Replace all occurrences of `from` with `to`, preserving the `TextHandler` type.
    pub fn replace_text(&self, from: &str, to: &str) -> TextHandler {
        TextHandler::new(self.0.replace(from, to))
    }

    /// Trim leading and trailing whitespace, preserving the `TextHandler` type.
    pub fn trim_text(&self) -> TextHandler {
        TextHandler::new(self.0.trim().to_owned())
    }

    /// Trim leading whitespace, preserving the `TextHandler` type.
    pub fn trim_start_text(&self) -> TextHandler {
        TextHandler::new(self.0.trim_start().to_owned())
    }

    /// Trim trailing whitespace, preserving the `TextHandler` type.
    pub fn trim_end_text(&self) -> TextHandler {
        TextHandler::new(self.0.trim_end().to_owned())
    }

    /// Split on `sep` and return a [`TextHandlers`] collection.
    ///
    /// Each segment is a `TextHandler`, so regex/cleaning methods remain
    /// available on every piece.
    pub fn split_text(&self, sep: &str) -> TextHandlers {
        TextHandlers::new(self.0.split(sep).map(TextHandler::new).collect())
    }

    /// Return a copy with characters sorted in Unicode order.
    ///
    /// If `reverse` is `true`, sort in descending order.
    pub fn sort_chars(&self, reverse: bool) -> TextHandler {
        let mut chars: Vec<char> = self.0.chars().collect();
        match reverse {
            true => chars.sort_by(|a, b| b.cmp(a)),
            false => chars.sort(),
        }
        TextHandler::new(chars.into_iter().collect::<String>())
    }

    // -- Scrapy/parsel compatibility aliases ----------------------------------

    /// Identity — returns a reference to `self`.
    ///
    /// Provided for API compatibility with Scrapy/parsel, where `Selector.get()`
    /// returns the first match. On a single `TextHandler` there is nothing to
    /// select, so this is a no-op.
    pub fn get(&self) -> &TextHandler {
        self
    }

    /// Identity — returns a reference to `self`.
    ///
    /// Provided for API compatibility with Scrapy/parsel.
    pub fn getall(&self) -> &TextHandler {
        self
    }
}

impl Deref for TextHandler {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TextHandler {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TextHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TextHandler {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TextHandler {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<TextHandler> for String {
    fn from(t: TextHandler) -> Self {
        t.0
    }
}

// ---------------------------------------------------------------------------
// TextHandlers
// ---------------------------------------------------------------------------

/// A collection of [`TextHandler`] values with batch regex operations.
///
/// This is the Rust equivalent of Python scrapling's `TextHandlers(list[TextHandler])`.
///
/// `TextHandlers` wraps a `Vec<TextHandler>` and exposes it via
/// [`Deref`], so standard slice/vec methods (indexing, iteration, `len`,
/// `is_empty`, etc.) work directly. Batch methods like [`re()`](TextHandlers::re)
/// fan out to every element and flatten the results into a new `TextHandlers`.
///
/// # Construction
///
/// ```
/// use scrapling::{TextHandler, TextHandlers};
///
/// let handlers = TextHandlers::new(vec![
///     TextHandler::new("price: $10"),
///     TextHandler::new("tax: $2"),
/// ]);
///
/// // Or collect from an iterator:
/// let from_iter: TextHandlers = vec!["a", "b", "c"]
///     .into_iter()
///     .map(TextHandler::new)
///     .collect();
/// ```
///
/// # Batch regex
///
/// ```
/// # use scrapling::{TextHandler, TextHandlers};
/// let handlers = TextHandlers::new(vec![
///     TextHandler::new("item 1 costs $10"),
///     TextHandler::new("item 2 costs $20"),
/// ]);
/// let prices = handlers.re(r"\$(\d+)", false, false, true).unwrap();
/// assert_eq!(prices.len(), 2);
/// assert_eq!(prices[0].as_ref(), "10");
/// assert_eq!(prices[1].as_ref(), "20");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextHandlers(Vec<TextHandler>);

impl TextHandlers {
    /// Create a new `TextHandlers` from a pre-built vector.
    pub fn new(items: Vec<TextHandler>) -> Self {
        Self(items)
    }

    /// Consume `self` and return the inner `Vec<TextHandler>`.
    pub fn into_inner(self) -> Vec<TextHandler> {
        self.0
    }

    /// Apply `regex` to every element and flatten all matches into a new
    /// `TextHandlers`.
    ///
    /// Parameters have the same meaning as [`TextHandler::re()`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Regex`](crate::Error::Regex) if the pattern is invalid.
    pub fn re(
        &self,
        regex: &str,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<TextHandlers> {
        self.0
            .iter()
            .map(|h| {
                h.re(regex, replace_entities, clean_match, case_sensitive)
                    .map(TextHandlers::into_inner)
            })
            .collect::<Result<Vec<_>>>()
            .map(|vecs| TextHandlers::new(flatten(vecs)))
    }

    /// Return the first regex match across all elements, or `default` if none.
    ///
    /// Iterates through elements in order, returning as soon as a match is found.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Regex`](crate::Error::Regex) if the pattern is invalid.
    pub fn re_first(
        &self,
        regex: &str,
        default: Option<TextHandler>,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<Option<TextHandler>> {
        self.0
            .iter()
            .try_fold(None, |acc, handler| match acc {
                Some(_) => Ok(acc),
                None => handler
                    .re(regex, replace_entities, clean_match, case_sensitive)
                    .map(|m| m.first().cloned()),
            })
            .map(|found| found.or(default))
    }

    /// Return the first element, or `None` if the collection is empty.
    ///
    /// Provided for Scrapy/parsel API compatibility (`.get()` / `.extract_first`).
    pub fn get(&self) -> Option<&TextHandler> {
        self.0.first()
    }

    /// Return all elements as a slice.
    ///
    /// Provided for Scrapy/parsel API compatibility (`.getall()` / `.extract`).
    pub fn getall(&self) -> &[TextHandler] {
        &self.0
    }
}

impl Deref for TextHandlers {
    type Target = Vec<TextHandler>;
    fn deref(&self) -> &Vec<TextHandler> {
        &self.0
    }
}

impl IntoIterator for TextHandlers {
    type Item = TextHandler;
    type IntoIter = std::vec::IntoIter<TextHandler>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a TextHandlers {
    type Item = &'a TextHandler;
    type IntoIter = std::slice::Iter<'a, TextHandler>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<TextHandler> for TextHandlers {
    fn from_iter<I: IntoIterator<Item = TextHandler>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl fmt::Display for TextHandlers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self
            .0
            .iter()
            .map(|item| format!("\"{item}\""))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "[{inner}]")
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compile a regex pattern, optionally case-insensitive.
fn compile_regex(pattern: &str, case_sensitive: bool) -> Result<Regex> {
    match case_sensitive {
        true => Regex::new(pattern).map_err(Into::into),
        false => Regex::new(&format!("(?i){pattern}")).map_err(Into::into),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_handler_deref() {
        let t = TextHandler::new("hello");
        assert_eq!(t.len(), 5);
        assert!(t.starts_with("hel"));
    }

    #[test]
    fn text_handler_clean() {
        // TextHandler.clean() uses clean_whitespace: \t, \n, \r → space, then collapse + trim
        let t = TextHandler::new("  hello\t\tworld\n\nfoo  ");
        assert_eq!(t.clean(false).as_ref(), "hello world foo");
    }

    #[test]
    fn text_handler_clean_idempotent() {
        let t = TextHandler::new("  a\t\tb\nc  ");
        let once = t.clean(false);
        let twice = once.clean(false);
        assert_eq!(once, twice);
    }

    #[test]
    fn text_handler_re_basic() {
        let t = TextHandler::new("price: $42.99 and $10.50");
        let matches = t.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].as_ref(), "42.99");
        assert_eq!(matches[1].as_ref(), "10.50");
    }

    #[test]
    fn text_handler_re_no_groups() {
        let t = TextHandler::new("abc 123 def 456");
        let matches = t.re(r"\d+", false, false, true).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].as_ref(), "123");
        assert_eq!(matches[1].as_ref(), "456");
    }

    #[test]
    fn text_handler_re_case_insensitive() {
        let t = TextHandler::new("Hello WORLD");
        let matches = t.re(r"hello", false, false, false).unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn text_handler_re_first() {
        let t = TextHandler::new("foo 123 bar 456");
        let first = t.re_first(r"\d+", None, false, false, true).unwrap();
        assert_eq!(first.unwrap().as_ref(), "123");
    }

    #[test]
    fn text_handler_re_first_default() {
        let t = TextHandler::new("no numbers here");
        let default = TextHandler::new("N/A");
        let result = t
            .re_first(r"\d+", Some(default), false, false, true)
            .unwrap();
        assert_eq!(result.unwrap().as_ref(), "N/A");
    }

    #[test]
    fn text_handler_json() {
        let t = TextHandler::new(r#"{"name": "test", "value": 42}"#);
        let v: serde_json::Value = t.json().unwrap();
        assert_eq!(v["name"], "test");
        assert_eq!(v["value"], 42);
    }

    #[test]
    fn text_handler_transforms() {
        let t = TextHandler::new("Hello World");
        assert_eq!(t.to_uppercase_text().as_ref(), "HELLO WORLD");
        assert_eq!(t.to_lowercase_text().as_ref(), "hello world");
        assert_eq!(t.replace_text("World", "Rust").as_ref(), "Hello Rust");
    }

    #[test]
    fn text_handler_split() {
        let t = TextHandler::new("a,b,c");
        let parts = t.split_text(",");
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].as_ref(), "a");
    }

    #[test]
    fn text_handler_sort_chars() {
        let t = TextHandler::new("cba");
        assert_eq!(t.sort_chars(false).as_ref(), "abc");
        assert_eq!(t.sort_chars(true).as_ref(), "cba");
    }

    #[test]
    fn text_handlers_re() {
        let handlers = TextHandlers::new(vec![
            TextHandler::new("foo 1"),
            TextHandler::new("bar 2 baz 3"),
        ]);
        let matches = handlers.re(r"\d+", false, false, true).unwrap();
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn text_handlers_re_first() {
        let handlers = TextHandlers::new(vec![
            TextHandler::new("no match"),
            TextHandler::new("has 42"),
        ]);
        let first = handlers.re_first(r"\d+", None, false, false, true).unwrap();
        assert_eq!(first.unwrap().as_ref(), "42");
    }

    #[test]
    fn text_handlers_get() {
        let handlers =
            TextHandlers::new(vec![TextHandler::new("first"), TextHandler::new("second")]);
        assert_eq!(handlers.get().unwrap().as_ref(), "first");

        let empty = TextHandlers::new(vec![]);
        assert!(empty.get().is_none());
    }
}
