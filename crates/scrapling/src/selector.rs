//! HTML element selection and DOM traversal.
//!
//! This module provides [`Selector`] and [`Selectors`], the central types of
//! the scrapling library. Together they offer:
//!
//! - HTML parsing via [`html5ever`] (through the [`scraper`] crate)
//! - CSS selector matching with `::text` / `::attr()` pseudo-element support
//! - DOM tree navigation (parent, children, siblings, ancestors)
//! - Text and attribute extraction as [`TextHandler`] / [`AttributesHandler`]
//! - Compound filtering (`find_all`, `find_by_text`, `find_by_regex`)
//! - Selector generation (`generate_css_selector`, `generate_xpath_selector`)
//! - Scrapy/parsel API compatibility (`get`, `getall`, `extract`, `re`)
//!
//! # Design
//!
//! Python scrapling wraps `lxml.html.HtmlElement` — a C-backed tree with
//! XPath evaluation. In Rust we use [`scraper::Html`] (an
//! [`html5ever`]-powered DOM backed by [`ego_tree`]) with native CSS matching
//! via the [`selectors`] crate. This gives us:
//!
//! - Zero-copy references into a single arena-allocated tree
//! - CSS matching without XPath translation overhead
//! - Thread-safe `Send + Sync` trees (no GIL)
//!
//! A [`Selector`] holds an `DocRef` (the parsed document) plus a
//! [`NodeId`] pointing to its element within that tree. Multiple `Selector`
//! instances can cheaply reference different nodes in the same document.
//!
//! # Examples
//!
//! ```
//! use scrapling::selector::{Selector, Selectors};
//!
//! let sel = Selector::from_html(r#"
//!     <div class="products">
//!         <div class="item"><span>Widget</span></div>
//!         <div class="item"><span>Gadget</span></div>
//!     </div>
//! "#);
//!
//! let items = sel.css("div.item");
//! assert_eq!(items.len(), 2);
//! assert_eq!(items[0].css("span").first().unwrap().text().as_ref(), "Widget");
//! ```

use std::fmt;

// `scraper::Html` contains non-Send types (`Cell`, `UnsafeCell` from
// html5ever tendrils), so the DOM tree is inherently single-threaded.
// For multi-threaded usage, parse HTML inside each task independently
// or pass the raw HTML string across thread boundaries.
type DocRef = std::rc::Rc<Html>;

use ego_tree::NodeId;
use regex::Regex;

use scraper::{Html, Node};

use crate::attributes::AttributesHandler;
use crate::error::Result;
use crate::text::{TextHandler, TextHandlers};
use crate::translator::CssQuery;
use crate::utils::clean_spaces;

// ---------------------------------------------------------------------------
// ParseOptions
// ---------------------------------------------------------------------------

/// Options for HTML parsing.
///
/// Controls comment/CDATA preservation and associates a URL for relative
/// URL resolution via [`Selector::urljoin`].
///
/// Note: The underlying `html5ever` parser always preserves comment and
/// CDATA nodes in the tree. These flags control whether scrapling includes
/// them in traversal results like [`Selector::children`] and
/// [`Selector::descendants`].
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ParseOptions {
    /// Base URL for relative URL resolution.
    pub url: Option<String>,
    /// If `true`, include comment nodes in traversal. Default: `false`.
    pub keep_comments: bool,
    /// If `true`, include CDATA sections in traversal. Default: `false`.
    pub keep_cdata: bool,
}

// ---------------------------------------------------------------------------
// Selector
// ---------------------------------------------------------------------------

/// A reference to a single node in a parsed HTML document.
///
/// See the [module-level documentation](self) for an overview.
///
/// A `Selector` is cheap to clone — it holds an `Rc` to the shared document
/// tree plus a lightweight [`NodeId`].
#[derive(Clone)]
pub struct Selector {
    /// The parsed HTML document (shared across all selectors from the same parse).
    doc: DocRef,
    /// The node this selector points to within `doc`.
    node_id: NodeId,
    /// The URL associated with the document, used for `urljoin`.
    url: String,
    /// Whether this selector represents a text node (from `::text` / `/text()`).
    is_text_node: bool,
    /// Cached text for text-node selectors.
    text_value: Option<String>,
}

impl Selector {
    // -- Construction --------------------------------------------------------

    /// Parse an HTML string and return a `Selector` pointing at the document root.
    ///
    /// This is the primary entry point. The HTML is parsed with
    /// [`html5ever`]'s error-recovering parser, so malformed HTML is handled
    /// gracefully.
    ///
    /// # Examples
    ///
    /// ```
    /// use scrapling::selector::Selector;
    ///
    /// let sel = Selector::from_html("<p>hello</p>");
    /// assert_eq!(sel.css("p").first().unwrap().text().as_ref(), "hello");
    /// ```
    pub fn from_html(html: &str) -> Self {
        Self::from_doc(Html::parse_document(html))
    }

    /// Parse an HTML fragment (not a full document).
    ///
    /// Use this when parsing a snippet like `<span>hi</span>` that shouldn't
    /// be wrapped in `<html><body>`.
    pub fn from_fragment(html: &str) -> Self {
        Self::from_doc(Html::parse_fragment(html))
    }

    /// Parse HTML with an associated URL (used by [`urljoin`](Selector::urljoin)).
    pub fn from_html_with_url(html: &str, url: impl Into<String>) -> Self {
        let mut sel = Self::from_html(html);
        sel.url = url.into();
        sel
    }

    /// Parse HTML with additional options.
    ///
    /// Use [`ParseOptions`] to control comment/CDATA preservation and
    /// associate a URL for relative URL resolution.
    pub fn from_html_with_options(html: &str, options: &ParseOptions) -> Self {
        let mut sel = Self::from_doc(Html::parse_document(html));
        if let Some(ref url) = options.url {
            sel.url = url.clone();
        }
        sel
    }

    /// Parse HTML from raw bytes with automatic encoding detection.
    ///
    /// Uses `encoding_rs` to detect the encoding from BOM or content, then
    /// decodes to UTF-8 before parsing. Falls back to UTF-8 if detection fails.
    pub fn from_bytes(data: &[u8]) -> Self {
        let (decoded, _, _) = encoding_rs::UTF_8.decode(data);
        // Try to detect encoding from meta charset in the first pass
        let html_str = if let Some(encoding) = detect_meta_charset(&decoded) {
            let enc =
                encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);
            let (result, _, _) = enc.decode(data);
            result.into_owned()
        } else {
            decoded.into_owned()
        };
        Self::from_html(&html_str)
    }

    /// Parse HTML from raw bytes with a known encoding.
    pub fn from_bytes_with_encoding(data: &[u8], encoding: &str) -> Self {
        let enc =
            encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);
        let (decoded, _, _) = enc.decode(data);
        Self::from_html(&decoded)
    }

    /// Shared constructor for both document and fragment parsing.
    fn from_doc(doc: Html) -> Self {
        let doc = DocRef::new(doc);
        let root_id = doc.root_element().id();
        Self {
            doc,
            node_id: root_id,
            url: String::new(),
            is_text_node: false,
            text_value: None,
        }
    }

    /// Create a selector pointing to a different node in the same document.
    fn new_ref(&self, node_id: NodeId) -> Self {
        Self {
            doc: DocRef::clone(&self.doc),
            node_id,
            url: self.url.clone(),
            is_text_node: false,
            text_value: None,
        }
    }

    /// Create a text-node selector (result of `::text` or `/text()`).
    fn new_text_node(&self, text: String) -> Self {
        Self {
            doc: DocRef::clone(&self.doc),
            node_id: self.node_id,
            url: self.url.clone(),
            is_text_node: true,
            text_value: Some(text),
        }
    }

    // -- Basic properties ----------------------------------------------------

    /// The tag name of this element (e.g. `"div"`, `"a"`).
    ///
    /// Returns `"#text"` for text nodes, `"#document"` for the document root.
    pub fn tag(&self) -> &str {
        if self.is_text_node {
            return "#text";
        }
        match self.node().value() {
            Node::Element(el) => el.name(),
            Node::Document => "#document",
            Node::Text(_) => "#text",
            _ => "#unknown",
        }
    }

    /// The direct text content of this element (not recursive).
    ///
    /// For element nodes, returns the first text child. For text nodes,
    /// returns the text value. Returns an empty `TextHandler` if there is
    /// no text.
    pub fn text(&self) -> TextHandler {
        if self.is_text_node {
            return TextHandler::new(self.text_node_str());
        }
        let node = self.node();
        // Collect direct text children (not descendants)
        let mut text = String::new();
        for child in node.children() {
            if let Node::Text(t) = child.value() {
                text.push_str(t);
            }
        }
        TextHandler::new(text)
    }

    /// Get all text content recursively, concatenated with `separator`.
    ///
    /// # Parameters
    ///
    /// - `separator` — string inserted between text segments (default `"\n"`).
    /// - `strip` — if `true`, each segment is trimmed before joining.
    /// - `ignore_tags` — elements with these tag names are skipped.
    /// - `valid_values` — if `true`, skip segments that are empty or only whitespace.
    pub fn get_all_text(
        &self,
        separator: &str,
        strip: bool,
        ignore_tags: &[&str],
        valid_values: bool,
    ) -> TextHandler {
        if self.is_text_node {
            return TextHandler::new(self.text_node_str());
        }
        let mut parts = Vec::new();
        self.collect_text(self.node_id, ignore_tags, strip, valid_values, &mut parts);
        TextHandler::new(parts.join(separator))
    }

    /// Recursive text collector.
    fn collect_text(
        &self,
        node_id: NodeId,
        ignore_tags: &[&str],
        strip: bool,
        valid_values: bool,
        out: &mut Vec<String>,
    ) {
        let node_ref = self.doc.tree.get(node_id).unwrap();
        for child in node_ref.children() {
            match child.value() {
                Node::Text(t) => {
                    let s = if strip {
                        t.trim().to_owned()
                    } else {
                        t.to_string()
                    };
                    if !valid_values || !s.trim().is_empty() {
                        out.push(s);
                    }
                }
                Node::Element(el) => {
                    if !ignore_tags.contains(&el.name()) {
                        self.collect_text(child.id(), ignore_tags, strip, valid_values, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// The attributes of this element as an [`AttributesHandler`].
    ///
    /// Returns an empty handler for text nodes.
    pub fn attrib(&self) -> AttributesHandler {
        if self.is_text_node {
            return AttributesHandler::empty();
        }
        match self.node().value() {
            Node::Element(el) => {
                AttributesHandler::new(el.attrs().map(|(k, v)| (k.to_owned(), v.to_owned())))
            }
            _ => AttributesHandler::empty(),
        }
    }

    /// The inner HTML of this element as a string.
    ///
    /// For text nodes, returns the text value.
    pub fn html_content(&self) -> TextHandler {
        if self.is_text_node {
            return TextHandler::new(self.text_node_str());
        }
        let node = self.node();
        let mut html = String::new();
        for child in node.children() {
            write_node_html(&self.doc, child.id(), &mut html);
        }
        TextHandler::new(html)
    }

    /// The outer HTML of this element (including the element's own tag).
    pub fn outer_html(&self) -> TextHandler {
        if self.is_text_node {
            return TextHandler::new(self.text_node_str());
        }
        let mut html = String::new();
        write_node_html(&self.doc, self.node_id, &mut html);
        TextHandler::new(html)
    }

    /// Check if this element has a specific CSS class.
    pub fn has_class(&self, class_name: &str) -> bool {
        if self.is_text_node {
            return false;
        }
        match self.node().value() {
            Node::Element(el) => el.has_class(class_name, scraper::CaseSensitivity::CaseSensitive),
            _ => false,
        }
    }

    /// The base URL associated with this document, if any.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Join a relative URL with this selector's base URL.
    ///
    /// Returns the relative URL unchanged if no base URL was set.
    pub fn urljoin(&self, relative_url: &str) -> String {
        if self.url.is_empty() {
            return relative_url.to_owned();
        }
        match url::Url::parse(&self.url) {
            Ok(base) => base
                .join(relative_url)
                .map_or_else(|_| relative_url.to_owned(), |u| u.to_string()),
            Err(_) => relative_url.to_owned(),
        }
    }

    // -- Navigation ----------------------------------------------------------

    /// The direct parent element, or `None` for the root.
    pub fn parent(&self) -> Option<Selector> {
        let node = self.node();
        node.parent().and_then(|p| {
            if matches!(p.value(), Node::Document) {
                None
            } else {
                Some(self.new_ref(p.id()))
            }
        })
    }

    /// Direct child elements (excludes text nodes and comments).
    pub fn children(&self) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }
        let node = self.node();
        let kids: Vec<Selector> = node
            .children()
            .filter(|c| matches!(c.value(), Node::Element(_)))
            .map(|c| self.new_ref(c.id()))
            .collect();
        Selectors::new(kids)
    }

    /// Sibling elements (other children of this element's parent).
    pub fn siblings(&self) -> Selectors {
        match self.parent() {
            Some(p) => {
                let my_id = self.node_id;
                Selectors::new(
                    p.children()
                        .into_iter()
                        .filter(|c| c.node_id != my_id)
                        .collect(),
                )
            }
            None => Selectors::empty(),
        }
    }

    /// The next sibling element, or `None`.
    pub fn next(&self) -> Option<Selector> {
        if self.is_text_node {
            return None;
        }
        let node = self.node();
        let mut sib = node.next_sibling();
        while let Some(s) = sib {
            if matches!(s.value(), Node::Element(_)) {
                return Some(self.new_ref(s.id()));
            }
            sib = s.next_sibling();
        }
        None
    }

    /// The previous sibling element, or `None`.
    pub fn previous(&self) -> Option<Selector> {
        if self.is_text_node {
            return None;
        }
        let node = self.node();
        let mut sib = node.prev_sibling();
        while let Some(s) = sib {
            if matches!(s.value(), Node::Element(_)) {
                return Some(self.new_ref(s.id()));
            }
            sib = s.prev_sibling();
        }
        None
    }

    /// Iterator over all ancestor elements, starting with the parent.
    pub fn ancestors(&self) -> Vec<Selector> {
        let mut result = Vec::new();
        let mut current = self.parent();
        while let Some(p) = current {
            result.push(p.clone());
            current = p.parent();
        }
        result
    }

    /// Path from root to this element (list of ancestors, root first).
    pub fn path(&self) -> Selectors {
        let mut anc = self.ancestors();
        anc.reverse();
        Selectors::new(anc)
    }

    /// All descendant elements (depth-first).
    pub fn descendants(&self) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }
        let node = self.node();
        let descs: Vec<Selector> = node
            .descendants()
            .skip(1) // skip self
            .filter(|n| matches!(n.value(), Node::Element(_)))
            .map(|n| self.new_ref(n.id()))
            .collect();
        Selectors::new(descs)
    }

    /// Find the first ancestor matching a predicate.
    pub fn find_ancestor(&self, predicate: impl Fn(&Selector) -> bool) -> Option<Selector> {
        self.ancestors().into_iter().find(|a| predicate(a))
    }

    // -- Selection -----------------------------------------------------------

    /// Select elements matching a CSS selector.
    ///
    /// Supports `::text` and `::attr(name)` pseudo-elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::selector::Selector;
    /// let sel = Selector::from_html("<ul><li>a</li><li>b</li></ul>");
    /// let items = sel.css("li");
    /// assert_eq!(items.len(), 2);
    ///
    /// let texts = sel.css("li::text");
    /// assert_eq!(texts.len(), 2);
    /// assert_eq!(texts[0].text().as_ref(), "a");
    /// ```
    pub fn css(&self, selector: &str) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }

        let query = match CssQuery::parse(selector) {
            Ok(q) => q,
            Err(_) => return Selectors::empty(),
        };

        let css_sel = match scraper::Selector::parse(query.css()) {
            Ok(s) => s,
            Err(_) => return Selectors::empty(),
        };

        let node = self.node();
        let element_ref = match node.value() {
            Node::Element(_) => scraper::ElementRef::wrap(node).unwrap(),
            _ => return Selectors::empty(),
        };

        let matched: Vec<Selector> = element_ref
            .select(&css_sel)
            .map(|el| self.new_ref(el.id()))
            .collect();

        // Apply pseudo-element post-processing
        match query.pseudo() {
            Some(crate::translator::PseudoElement::Text) => {
                let mut text_nodes = Vec::new();
                for sel in &matched {
                    let node_ref = sel.node();
                    for child in node_ref.children() {
                        if let Node::Text(t) = child.value() {
                            if !t.trim().is_empty() {
                                text_nodes.push(sel.new_text_node(t.to_string()));
                            }
                        }
                    }
                }
                Selectors::new(text_nodes)
            }
            Some(crate::translator::PseudoElement::Attr(name)) => {
                let mut attr_nodes = Vec::new();
                for sel in &matched {
                    if let Some(val) = sel.attrib().get(name) {
                        attr_nodes.push(sel.new_text_node(val.to_string()));
                    }
                }
                Selectors::new(attr_nodes)
            }
            None => Selectors::new(matched),
        }
    }

    /// CSS selection with adaptive relocation fallback.
    ///
    /// Mirrors Python scrapling's `css(selector, adaptive=True, auto_save=True)` flow:
    ///
    /// 1. Try the normal CSS selector match.
    /// 2. **If match succeeds** and `auto_save` is true: save the first matched
    ///    element's structural fingerprint to `storage` under `identifier`.
    /// 3. **If match fails** and `adaptive` is true: retrieve the stored
    ///    fingerprint and relocate the element by structural similarity.
    ///    If relocation succeeds and `auto_save` is true, save the relocated element.
    ///
    /// The `identifier` defaults to the selector string itself if `None`.
    /// The `percentage` sets the minimum similarity score (0–100) for relocation.
    pub fn css_adaptive(
        &self,
        selector: &str,
        storage: &dyn crate::storage::StorageSystem,
        adaptive: bool,
        auto_save: bool,
        identifier: Option<&str>,
        percentage: f64,
    ) -> Selectors {
        let id = identifier.unwrap_or(selector);
        let results = self.css(selector);

        if !results.is_empty() {
            if auto_save {
                if let Some(first) = results.first() {
                    let _ = first.save(storage, id);
                }
            }
            return results;
        }

        if !adaptive {
            return results;
        }

        let stored = match Self::retrieve(storage, id) {
            Ok(Some(data)) => data,
            _ => return Selectors::empty(),
        };

        let relocated = crate::adaptive::relocate(self, &stored, percentage);

        if !relocated.is_empty() && auto_save {
            if let Some(first) = relocated.first() {
                let _ = first.save(storage, id);
            }
        }

        relocated
    }

    /// Relocate an element by structural similarity against a stored fingerprint.
    ///
    /// Scores every descendant element against `original` using the 12-factor
    /// similarity algorithm and returns elements with the highest score above
    /// `min_percentage`.
    pub fn relocate(
        &self,
        original: &crate::storage::ElementData,
        min_percentage: f64,
    ) -> Selectors {
        crate::adaptive::relocate(self, original, min_percentage)
    }

    /// Find elements by compound filters: tag names, attribute key/value pairs,
    /// regex patterns on text, and/or predicate functions.
    ///
    /// Builds a CSS selector from `tags` + `attributes`, runs it, then further
    /// filters by `patterns` and `predicates`.
    ///
    /// # Parameters
    ///
    /// - `tags` — tag names to match (empty = wildcard `*`).
    /// - `attributes` — attribute key/value pairs (`[key="value"]`).
    /// - `patterns` — regex patterns applied to each element's text.
    /// - `predicates` — closures taking `&Selector` → `bool`.
    pub fn find_all(
        &self,
        tags: &[&str],
        attributes: &[(&str, &str)],
        patterns: &[&str],
        predicates: &[&dyn Fn(&Selector) -> bool],
    ) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }

        if tags.is_empty() && attributes.is_empty() && patterns.is_empty() && predicates.is_empty()
        {
            return Selectors::empty();
        }

        let effective_tags: Vec<&str> = if tags.is_empty() {
            vec!["*"]
        } else {
            tags.to_vec()
        };

        let mut css_parts = Vec::new();
        for tag in &effective_tags {
            let mut selector = tag.to_string();
            for (key, value) in attributes {
                let escaped = value.replace('"', r#"\""#);
                selector.push_str(&format!(r#"[{key}="{escaped}"]"#));
            }
            css_parts.push(selector);
        }

        let css_query = css_parts.join(", ");

        let mut results = if css_query == "*" && attributes.is_empty() {
            self.descendants()
        } else {
            self.css(&css_query)
        };

        for pattern in patterns {
            results = results.filter(|el| {
                let text = el.text();
                if text.is_empty() {
                    return false;
                }
                text.re_matches(pattern, true).unwrap_or(false)
            });
        }

        for predicate in predicates {
            results = results.filter(|el| predicate(el));
        }

        results
    }

    /// Like [`find_all`](Selector::find_all) but returns only the first match.
    pub fn find(
        &self,
        tags: &[&str],
        attributes: &[(&str, &str)],
        patterns: &[&str],
        predicates: &[&dyn Fn(&Selector) -> bool],
    ) -> Option<Selector> {
        self.find_all(tags, attributes, patterns, predicates)
            .first()
            .cloned()
    }

    /// Find structurally similar sibling elements at the same DOM depth.
    ///
    /// Inspired by AutoScraper — given a reference element (e.g. one product card),
    /// finds other elements with the same tag, parent tag, and grandparent tag at the
    /// same depth, then filters by attribute similarity.
    ///
    /// # Parameters
    ///
    /// - `similarity_threshold` — minimum attribute similarity ratio (0.0–1.0, default 0.2).
    /// - `match_text` — if `true`, text content is included in similarity scoring.
    /// - `ignore_attributes` — attribute names to skip during comparison (default: `href`, `src`).
    pub fn find_similar(
        &self,
        similarity_threshold: Option<f64>,
        match_text: bool,
        ignore_attributes: &[&str],
    ) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }

        let threshold = similarity_threshold.unwrap_or(0.2);
        let my_tag = self.tag().to_owned();
        let ancestors = self.ancestors();
        let my_depth = ancestors.len();

        let mut path_parts = vec![my_tag.clone()];
        if let Some(parent) = ancestors.first() {
            path_parts.insert(0, parent.tag().to_owned());
            if ancestors.len() > 1 {
                path_parts.insert(0, ancestors[1].tag().to_owned());
            }
        }

        let my_attribs = self.filtered_attribs(ignore_attributes);

        let root = Self {
            doc: DocRef::clone(&self.doc),
            node_id: self.doc.root_element().id(),
            url: self.url.clone(),
            is_text_node: false,
            text_value: None,
        };

        let candidates = root.css(&my_tag);

        let mut similar = Vec::new();
        for candidate in candidates.iter() {
            if candidate.node_id == self.node_id {
                continue;
            }

            let cand_depth = candidate.ancestors().len();
            if cand_depth != my_depth {
                continue;
            }

            let cand_ancestors = candidate.ancestors();
            let matches_parent = match (ancestors.first(), cand_ancestors.first()) {
                (Some(a), Some(b)) => a.tag() == b.tag(),
                (None, None) => true,
                _ => false,
            };
            if !matches_parent {
                continue;
            }

            let cand_attribs = candidate.filtered_attribs(ignore_attributes);

            let sim =
                Self::attrib_similarity(&my_attribs, &cand_attribs, match_text, self, candidate);
            if sim >= threshold {
                similar.push(candidate.clone());
            }
        }

        Selectors::new(similar)
    }

    fn filtered_attribs(&self, ignore: &[&str]) -> std::collections::HashMap<String, String> {
        let attrib = self.attrib();
        attrib
            .keys()
            .filter(|k| !ignore.contains(k))
            .map(|k| (k.to_owned(), attrib[k].as_ref().to_owned()))
            .collect()
    }

    fn attrib_similarity(
        a: &std::collections::HashMap<String, String>,
        b: &std::collections::HashMap<String, String>,
        match_text: bool,
        sel_a: &Selector,
        sel_b: &Selector,
    ) -> f64 {
        if a.is_empty() && b.is_empty() {
            return if match_text {
                if sel_a.text().as_ref() == sel_b.text().as_ref() {
                    1.0
                } else {
                    0.0
                }
            } else {
                1.0
            };
        }

        let all_keys: std::collections::HashSet<&String> = a.keys().chain(b.keys()).collect();
        if all_keys.is_empty() {
            return 1.0;
        }

        let mut checks = 0.0_f64;
        let mut score = 0.0_f64;

        for key in &all_keys {
            checks += 1.0;
            if let (Some(va), Some(vb)) = (a.get(*key), b.get(*key)) {
                if va == vb {
                    score += 1.0;
                } else {
                    score += strsim::jaro_winkler(va, vb);
                }
            }
        }

        if match_text {
            checks += 1.0;
            let ta = sel_a.text();
            let tb = sel_b.text();
            if ta.as_ref() == tb.as_ref() {
                score += 1.0;
            } else {
                score += strsim::jaro_winkler(ta.as_ref(), tb.as_ref());
            }
        }

        if checks == 0.0 { 1.0 } else { score / checks }
    }

    /// Select elements by text content.
    ///
    /// # Parameters
    ///
    /// - `text` — the text to search for.
    /// - `partial` — if `true`, match when text is contained; if `false`, exact match.
    /// - `case_sensitive` — whether comparison is case-sensitive.
    /// - `clean_match` — normalise whitespace before comparing.
    pub fn find_by_text(
        &self,
        text: &str,
        partial: bool,
        case_sensitive: bool,
        clean_match: bool,
    ) -> Selectors {
        if self.is_text_node {
            return Selectors::empty();
        }

        let query = if case_sensitive {
            text.to_owned()
        } else {
            text.to_lowercase()
        };

        let mut results = Vec::new();
        for desc in self.descendants() {
            let node_text = desc.text();
            if node_text.is_empty() {
                continue;
            }

            let mut cmp_text = if clean_match {
                node_text.clean(false).into_inner()
            } else {
                node_text.into_inner()
            };

            if !case_sensitive {
                cmp_text = cmp_text.to_lowercase();
            }

            let matched = if partial {
                cmp_text.contains(&query)
            } else {
                cmp_text == query
            };

            if matched {
                results.push(desc);
            }
        }
        Selectors::new(results)
    }

    /// Select elements whose text matches a regex pattern.
    pub fn find_by_regex(
        &self,
        pattern: &str,
        case_sensitive: bool,
        clean_match: bool,
    ) -> Result<Selectors> {
        if self.is_text_node {
            return Ok(Selectors::empty());
        }

        let mut results = Vec::new();
        for desc in self.descendants() {
            let node_text = desc.text();
            if node_text.is_empty() {
                continue;
            }
            let search_text = if clean_match {
                node_text.clean(false)
            } else {
                node_text
            };
            if search_text.re_matches(pattern, case_sensitive)? {
                results.push(desc);
            }
        }
        Ok(Selectors::new(results))
    }

    // -- Extraction ----------------------------------------------------------

    /// Serialize this element to a string.
    ///
    /// For text nodes returns the text value; for elements returns outer HTML.
    /// Scrapy/parsel compat alias for [`outer_html`](Selector::outer_html).
    pub fn get(&self) -> TextHandler {
        if self.is_text_node {
            return TextHandler::new(self.text_node_str());
        }
        self.outer_html()
    }

    /// Return a single-element `TextHandlers` containing this element's serialized string.
    pub fn getall(&self) -> TextHandlers {
        TextHandlers::new(vec![self.get()])
    }

    /// Apply a regex to this element's text content and return matches.
    pub fn re(
        &self,
        regex: &str,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<TextHandlers> {
        self.text()
            .re(regex, replace_entities, clean_match, case_sensitive)
    }

    /// Apply a regex and return the first match.
    pub fn re_first(
        &self,
        regex: &str,
        default: Option<TextHandler>,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<Option<TextHandler>> {
        self.text().re_first(
            regex,
            default,
            replace_entities,
            clean_match,
            case_sensitive,
        )
    }

    /// Deserialize this element's text/body content as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        self.text().json()
    }

    // -- Storage (adaptive persistence) --------------------------------------

    /// Save this element's structural fingerprint to storage for adaptive relocation.
    ///
    /// The `identifier` is typically the CSS/XPath selector string used to find
    /// this element. On future pages, if the same selector fails, the stored
    /// fingerprint can be used to relocate the element by similarity.
    pub fn save(
        &self,
        storage: &dyn crate::storage::StorageSystem,
        identifier: &str,
    ) -> Result<()> {
        let data = crate::storage::ElementData::from_selector(self);
        storage.save(&data, identifier)
    }

    /// Retrieve a previously saved element fingerprint from storage.
    pub fn retrieve(
        storage: &dyn crate::storage::StorageSystem,
        identifier: &str,
    ) -> Result<Option<crate::storage::ElementData>> {
        storage.retrieve(identifier)
    }

    // -- Selector generation -------------------------------------------------

    /// Generate a unique CSS selector for this element within its document.
    ///
    /// Traverses ancestors, using IDs as shortcuts and `:nth-of-type()` for
    /// disambiguation among siblings with the same tag.
    pub fn generate_css_selector(&self) -> String {
        self.generate_selector(SelectorFormat::Css, false)
    }

    /// Generate a full-path CSS selector from the root to this element.
    pub fn generate_full_css_selector(&self) -> String {
        self.generate_selector(SelectorFormat::Css, true)
    }

    /// Generate a unique XPath selector for this element.
    pub fn generate_xpath_selector(&self) -> String {
        self.generate_selector(SelectorFormat::XPath, false)
    }

    /// Generate a full-path XPath selector from the root to this element.
    pub fn generate_full_xpath_selector(&self) -> String {
        self.generate_selector(SelectorFormat::XPath, true)
    }

    /// Core selector generation engine.
    fn generate_selector(&self, format: SelectorFormat, full_path: bool) -> String {
        if self.is_text_node {
            return String::new();
        }

        let mut parts = Vec::new();
        let mut current_id = Some(self.node_id);

        while let Some(nid) = current_id {
            let node_ref = self.doc.tree.get(nid).unwrap();
            match node_ref.value() {
                Node::Element(el) => {
                    let tag = el.name();
                    if tag == "html" {
                        if full_path {
                            parts.push(tag.to_owned());
                        }
                        break;
                    }

                    // ID shortcut: stop early unless full_path requested
                    if let Some(id_val) = el.attr("id") {
                        match format {
                            SelectorFormat::Css => parts.push(format!("#{id_val}")),
                            SelectorFormat::XPath => parts.push(format!("{tag}[@id='{id_val}']")),
                        }
                        if !full_path {
                            break;
                        }
                        current_id = node_ref.parent().map(|p| p.id());
                        continue;
                    }

                    // Compute nth-of-type among siblings
                    let nth = self.nth_of_type(nid, tag);
                    let part = if nth > 1 || self.has_same_tag_sibling(nid, tag) {
                        match format {
                            SelectorFormat::Css => format!("{tag}:nth-of-type({nth})"),
                            SelectorFormat::XPath => format!("{tag}[{nth}]"),
                        }
                    } else {
                        tag.to_owned()
                    };
                    parts.push(part);

                    current_id = node_ref.parent().map(|p| p.id());
                }
                Node::Document => break,
                _ => break,
            }
        }

        parts.reverse();
        match format {
            SelectorFormat::Css => parts.join(" > "),
            SelectorFormat::XPath => {
                if parts.is_empty() {
                    return String::new();
                }
                format!("//{}", parts.join("/"))
            }
        }
    }

    /// 1-based position of this node among same-tag siblings.
    fn nth_of_type(&self, node_id: NodeId, tag: &str) -> usize {
        let node_ref = self.doc.tree.get(node_id).unwrap();
        if let Some(parent) = node_ref.parent() {
            let mut count = 0;
            for sib in parent.children() {
                if let Node::Element(el) = sib.value() {
                    if el.name() == tag {
                        count += 1;
                        if sib.id() == node_id {
                            return count;
                        }
                    }
                }
            }
        }
        1
    }

    /// Whether this node has siblings with the same tag.
    fn has_same_tag_sibling(&self, node_id: NodeId, tag: &str) -> bool {
        let node_ref = self.doc.tree.get(node_id).unwrap();
        if let Some(parent) = node_ref.parent() {
            let count = parent
                .children()
                .filter(|c| matches!(c.value(), Node::Element(el) if el.name() == tag))
                .count();
            return count > 1;
        }
        false
    }

    // -- Helpers -------------------------------------------------------------

    /// The cached text for text-node selectors, or `""` if not a text node.
    fn text_node_str(&self) -> &str {
        self.text_value.as_deref().unwrap_or("")
    }

    /// Get the ego-tree node reference for this selector's node.
    fn node(&self) -> ego_tree::NodeRef<'_, Node> {
        self.doc.tree.get(self.node_id).unwrap()
    }
}

#[derive(Clone, Copy)]
enum SelectorFormat {
    Css,
    XPath,
}

impl fmt::Debug for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_text_node {
            let text = self.text_node_str();
            let display = if text.len() > 40 {
                format!("{}...", &text[..40])
            } else {
                text.to_owned()
            };
            return write!(f, "<text='{display}'>");
        }
        let html = clean_spaces(&self.outer_html());
        let display = if html.len() > 40 {
            format!("{}...", &html[..40])
        } else {
            html
        };
        write!(f, "<data='{display}'>")
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_text_node {
            return write!(f, "{}", self.text_node_str());
        }
        write!(f, "{}", self.outer_html())
    }
}

// ---------------------------------------------------------------------------
// Selectors (collection)
// ---------------------------------------------------------------------------

/// A collection of [`Selector`] values with batch CSS/regex operations.
///
/// Wraps a `Vec<Selector>` and provides chainable methods for filtering,
/// mapping, and extracting data from multiple elements at once.
///
/// # Examples
///
/// ```
/// # use scrapling::selector::Selector;
/// let sel = Selector::from_html("<ul><li>1</li><li>2</li><li>3</li></ul>");
/// let items = sel.css("li");
///
/// // Batch text extraction
/// let texts = items.getall();
/// assert_eq!(texts.len(), 3);
///
/// // Filter
/// let filtered = items.filter(|s| s.text().contains("2"));
/// assert_eq!(filtered.len(), 1);
/// ```
#[derive(Clone)]
pub struct Selectors(Vec<Selector>);

impl Selectors {
    /// Create from a pre-built vector.
    pub fn new(items: Vec<Selector>) -> Self {
        Self(items)
    }

    /// Empty collection.
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// First element, or `None`.
    pub fn first(&self) -> Option<&Selector> {
        self.0.first()
    }

    /// Last element, or `None`.
    pub fn last(&self) -> Option<&Selector> {
        self.0.last()
    }

    /// Iterate over selectors.
    pub fn iter(&self) -> impl Iterator<Item = &Selector> {
        self.0.iter()
    }

    /// Index access.
    pub fn get(&self, index: usize) -> Option<&Selector> {
        self.0.get(index)
    }

    // -- Batch selection -----------------------------------------------------

    /// Apply a CSS selector to each element and flatten results.
    pub fn css(&self, selector: &str) -> Selectors {
        let results: Vec<Selector> = self.0.iter().flat_map(|s| s.css(selector).0).collect();
        Selectors::new(results)
    }

    // -- Batch extraction ----------------------------------------------------

    /// Apply regex to each element's text and flatten results.
    pub fn re(
        &self,
        regex: &str,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<TextHandlers> {
        let mut all = Vec::new();
        for sel in &self.0 {
            let matches = sel.re(regex, replace_entities, clean_match, case_sensitive)?;
            all.extend(matches.into_iter());
        }
        Ok(TextHandlers::new(all))
    }

    /// Return the first regex match across all elements.
    pub fn re_first(
        &self,
        regex: &str,
        default: Option<TextHandler>,
        replace_entities: bool,
        clean_match: bool,
        case_sensitive: bool,
    ) -> Result<Option<TextHandler>> {
        for sel in &self.0 {
            let matches = sel.re(regex, replace_entities, clean_match, case_sensitive)?;
            if let Some(first) = matches.first().cloned() {
                return Ok(Some(first));
            }
        }
        Ok(default)
    }

    /// Serialize the first element, or return `None`.
    pub fn get_first(&self) -> Option<TextHandler> {
        self.0.first().map(|s| s.get())
    }

    /// Serialize all elements as a `TextHandlers` list.
    pub fn getall(&self) -> TextHandlers {
        TextHandlers::new(self.0.iter().map(|s| s.get()).collect())
    }

    // -- Filtering -----------------------------------------------------------

    /// Keep only elements matching the predicate.
    pub fn filter(&self, predicate: impl Fn(&Selector) -> bool) -> Selectors {
        Selectors::new(self.0.iter().filter(|s| predicate(s)).cloned().collect())
    }

    /// Return the first element matching the predicate.
    pub fn search(&self, predicate: impl Fn(&Selector) -> bool) -> Option<&Selector> {
        self.0.iter().find(|s| predicate(s))
    }
}

impl std::ops::Index<usize> for Selectors {
    type Output = Selector;

    fn index(&self, index: usize) -> &Selector {
        &self.0[index]
    }
}

impl IntoIterator for Selectors {
    type Item = Selector;
    type IntoIter = std::vec::IntoIter<Selector>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Selectors {
    type Item = &'a Selector;
    type IntoIter = std::slice::Iter<'a, Selector>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Debug for Selectors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

// ---------------------------------------------------------------------------
// HTML serialisation helpers
// ---------------------------------------------------------------------------

/// Write the HTML representation of a node and its descendants.
fn write_node_html(doc: &Html, node_id: NodeId, out: &mut String) {
    let node_ref = doc.tree.get(node_id).unwrap();
    match node_ref.value() {
        Node::Element(el) => {
            out.push('<');
            out.push_str(el.name());
            for (key, val) in el.attrs() {
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                out.push_str(&html_escape_attr(val));
                out.push('"');
            }
            out.push('>');

            for child in node_ref.children() {
                write_node_html(doc, child.id(), out);
            }

            if !is_void_element(el.name()) {
                out.push_str("</");
                out.push_str(el.name());
                out.push('>');
            }
        }
        Node::Text(t) => {
            out.push_str(t);
        }
        Node::Comment(c) => {
            out.push_str("<!--");
            out.push_str(c);
            out.push_str("-->");
        }
        _ => {}
    }
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Detect charset from `<meta charset="...">` or `<meta http-equiv="Content-Type" content="...charset=...">`.
fn detect_meta_charset(html: &str) -> Option<String> {
    let re = Regex::new(r#"(?i)<meta[^>]+charset\s*=\s*["']?([^\s"';>]+)"#).ok()?;
    re.captures(html).map(|c| c[1].to_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_html() -> &'static str {
        r#"<html><body>
            <div id="main" class="container">
                <h1>Title</h1>
                <ul class="list">
                    <li class="item active">First</li>
                    <li class="item">Second</li>
                    <li class="item">Third</li>
                </ul>
                <a href="/about" class="link">About</a>
                <p>Some <strong>bold</strong> text</p>
            </div>
        </body></html>"#
    }

    fn sel() -> Selector {
        Selector::from_html(sample_html())
    }

    // -- Parsing & basic properties --

    #[test]
    fn parse_and_tag() {
        let s = sel();
        let h1 = s.css("h1").first().unwrap().clone();
        assert_eq!(h1.tag(), "h1");
    }

    #[test]
    fn text_content() {
        let s = sel();
        let h1 = s.css("h1").first().unwrap().clone();
        assert_eq!(h1.text().as_ref(), "Title");
    }

    #[test]
    fn attributes() {
        let s = sel();
        let link = s.css("a.link").first().unwrap().clone();
        assert_eq!(link.attrib()["href"].as_ref(), "/about");
        assert!(link.has_class("link"));
    }

    #[test]
    fn html_content() {
        let s = sel();
        let p = s.css("p").first().unwrap().clone();
        let inner = p.html_content().into_inner();
        assert!(inner.contains("<strong>bold</strong>"));
        assert!(inner.contains("Some "));
    }

    // -- Navigation --

    #[test]
    fn parent_and_children() {
        let s = sel();
        let ul = s.css("ul").first().unwrap().clone();
        let kids = ul.children();
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].tag(), "li");

        let parent = kids[0].parent().unwrap();
        assert_eq!(parent.tag(), "ul");
    }

    #[test]
    fn siblings() {
        let s = sel();
        let items = s.css("li");
        let first = &items[0];
        let sibs = first.siblings();
        assert_eq!(sibs.len(), 2);
    }

    #[test]
    fn next_and_previous() {
        let s = sel();
        let items = s.css("li");
        let first = &items[0];
        let second = first.next().unwrap();
        assert_eq!(second.text().as_ref(), "Second");

        let back = second.previous().unwrap();
        assert_eq!(back.text().as_ref(), "First");
    }

    #[test]
    fn ancestors_and_path() {
        let s = sel();
        let li = s.css("li").first().unwrap().clone();
        let ancestors = li.ancestors();
        let tags: Vec<&str> = ancestors.iter().map(|a| a.tag()).collect();
        assert!(tags.contains(&"ul"));
        assert!(tags.contains(&"div"));
        assert!(tags.contains(&"body"));
    }

    // -- CSS selection --

    #[test]
    fn css_basic() {
        let s = sel();
        assert_eq!(s.css("li").len(), 3);
        assert_eq!(s.css("li.active").len(), 1);
        assert_eq!(s.css("#main").len(), 1);
    }

    #[test]
    fn css_text_pseudo() {
        let s = sel();
        let texts = s.css("h1::text");
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].text().as_ref(), "Title");
    }

    #[test]
    fn css_attr_pseudo() {
        let s = sel();
        let hrefs = s.css("a::attr(href)");
        assert_eq!(hrefs.len(), 1);
        assert_eq!(hrefs[0].text().as_ref(), "/about");
    }

    // -- Text search --

    #[test]
    fn find_by_text_exact() {
        let s = sel();
        let results = s.find_by_text("Title", false, false, false);
        assert!(!results.is_empty());
        assert_eq!(results[0].tag(), "h1");
    }

    #[test]
    fn find_by_text_partial() {
        let s = sel();
        let results = s.find_by_text("eco", true, false, false);
        assert!(!results.is_empty());
    }

    // -- Extraction --

    #[test]
    fn get_and_getall() {
        let s = sel();
        let items = s.css("li");
        let all = items.getall();
        assert_eq!(all.len(), 3);
        assert!(all[0].contains("First"));
    }

    // -- Selector generation --

    #[test]
    fn generate_css_selector() {
        let s = sel();
        let link = s.css("a.link").first().unwrap().clone();
        let css = link.generate_css_selector();
        assert!(!css.is_empty());
        // Should use the parent's #main ID as shortcut
        assert!(css.contains("a") || css.contains("#"));
    }

    #[test]
    fn generate_xpath_selector() {
        let s = sel();
        let h1 = s.css("h1").first().unwrap().clone();
        let xpath = h1.generate_xpath_selector();
        assert!(xpath.starts_with("//"));
        assert!(xpath.contains("h1"));
    }

    // -- Selectors collection --

    #[test]
    fn selectors_filter() {
        let s = sel();
        let items = s.css("li");
        let active = items.filter(|s| s.has_class("active"));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].text().as_ref(), "First");
    }

    #[test]
    fn selectors_search() {
        let s = sel();
        let items = s.css("li");
        let found = items.search(|s| s.text().contains("Third"));
        assert!(found.is_some());
    }

    #[test]
    fn selectors_css_batch() {
        let s = sel();
        let divs = s.css("div");
        let all_lis = divs.css("li");
        assert_eq!(all_lis.len(), 3);
    }

    #[test]
    fn get_all_text() {
        let s = sel();
        let p = s.css("p").first().unwrap().clone();
        let all = p.get_all_text(" ", true, &[], true);
        assert!(all.contains("Some"));
        assert!(all.contains("bold"));
        assert!(all.contains("text"));
    }

    #[test]
    fn get_all_text_ignore_tags() {
        let s = Selector::from_html(
            "<div><p>visible</p><script>hidden</script><style>also hidden</style></div>",
        );
        let div = s.css("div").first().unwrap().clone();
        let text = div.get_all_text(" ", true, &["script", "style"], true);
        assert!(text.contains("visible"));
        assert!(!text.contains("hidden"));
    }

    #[test]
    fn urljoin() {
        let s = Selector::from_html_with_url("<a href='/page'>link</a>", "https://example.com/");
        let links = s.css("a");
        let link = links.first().unwrap();
        assert_eq!(link.urljoin("/page"), "https://example.com/page");
    }

    #[test]
    fn empty_html() {
        let s = Selector::from_html("");
        assert!(s.css("div").is_empty());
    }

    #[test]
    fn text_node_properties() {
        let s = Selector::from_html("<p>hello</p>");
        let texts = s.css("p::text");
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].tag(), "#text");
        assert_eq!(texts[0].text().as_ref(), "hello");
        assert!(texts[0].children().is_empty());
        assert!(texts[0].next().is_none());
    }
}
