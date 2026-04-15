//! Read-only HTML element attribute map.
//!
//! This module provides [`AttributesHandler`], a read-only mapping from
//! attribute names to [`TextHandler`] values. Because values are `TextHandler`s,
//! callers get regex extraction and whitespace cleaning methods directly on
//! attribute values without an extra conversion step.
//!
//! This is the Rust equivalent of Python scrapling's
//! `AttributesHandler(Mapping)` from `core/custom_types.py`.
//!
//! # Design
//!
//! The map is constructed once from an iterator of `(String, String)` pairs
//! and is **immutable** by API — there are no `insert` or `remove` methods.
//! In Python this is enforced via `MappingProxyType`; in Rust the absence of
//! mutation methods on the public API serves the same purpose, and ownership
//! rules prevent external mutation of the inner `HashMap`.
//!
//! # Examples
//!
//! ```
//! use scrapling::AttributesHandler;
//!
//! let attrs = AttributesHandler::new([
//!     ("class".to_owned(), "price-tag sale".to_owned()),
//!     ("data-price".to_owned(), "42.99".to_owned()),
//! ]);
//!
//! // Direct index access (panics on missing key)
//! assert_eq!(attrs["data-price"].as_ref(), "42.99");
//!
//! // Fallible access
//! assert!(attrs.get("missing").is_none());
//!
//! // Search across values
//! let sale_attrs = attrs.search_values("sale", true);
//! assert_eq!(sale_attrs.len(), 1);
//! ```

use std::collections::HashMap;
use std::fmt;
use std::ops::Index;

use serde::{Deserialize, Serialize};

use crate::text::TextHandler;

// ---------------------------------------------------------------------------
// AttributesHandler
// ---------------------------------------------------------------------------

/// A read-only mapping of HTML element attributes to [`TextHandler`] values.
///
/// See the [module-level documentation](self) for an overview.
///
/// # Serialization
///
/// Serializes as a JSON object `{"key": "value", ...}` via `serde`.
///
/// # Indexing
///
/// Supports `attrs["key"]` via [`Index<&str>`]. This **panics** if the key
/// is absent — use [`get()`](AttributesHandler::get) for fallible access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributesHandler {
    data: HashMap<String, TextHandler>,
}

impl AttributesHandler {
    /// Build from any iterator of `(key, value)` string pairs.
    ///
    /// Values are converted to [`TextHandler`] automatically.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::AttributesHandler;
    /// let attrs = AttributesHandler::new([
    ///     ("href".to_owned(), "/about".to_owned()),
    /// ]);
    /// assert_eq!(attrs["href"].as_ref(), "/about");
    /// ```
    pub fn new(attrs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            data: attrs
                .into_iter()
                .map(|(k, v)| (k, TextHandler::new(v)))
                .collect(),
        }
    }

    /// Build from a pre-existing `HashMap<String, String>`.
    ///
    /// Equivalent to [`new()`](AttributesHandler::new) but avoids the need
    /// for an explicit `.into_iter()` call.
    pub fn from_map(map: HashMap<String, String>) -> Self {
        Self::new(map)
    }

    /// Create an empty attribute set.
    pub fn empty() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Look up a value by key, returning `None` if the key is absent.
    ///
    /// Prefer this over indexing (`attrs["key"]`) when the key may not exist.
    pub fn get(&self, key: &str) -> Option<&TextHandler> {
        self.data.get(key)
    }

    /// Return `true` if the given attribute name is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Return the number of attributes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Return `true` if there are no attributes.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Iterate over `(key, value)` pairs as `(&str, &TextHandler)`.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &TextHandler)> {
        self.data.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterate over attribute names.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.data.keys().map(|k| k.as_str())
    }

    /// Iterate over attribute values.
    pub fn values(&self) -> impl Iterator<Item = &TextHandler> {
        self.data.values()
    }

    /// Search attribute values for a keyword.
    ///
    /// Returns a `Vec` of single-entry `AttributesHandler` instances, one for
    /// each attribute whose value matches.
    ///
    /// # Parameters
    ///
    /// - `keyword` — the string to search for.
    /// - `partial` — if `true`, matches when the keyword is **contained in**
    ///   the value. If `false`, requires an **exact** match.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::AttributesHandler;
    /// let attrs = AttributesHandler::new([
    ///     ("class".to_owned(), "main-content".to_owned()),
    ///     ("id".to_owned(), "article".to_owned()),
    /// ]);
    ///
    /// // Exact match
    /// let exact = attrs.search_values("article", false);
    /// assert_eq!(exact.len(), 1);
    /// assert!(exact[0].contains_key("id"));
    ///
    /// // Partial match
    /// let partial = attrs.search_values("main", true);
    /// assert_eq!(partial.len(), 1);
    /// assert!(partial[0].contains_key("class"));
    /// ```
    pub fn search_values(&self, keyword: &str, partial: bool) -> Vec<AttributesHandler> {
        self.data
            .iter()
            .filter(|(_, v)| match partial {
                true => v.contains(keyword),
                false => v.as_ref() == keyword,
            })
            .map(|(k, v)| AttributesHandler::new(std::iter::once((k.clone(), v.to_string()))))
            .collect()
    }

    /// Serialize the attributes as a JSON string.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if serialization fails (unlikely for
    /// string-valued maps).
    pub fn json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.data)
    }

    /// Serialize the attributes as a [`serde_json::Value`].
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if serialization fails.
    pub fn json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(&self.data)
    }

    /// Consume `self` and return the inner `HashMap<String, TextHandler>`.
    pub fn into_inner(self) -> HashMap<String, TextHandler> {
        self.data
    }
}

impl Index<&str> for AttributesHandler {
    type Output = TextHandler;

    /// Index into the attribute map by key.
    ///
    /// # Panics
    ///
    /// Panics if the key is not present. Use [`get()`](AttributesHandler::get)
    /// for fallible access.
    fn index(&self, key: &str) -> &TextHandler {
        &self.data[key]
    }
}

impl<'a> IntoIterator for &'a AttributesHandler {
    type Item = (&'a String, &'a TextHandler);
    type IntoIter = std::collections::hash_map::Iter<'a, String, TextHandler>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

impl IntoIterator for AttributesHandler {
    type Item = (String, TextHandler);
    type IntoIter = std::collections::hash_map::IntoIter<String, TextHandler>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl fmt::Display for AttributesHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        let mut first = true;
        for (k, v) in &self.data {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "\"{k}\": \"{v}\"")?;
            first = false;
        }
        write!(f, "}}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AttributesHandler {
        AttributesHandler::new([
            ("class".to_owned(), "main-content".to_owned()),
            ("id".to_owned(), "article".to_owned()),
            ("data-page".to_owned(), "1".to_owned()),
        ])
    }

    #[test]
    fn basic_access() {
        let attrs = sample();
        assert_eq!(attrs.get("id").unwrap().as_ref(), "article");
        assert_eq!(attrs["class"].as_ref(), "main-content");
        assert!(attrs.get("missing").is_none());
    }

    #[test]
    fn len_and_contains() {
        let attrs = sample();
        assert_eq!(attrs.len(), 3);
        assert!(attrs.contains_key("class"));
        assert!(!attrs.contains_key("href"));
    }

    #[test]
    fn search_values_exact() {
        let attrs = sample();
        let results = attrs.search_values("article", false);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("id"));
    }

    #[test]
    fn search_values_partial() {
        let attrs = sample();
        let results = attrs.search_values("main", true);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("class"));
    }

    #[test]
    fn json_roundtrip() {
        let attrs = sample();
        let json_str = attrs.json_string().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(value["id"], "article");
    }

    #[test]
    fn empty() {
        let attrs = AttributesHandler::empty();
        assert!(attrs.is_empty());
        assert_eq!(attrs.len(), 0);
    }

    #[test]
    fn iteration() {
        let attrs = sample();
        let keys: Vec<&str> = attrs.keys().collect();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn display() {
        let attrs = AttributesHandler::new([("id".to_owned(), "test".to_owned())]);
        let s = attrs.to_string();
        assert!(s.contains("\"id\": \"test\""));
    }
}
