//! Persistent element storage for adaptive selection.
//!
//! This module defines the [`StorageSystem`] trait and the [`ElementData`]
//! struct used by the adaptive relocation engine. When a CSS/XPath selector
//! matches an element, its structural properties can be saved to storage.
//! If the page later changes and the same selector fails, the stored
//! properties are used to relocate the element by structural similarity.
//!
//! # Architecture
//!
//! ```text
//! Selector::css("div.price", adaptive=true, auto_save=true)
//!    │
//!    ├─ match found → save ElementData to storage (keyed by identifier + URL)
//!    │
//!    └─ match failed → retrieve stored ElementData → relocate() via similarity scoring
//! ```
//!
//! # Storage backends
//!
//! The [`StorageSystem`] trait is backend-agnostic. The [`SqliteStorage`]
//! implementation (behind the `storage` Cargo feature) provides a
//! thread-safe, WAL-mode SQLite backend that mirrors Python scrapling's
//! `SQLiteStorageSystem`.

#[cfg(feature = "storage")]
pub mod sqlite;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::selector::Selector;

// ---------------------------------------------------------------------------
// ElementData — the structural fingerprint of an element
// ---------------------------------------------------------------------------

/// The structural fingerprint of an HTML element, used for similarity
/// comparison during adaptive relocation.
///
/// This is the Rust equivalent of the dictionary produced by Python's
/// `_StorageTools.element_to_dict()`. Every field captures a dimension
/// of the element's identity within the DOM — tag name, attributes, text,
/// ancestor path, parent info, and sibling/child structure.
///
/// All fields are `String`-based for serialization simplicity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementData {
    /// The element's tag name (e.g. `"div"`, `"a"`).
    pub tag: String,
    /// The element's attributes with whitespace-stripped values.
    /// Empty attributes and attributes with empty values are excluded.
    pub attributes: HashMap<String, String>,
    /// The element's direct text content, or `None` if empty.
    pub text: Option<String>,
    /// The path from root to this element as a list of tag names.
    /// E.g. `["html", "body", "div", "ul", "li"]`.
    pub path: Vec<String>,
    /// The parent element's tag name.
    pub parent_name: Option<String>,
    /// The parent element's attributes.
    pub parent_attribs: Option<HashMap<String, String>>,
    /// The parent element's direct text content.
    pub parent_text: Option<String>,
    /// Tag names of sibling elements (other children of the parent).
    pub siblings: Vec<String>,
    /// Tag names of direct child elements.
    pub children: Vec<String>,
}

impl ElementData {
    /// Extract structural data from a [`Selector`].
    ///
    /// This is the Rust equivalent of `_StorageTools.element_to_dict()`.
    pub fn from_selector(sel: &Selector) -> Self {
        let tag = sel.tag().to_owned();
        let attributes: HashMap<String, String> = sel
            .attrib()
            .iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
            .collect();

        let text = trimmed_text_or_none(&sel.text());

        let path: Vec<String> = {
            let mut p: Vec<String> = sel.ancestors().iter().map(|a| a.tag().to_owned()).collect();
            p.reverse();
            p.push(tag.clone());
            p
        };

        let (parent_name, parent_attribs, parent_text) = match sel.parent() {
            Some(parent) => {
                let pattribs: HashMap<String, String> = parent
                    .attrib()
                    .iter()
                    .map(|(k, v)| (k.to_owned(), v.to_string()))
                    .collect();
                (
                    Some(parent.tag().to_owned()),
                    Some(pattribs),
                    trimmed_text_or_none(&parent.text()),
                )
            }
            None => (None, None, None),
        };

        let siblings: Vec<String> = sel.siblings().iter().map(|s| s.tag().to_owned()).collect();
        let children: Vec<String> = sel.children().iter().map(|c| c.tag().to_owned()).collect();

        Self {
            tag,
            attributes,
            text,
            path,
            parent_name,
            parent_attribs,
            parent_text,
            siblings,
            children,
        }
    }
}

/// Return `Some(trimmed)` if the text is non-empty after trimming, else `None`.
fn trimmed_text_or_none(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

// ---------------------------------------------------------------------------
// StorageSystem trait
// ---------------------------------------------------------------------------

/// Trait for persistent element storage backends.
///
/// Implementations must be able to save and retrieve [`ElementData`] keyed
/// by `(url, identifier)` pairs. The URL scopes data per website; the
/// identifier is typically the CSS/XPath selector string.
pub trait StorageSystem {
    /// Save an element's structural data under the given identifier.
    ///
    /// If an entry with the same `(url, identifier)` already exists, it
    /// should be replaced (upsert semantics).
    fn save(&self, data: &ElementData, identifier: &str) -> crate::Result<()>;

    /// Retrieve previously saved element data by identifier.
    ///
    /// Returns `None` if no data exists for this identifier + URL.
    fn retrieve(&self, identifier: &str) -> crate::Result<Option<ElementData>>;
}
