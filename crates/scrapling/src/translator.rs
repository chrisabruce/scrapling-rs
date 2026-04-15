//! CSS-to-XPath translation with `::text` and `::attr()` pseudo-element support.
//!
//! This module provides the translation layer between CSS selectors and XPath
//! expressions, including support for the Scrapy/parsel-compatible
//! pseudo-elements `::text` and `::attr(ATTR_NAME)`.
//!
//! # Background
//!
//! Python scrapling (via `cssselect`) translates every CSS selector to an
//! XPath expression, which `lxml` then evaluates against the DOM. In Rust,
//! the `scraper` crate matches CSS selectors natively — no XPath needed.
//!
//! This module exists for three reasons:
//!
//! 1. **Pseudo-element parsing** — `::text` and `::attr(name)` must be
//!    stripped before CSS matching and applied as post-processing. The
//!    [`CssQuery`] type handles this.
//!
//! 2. **CSS→XPath conversion** — the [`css_to_xpath()`] function provides
//!    feature parity with the Python API for users who need XPath strings
//!    (e.g. for `generate_xpath_selector()`).
//!
//! 3. **Caching** — translations are LRU-cached (capacity 256) since the same
//!    selectors are typically used repeatedly.
//!
//! # Pseudo-elements
//!
//! | Pseudo-element | XPath suffix | Meaning |
//! |---|---|---|
//! | `::text` | `/text()` | Select text node children |
//! | `::attr(href)` | `/@href` | Select the `href` attribute value |
//!
//! # Examples
//!
//! ```
//! use scrapling::translator::{CssQuery, css_to_xpath};
//!
//! // Parse pseudo-elements from a CSS selector
//! let q = CssQuery::parse("div.content::text").unwrap();
//! assert_eq!(q.css(), "div.content");
//! assert!(q.is_text());
//!
//! let q2 = CssQuery::parse("a.link::attr(href)").unwrap();
//! assert_eq!(q2.css(), "a.link");
//! assert_eq!(q2.attribute(), Some("href"));
//!
//! // Translate CSS to XPath
//! let xpath = css_to_xpath("div.content::text").unwrap();
//! assert!(xpath.ends_with("/text()"));
//! ```

use std::sync::Mutex;

use lru::LruCache;
use once_cell::sync::Lazy;

use crate::error::{Error, Result};

// ---------------------------------------------------------------------------
// CssQuery — parsed CSS selector with pseudo-element extraction
// ---------------------------------------------------------------------------

/// The pseudo-element extracted from a CSS selector.
///
/// Scrapling supports two pseudo-elements that extend standard CSS:
///
/// - [`PseudoElement::Text`] — `::text`, selects text node children.
/// - [`PseudoElement::Attr`] — `::attr(name)`, selects a specific attribute value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PseudoElement {
    /// `::text` — select text node children of matched elements.
    Text,
    /// `::attr(name)` — select the named attribute value of matched elements.
    Attr(String),
}

/// A parsed CSS selector with any pseudo-element extracted.
///
/// CSS selectors like `div.content::text` or `a::attr(href)` contain
/// pseudo-elements that are not part of the standard CSS matching process.
/// `CssQuery` splits the selector into:
///
/// - **`css`** — the base CSS selector without the pseudo-element (e.g. `div.content`)
/// - **`pseudo`** — the extracted pseudo-element, if any
///
/// # Usage in the selection pipeline
///
/// 1. Parse the user's CSS string with [`CssQuery::parse()`].
/// 2. Match elements using `css()` (the base selector without pseudo-element).
/// 3. Post-process results based on the [`pseudo`](CssQuery::pseudo) field:
///    - `None` → return matched elements as-is.
///    - `Some(Text)` → extract text content from matched elements.
///    - `Some(Attr(name))` → extract the named attribute from matched elements.
///
/// # Examples
///
/// ```
/// use scrapling::translator::CssQuery;
///
/// let q = CssQuery::parse("ul > li.active::text").unwrap();
/// assert_eq!(q.css(), "ul > li.active");
/// assert!(q.is_text());
/// assert!(!q.is_attr());
///
/// let q2 = CssQuery::parse("img::attr(src)").unwrap();
/// assert_eq!(q2.css(), "img");
/// assert_eq!(q2.attribute(), Some("src"));
///
/// let q3 = CssQuery::parse("div.plain").unwrap();
/// assert!(q3.pseudo().is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CssQuery {
    css: String,
    pseudo: Option<PseudoElement>,
}

impl CssQuery {
    /// Parse a CSS selector string, extracting any `::text` or `::attr(name)`
    /// pseudo-element.
    ///
    /// The pseudo-element must appear at the **end** of the selector. If no
    /// pseudo-element is present, the entire string is treated as a plain CSS
    /// selector.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CssSelector`] if a `::attr()` pseudo-element has
    /// invalid syntax (e.g. missing parentheses or empty attribute name).
    ///
    /// # Examples
    ///
    /// ```
    /// # use scrapling::translator::CssQuery;
    /// let q = CssQuery::parse("div::text").unwrap();
    /// assert_eq!(q.css(), "div");
    /// assert!(q.is_text());
    /// ```
    pub fn parse(selector: &str) -> Result<Self> {
        let trimmed = selector.trim();

        // Check for ::text pseudo-element
        if let Some(base) = trimmed.strip_suffix("::text") {
            let base = base.trim_end();
            if base.is_empty() {
                return Ok(Self {
                    css: "*".to_owned(),
                    pseudo: Some(PseudoElement::Text),
                });
            }
            return Ok(Self {
                css: base.to_owned(),
                pseudo: Some(PseudoElement::Text),
            });
        }

        // Check for ::attr(name) pseudo-element
        if let Some(rest) = trimmed.strip_suffix(')') {
            if let Some(attr_start) = rest.rfind("::attr(") {
                let attr_name = &rest[attr_start + 7..];
                let attr_name = attr_name.trim();
                if attr_name.is_empty() {
                    return Err(Error::CssSelector(
                        "::attr() pseudo-element requires an attribute name".to_owned(),
                    ));
                }
                let base = trimmed[..attr_start].trim_end();
                let css = if base.is_empty() {
                    "*".to_owned()
                } else {
                    base.to_owned()
                };
                return Ok(Self {
                    css,
                    pseudo: Some(PseudoElement::Attr(attr_name.to_owned())),
                });
            }
        }

        // No pseudo-element
        Ok(Self {
            css: trimmed.to_owned(),
            pseudo: None,
        })
    }

    /// The base CSS selector without any pseudo-element.
    ///
    /// This is the part that should be used for CSS matching against the DOM.
    pub fn css(&self) -> &str {
        &self.css
    }

    /// The extracted pseudo-element, if any.
    pub fn pseudo(&self) -> Option<&PseudoElement> {
        self.pseudo.as_ref()
    }

    /// `true` if the selector had a `::text` pseudo-element.
    pub fn is_text(&self) -> bool {
        matches!(self.pseudo, Some(PseudoElement::Text))
    }

    /// `true` if the selector had a `::attr(name)` pseudo-element.
    pub fn is_attr(&self) -> bool {
        matches!(self.pseudo, Some(PseudoElement::Attr(_)))
    }

    /// If the selector had `::attr(name)`, return the attribute name.
    pub fn attribute(&self) -> Option<&str> {
        match &self.pseudo {
            Some(PseudoElement::Attr(name)) => Some(name),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// CSS → XPath translation
// ---------------------------------------------------------------------------

/// Global LRU cache for CSS→XPath translations (capacity 256).
///
/// Protected by a `Mutex` for thread safety. The cache key is the full CSS
/// selector string (including pseudo-elements); the value is the translated
/// XPath string.
static XPATH_CACHE: Lazy<Mutex<LruCache<String, String>>> =
    Lazy::new(|| Mutex::new(LruCache::new(std::num::NonZeroUsize::new(256).unwrap())));

/// Translate a CSS selector to an XPath expression.
///
/// Supports the `::text` and `::attr(name)` pseudo-elements, which append
/// `/text()` and `/@name` to the generated XPath respectively.
///
/// Results are cached in a global LRU cache (capacity 256) so repeated
/// translations of the same selector are essentially free.
///
/// # Supported CSS features
///
/// | CSS | XPath |
/// |---|---|
/// | `tag` | `descendant-or-self::tag` |
/// | `.class` | `contains(concat(' ', normalize-space(@class), ' '), ' class ')` |
/// | `#id` | `[@id='value']` |
/// | `[attr]` | `[@attr]` |
/// | `[attr=val]` | `[@attr='val']` |
/// | `[attr~=val]` | `contains(concat(' ', @attr, ' '), ' val ')` |
/// | `[attr^=val]` | `starts-with(@attr, 'val')` |
/// | `[attr$=val]` | `substring(@attr, string-length(@attr) - N + 1) = 'val'` |
/// | `[attr*=val]` | `contains(@attr, 'val')` |
/// | `parent > child` | `/child` |
/// | `ancestor descendant` | `//descendant` (descendant-or-self) |
/// | `prev + next` | `following-sibling::next[1]` |
/// | `prev ~ sibling` | `following-sibling::sibling` |
/// | `::text` | `/text()` |
/// | `::attr(name)` | `/@name` |
/// | `:first-child` | `[1]` |
/// | `:last-child` | `[last()]` |
/// | `:nth-child(n)` | `[position()=n]` |
/// | `*` | `*` |
///
/// # Errors
///
/// Returns [`Error::CssSelector`] if the selector contains syntax that
/// cannot be parsed.
///
/// # Examples
///
/// ```
/// use scrapling::translator::css_to_xpath;
///
/// let xpath = css_to_xpath("div.content").unwrap();
/// assert!(xpath.starts_with("descendant-or-self::"));
///
/// let xpath_text = css_to_xpath("p::text").unwrap();
/// assert!(xpath_text.ends_with("/text()"));
///
/// let xpath_attr = css_to_xpath("a::attr(href)").unwrap();
/// assert!(xpath_attr.ends_with("/@href"));
/// ```
pub fn css_to_xpath(css: &str) -> Result<String> {
    // Check cache first
    {
        let mut cache = XPATH_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&css.to_owned()) {
            return Ok(cached.clone());
        }
    }

    let query = CssQuery::parse(css)?;
    let mut xpath = translate_css_to_xpath(query.css())?;

    // Append pseudo-element suffix
    match query.pseudo() {
        Some(PseudoElement::Text) => xpath.push_str("/text()"),
        Some(PseudoElement::Attr(name)) => {
            xpath.push_str("/@");
            xpath.push_str(name);
        }
        None => {}
    }

    // Store in cache
    {
        let mut cache = XPATH_CACHE.lock().unwrap();
        cache.put(css.to_owned(), xpath.clone());
    }

    Ok(xpath)
}

/// Core CSS→XPath translation engine.
///
/// Translates a CSS selector (without pseudo-elements) into an XPath
/// expression. Uses `descendant-or-self::` as the default axis prefix,
/// matching the behaviour of Python's `cssselect.HTMLTranslator`.
fn translate_css_to_xpath(css: &str) -> Result<String> {
    let css = css.trim();
    if css.is_empty() || css == "*" {
        return Ok("descendant-or-self::*".to_owned());
    }

    let tokens = tokenize_css(css)?;
    let xpath = tokens_to_xpath(&tokens)?;
    Ok(xpath)
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// A token in a parsed CSS selector.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CssToken {
    /// A tag name (e.g. `div`, `a`, `*`).
    Tag(String),
    /// A class selector (e.g. `.foo`).
    Class(String),
    /// An ID selector (e.g. `#bar`).
    Id(String),
    /// An attribute selector (e.g. `[href]`, `[data-x="val"]`).
    Attribute(AttrSelector),
    /// A pseudo-class (e.g. `:first-child`, `:nth-child(2)`).
    PseudoClass(String),
    /// Child combinator `>`.
    ChildCombinator,
    /// Descendant combinator (whitespace).
    DescendantCombinator,
    /// Adjacent sibling combinator `+`.
    AdjacentSibling,
    /// General sibling combinator `~`.
    GeneralSibling,
}

/// A parsed attribute selector like `[attr]`, `[attr=val]`, `[attr*=val]`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AttrSelector {
    name: String,
    op: Option<AttrOp>,
    value: Option<String>,
}

/// The comparison operator in an attribute selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    /// `=` — exact match.
    Equals,
    /// `~=` — space-separated word match.
    Includes,
    /// `^=` — starts with.
    StartsWith,
    /// `$=` — ends with.
    EndsWith,
    /// `*=` — contains substring.
    Contains,
    /// `|=` — dash-separated prefix match.
    DashMatch,
}

/// Tokenize a CSS selector into a sequence of [`CssToken`]s.
fn tokenize_css(css: &str) -> Result<Vec<CssToken>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = css.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            ' ' | '\t' | '\n' | '\r' => {
                // Whitespace = descendant combinator (unless followed by another combinator)
                i = skip_whitespace(&chars, i);
                if i < len && !matches!(chars[i], '>' | '+' | '~' | ',') {
                    // Only add descendant combinator if there's a preceding simple selector
                    if !tokens.is_empty() && !is_combinator(tokens.last()) {
                        tokens.push(CssToken::DescendantCombinator);
                    }
                }
            }
            '>' => {
                // Replace any trailing descendant combinator
                if matches!(tokens.last(), Some(CssToken::DescendantCombinator)) {
                    tokens.pop();
                }
                tokens.push(CssToken::ChildCombinator);
                i += 1;
                i = skip_whitespace(&chars, i);
            }
            '+' => {
                if matches!(tokens.last(), Some(CssToken::DescendantCombinator)) {
                    tokens.pop();
                }
                tokens.push(CssToken::AdjacentSibling);
                i += 1;
                i = skip_whitespace(&chars, i);
            }
            '~' => {
                if matches!(tokens.last(), Some(CssToken::DescendantCombinator)) {
                    tokens.pop();
                }
                tokens.push(CssToken::GeneralSibling);
                i += 1;
                i = skip_whitespace(&chars, i);
            }
            '#' => {
                i += 1;
                let (name, new_i) = read_ident(&chars, i)?;
                tokens.push(CssToken::Id(name));
                i = new_i;
            }
            '.' => {
                i += 1;
                let (name, new_i) = read_ident(&chars, i)?;
                tokens.push(CssToken::Class(name));
                i = new_i;
            }
            '[' => {
                let (attr, new_i) = read_attribute(&chars, i)?;
                tokens.push(CssToken::Attribute(attr));
                i = new_i;
            }
            ':' => {
                // Skip `::` (pseudo-elements handled by CssQuery::parse)
                if i + 1 < len && chars[i + 1] == ':' {
                    break;
                }
                i += 1;
                let (name, new_i) = read_pseudo_class(&chars, i)?;
                tokens.push(CssToken::PseudoClass(name));
                i = new_i;
            }
            '*' => {
                tokens.push(CssToken::Tag("*".to_owned()));
                i += 1;
            }
            c if is_ident_start(c) => {
                let (name, new_i) = read_ident(&chars, i)?;
                tokens.push(CssToken::Tag(name));
                i = new_i;
            }
            ',' => {
                // Comma-separated selectors should be handled at a higher level.
                // For now, stop at the comma.
                break;
            }
            c => {
                return Err(Error::CssSelector(format!(
                    "unexpected character '{c}' at position {i}"
                )));
            }
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// XPath generation from tokens
// ---------------------------------------------------------------------------

/// Convert a token stream into an XPath expression string.
fn tokens_to_xpath(tokens: &[CssToken]) -> Result<String> {
    let mut xpath = String::from("descendant-or-self::");
    let mut need_element = true;
    let mut predicates: Vec<String> = Vec::new();

    for (idx, token) in tokens.iter().enumerate() {
        match token {
            CssToken::Tag(name) => {
                if !need_element {
                    flush_predicates(&mut xpath, &mut predicates);
                }
                xpath.push_str(name);
                need_element = false;
            }
            CssToken::Class(class) => {
                if need_element {
                    xpath.push('*');
                    need_element = false;
                }
                predicates.push(format!(
                    "contains(concat(' ', normalize-space(@class), ' '), ' {class} ')"
                ));
            }
            CssToken::Id(id) => {
                if need_element {
                    xpath.push('*');
                    need_element = false;
                }
                predicates.push(format!("@id='{id}'"));
            }
            CssToken::Attribute(attr) => {
                if need_element {
                    xpath.push('*');
                    need_element = false;
                }
                predicates.push(attr_to_xpath_predicate(attr));
            }
            CssToken::PseudoClass(pseudo) => {
                if need_element {
                    xpath.push('*');
                    need_element = false;
                }
                predicates.push(pseudo_class_to_xpath(pseudo)?);
            }
            CssToken::DescendantCombinator => {
                flush_predicates(&mut xpath, &mut predicates);
                if need_element {
                    xpath.push('*');
                }
                xpath.push_str("//");
                need_element = true;
            }
            CssToken::ChildCombinator => {
                flush_predicates(&mut xpath, &mut predicates);
                if need_element {
                    xpath.push('*');
                }
                xpath.push('/');
                need_element = true;
            }
            CssToken::AdjacentSibling | CssToken::GeneralSibling => {
                flush_predicates(&mut xpath, &mut predicates);
                if need_element {
                    xpath.push('*');
                }
                xpath.push_str("/following-sibling::");
                need_element = true;
            }
        }
        // For adjacent sibling, add [1] predicate on the element that follows
        if idx > 0
            && matches!(tokens.get(idx - 1), Some(CssToken::AdjacentSibling))
            && !matches!(token, CssToken::AdjacentSibling | CssToken::GeneralSibling)
            && !need_element
        {
            predicates.insert(0, "1".to_owned());
        }
    }

    // If we still need an element at the end, add wildcard
    if need_element {
        xpath.push('*');
    }

    flush_predicates(&mut xpath, &mut predicates);

    Ok(xpath)
}

/// Flush accumulated predicates into the XPath string as `[pred1][pred2]...`.
fn flush_predicates(xpath: &mut String, predicates: &mut Vec<String>) {
    for pred in predicates.drain(..) {
        xpath.push('[');
        xpath.push_str(&pred);
        xpath.push(']');
    }
}

/// Convert an attribute selector to an XPath predicate string (without brackets).
fn attr_to_xpath_predicate(attr: &AttrSelector) -> String {
    match (&attr.op, &attr.value) {
        (None, _) => format!("@{}", attr.name),
        (Some(AttrOp::Equals), Some(val)) => format!("@{}='{}'", attr.name, escape_xpath(val)),
        (Some(AttrOp::Includes), Some(val)) => {
            format!(
                "contains(concat(' ', @{}, ' '), ' {} ')",
                attr.name,
                escape_xpath(val)
            )
        }
        (Some(AttrOp::StartsWith), Some(val)) => {
            format!("starts-with(@{}, '{}')", attr.name, escape_xpath(val))
        }
        (Some(AttrOp::EndsWith), Some(val)) => {
            let escaped = escape_xpath(val);
            format!(
                "substring(@{name}, string-length(@{name}) - {len} + 1) = '{val}'",
                name = attr.name,
                len = val.len(),
                val = escaped,
            )
        }
        (Some(AttrOp::Contains), Some(val)) => {
            format!("contains(@{}, '{}')", attr.name, escape_xpath(val))
        }
        (Some(AttrOp::DashMatch), Some(val)) => {
            let escaped = escape_xpath(val);
            format!(
                "@{name}='{val}' or starts-with(@{name}, '{val}-')",
                name = attr.name,
                val = escaped,
            )
        }
        (Some(_), None) => format!("@{}", attr.name),
    }
}

/// Convert a pseudo-class name to an XPath predicate.
fn pseudo_class_to_xpath(pseudo: &str) -> Result<String> {
    if pseudo == "first-child" {
        return Ok("position()=1".to_owned());
    }
    if pseudo == "last-child" {
        return Ok("position()=last()".to_owned());
    }
    if let Some(rest) = pseudo.strip_prefix("nth-child(") {
        let n = rest
            .strip_suffix(')')
            .ok_or_else(|| Error::CssSelector(format!("malformed :nth-child: {pseudo}")))?
            .trim();
        return Ok(format!("position()={n}"));
    }
    if pseudo == "only-child" {
        return Ok("last()=1".to_owned());
    }
    if pseudo == "empty" {
        return Ok("not(*)".to_owned());
    }
    if let Some(rest) = pseudo.strip_prefix("not(") {
        let inner = rest
            .strip_suffix(')')
            .ok_or_else(|| Error::CssSelector(format!("malformed :not: {pseudo}")))?
            .trim();
        let inner_tokens = tokenize_css(inner)?;
        let inner_xpath = tokens_to_xpath(&inner_tokens)?;
        // Extract just the predicate part from the inner xpath
        return Ok(format!(
            "not(self::{inner_xpath})",
            inner_xpath = inner_xpath
                .strip_prefix("descendant-or-self::")
                .unwrap_or(&inner_xpath)
        ));
    }

    Err(Error::CssSelector(format!(
        "unsupported pseudo-class :{pseudo}"
    )))
}

/// Escape single quotes in XPath string values.
fn escape_xpath(s: &str) -> String {
    s.replace('\'', "\\'")
}

// ---------------------------------------------------------------------------
// Tokenizer helpers
// ---------------------------------------------------------------------------

fn skip_whitespace(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || !c.is_ascii()
}

fn is_combinator(token: Option<&CssToken>) -> bool {
    matches!(
        token,
        Some(
            CssToken::ChildCombinator
                | CssToken::DescendantCombinator
                | CssToken::AdjacentSibling
                | CssToken::GeneralSibling
        )
    )
}

fn read_ident(chars: &[char], start: usize) -> Result<(String, usize)> {
    let mut i = start;
    while i < chars.len() && is_ident_char(chars[i]) {
        i += 1;
    }
    if i == start {
        return Err(Error::CssSelector(format!(
            "expected identifier at position {start}"
        )));
    }
    Ok((chars[start..i].iter().collect(), i))
}

fn read_pseudo_class(chars: &[char], start: usize) -> Result<(String, usize)> {
    let mut i = start;
    // Read the name
    while i < chars.len() && is_ident_char(chars[i]) {
        i += 1;
    }
    // Check for functional pseudo-class with parentheses
    if i < chars.len() && chars[i] == '(' {
        let mut depth = 1;
        i += 1;
        while i < chars.len() && depth > 0 {
            match chars[i] {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
    }
    if i == start {
        return Err(Error::CssSelector(format!(
            "expected pseudo-class at position {start}"
        )));
    }
    Ok((chars[start..i].iter().collect(), i))
}

fn read_attribute(chars: &[char], start: usize) -> Result<(AttrSelector, usize)> {
    debug_assert_eq!(chars[start], '[');
    let mut i = start + 1;
    i = skip_whitespace(chars, i);

    // Read attribute name
    let (name, new_i) = read_ident(chars, i)?;
    i = new_i;
    i = skip_whitespace(chars, i);

    // Check for operator
    if i < chars.len() && chars[i] == ']' {
        return Ok((
            AttrSelector {
                name,
                op: None,
                value: None,
            },
            i + 1,
        ));
    }

    // Read operator
    let op = match chars.get(i) {
        Some('=') => {
            i += 1;
            AttrOp::Equals
        }
        Some('~') if chars.get(i + 1) == Some(&'=') => {
            i += 2;
            AttrOp::Includes
        }
        Some('^') if chars.get(i + 1) == Some(&'=') => {
            i += 2;
            AttrOp::StartsWith
        }
        Some('$') if chars.get(i + 1) == Some(&'=') => {
            i += 2;
            AttrOp::EndsWith
        }
        Some('*') if chars.get(i + 1) == Some(&'=') => {
            i += 2;
            AttrOp::Contains
        }
        Some('|') if chars.get(i + 1) == Some(&'=') => {
            i += 2;
            AttrOp::DashMatch
        }
        _ => {
            return Err(Error::CssSelector(format!(
                "unexpected character in attribute selector at position {i}"
            )));
        }
    };

    i = skip_whitespace(chars, i);

    // Read value (quoted or unquoted)
    let value;
    if i < chars.len() && (chars[i] == '\'' || chars[i] == '"') {
        let quote = chars[i];
        i += 1;
        let val_start = i;
        while i < chars.len() && chars[i] != quote {
            i += 1;
        }
        value = chars[val_start..i].iter().collect();
        i += 1; // skip closing quote
    } else {
        let (val, new_i) = read_ident(chars, i)?;
        value = val;
        i = new_i;
    }

    i = skip_whitespace(chars, i);
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    } else {
        return Err(Error::CssSelector(
            "expected ']' to close attribute selector".to_owned(),
        ));
    }

    Ok((
        AttrSelector {
            name,
            op: Some(op),
            value: Some(value),
        },
        i,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- CssQuery parsing --

    #[test]
    fn parse_plain_selector() {
        let q = CssQuery::parse("div.content").unwrap();
        assert_eq!(q.css(), "div.content");
        assert!(q.pseudo().is_none());
    }

    #[test]
    fn parse_text_pseudo() {
        let q = CssQuery::parse("p.intro::text").unwrap();
        assert_eq!(q.css(), "p.intro");
        assert!(q.is_text());
    }

    #[test]
    fn parse_attr_pseudo() {
        let q = CssQuery::parse("a.link::attr(href)").unwrap();
        assert_eq!(q.css(), "a.link");
        assert_eq!(q.attribute(), Some("href"));
    }

    #[test]
    fn parse_bare_text() {
        let q = CssQuery::parse("::text").unwrap();
        assert_eq!(q.css(), "*");
        assert!(q.is_text());
    }

    #[test]
    fn parse_bare_attr() {
        let q = CssQuery::parse("::attr(class)").unwrap();
        assert_eq!(q.css(), "*");
        assert_eq!(q.attribute(), Some("class"));
    }

    #[test]
    fn parse_empty_attr_name_errors() {
        assert!(CssQuery::parse("div::attr()").is_err());
    }

    // -- CSS to XPath translation --

    #[test]
    fn translate_tag() {
        let xpath = css_to_xpath("div").unwrap();
        assert_eq!(xpath, "descendant-or-self::div");
    }

    #[test]
    fn translate_wildcard() {
        let xpath = css_to_xpath("*").unwrap();
        assert_eq!(xpath, "descendant-or-self::*");
    }

    #[test]
    fn translate_class() {
        let xpath = css_to_xpath(".content").unwrap();
        assert!(xpath.contains("normalize-space(@class)"));
        assert!(xpath.contains("content"));
    }

    #[test]
    fn translate_id() {
        let xpath = css_to_xpath("#main").unwrap();
        assert!(xpath.contains("@id='main'"));
    }

    #[test]
    fn translate_tag_with_class() {
        let xpath = css_to_xpath("div.content").unwrap();
        assert!(xpath.starts_with("descendant-or-self::div"));
        assert!(xpath.contains("content"));
    }

    #[test]
    fn translate_descendant() {
        let xpath = css_to_xpath("div p").unwrap();
        assert!(xpath.contains("//p"));
    }

    #[test]
    fn translate_child() {
        let xpath = css_to_xpath("div > p").unwrap();
        assert!(xpath.contains("/p"));
        assert!(!xpath.contains("//p"));
    }

    #[test]
    fn translate_attribute_exists() {
        let xpath = css_to_xpath("[href]").unwrap();
        assert!(xpath.contains("@href"));
    }

    #[test]
    fn translate_attribute_equals() {
        let xpath = css_to_xpath("[type='text']").unwrap();
        assert!(xpath.contains("@type='text'"));
    }

    #[test]
    fn translate_attribute_contains() {
        let xpath = css_to_xpath("[class*='active']").unwrap();
        assert!(xpath.contains("contains(@class, 'active')"));
    }

    #[test]
    fn translate_text_pseudo() {
        let xpath = css_to_xpath("p::text").unwrap();
        assert!(xpath.ends_with("/text()"));
    }

    #[test]
    fn translate_attr_pseudo() {
        let xpath = css_to_xpath("a::attr(href)").unwrap();
        assert!(xpath.ends_with("/@href"));
    }

    #[test]
    fn translate_complex() {
        let xpath = css_to_xpath("div.main > ul > li.active").unwrap();
        assert!(xpath.contains("div"));
        assert!(xpath.contains("ul"));
        assert!(xpath.contains("li"));
        assert!(xpath.contains("active"));
    }

    #[test]
    fn cache_works() {
        // Call twice — second should hit cache
        let first = css_to_xpath("div.test").unwrap();
        let second = css_to_xpath("div.test").unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn translate_first_child() {
        let xpath = css_to_xpath("li:first-child").unwrap();
        assert!(xpath.contains("position()=1"));
    }

    #[test]
    fn translate_adjacent_sibling() {
        let xpath = css_to_xpath("h1 + p").unwrap();
        assert!(xpath.contains("following-sibling"));
    }

    #[test]
    fn translate_general_sibling() {
        let xpath = css_to_xpath("h1 ~ p").unwrap();
        assert!(xpath.contains("following-sibling"));
    }
}
