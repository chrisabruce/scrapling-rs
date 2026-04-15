# Querying Elements

This document covers the ways you can find and filter elements in a parsed HTML document using the scrapling core crate.

All examples assume you have a `Selector` in scope:

```rust
use scrapling::selector::{Selector, Selectors};

let page = Selector::from_html(r#"
    <html><body>
        <div id="main">
            <h1>Products</h1>
            <ul class="product-list">
                <li class="product" data-id="1"><a href="/p/1">Widget</a> <span class="price">$9.99</span></li>
                <li class="product" data-id="2"><a href="/p/2">Gadget</a> <span class="price">$14.50</span></li>
                <li class="product featured" data-id="3"><a href="/p/3">Doohickey</a> <span class="price">$22.00</span></li>
            </ul>
        </div>
    </body></html>
"#);
```

## CSS selectors

The `css()` method accepts standard CSS selectors. It returns a `Selectors` collection (never panics -- returns an empty collection for invalid selectors or no matches).

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<ul><li class=\"product\">A</li><li class=\"product\">B</li></ul>");
let products = page.css("li.product");
assert_eq!(products.len(), 2);

// Compound selectors work fine
let headings_and_links = page.css("h1, a");
```

### The `::text` pseudo-element

Append `::text` to extract the direct text nodes of matched elements. Each text node becomes its own `Selector` in the result, so you get one entry per text segment.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<ul><li>Alpha</li><li>Beta</li></ul>");
let names = page.css("li::text");
assert_eq!(names.len(), 2);
assert_eq!(names[0].text().as_ref(), "Alpha");
assert_eq!(names[1].text().as_ref(), "Beta");
```

### The `::attr(name)` pseudo-element

Append `::attr(attribute_name)` to extract attribute values instead of elements. Like `::text`, each result becomes a text-node `Selector`.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<a href="/about">About</a><a href="/contact">Contact</a>"#);
let hrefs = page.css("a::attr(href)");
assert_eq!(hrefs[0].text().as_ref(), "/about");
assert_eq!(hrefs[1].text().as_ref(), "/contact");
```

## find_by_text

Search for elements whose direct text content matches a string. This walks all descendants and compares text, not structure.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<div><p>Hello World</p><p>Goodbye</p></div>");
// Exact match (case-sensitive, no whitespace normalization)
let exact = page.find_by_text("Hello World", false, true, false);
assert_eq!(exact.len(), 1);

// Partial match
let partial = page.find_by_text("Hello", true, true, false);
assert_eq!(partial.len(), 1);

// Case-insensitive
let insensitive = page.find_by_text("hello world", false, false, false);
assert_eq!(insensitive.len(), 1);

// With whitespace cleaning (normalizes tabs, newlines, multiple spaces)
let cleaned = page.find_by_text("Hello World", false, true, true);
```

Parameters:

| Parameter | Type | Description |
|---|---|---|
| `text` | `&str` | The text to search for |
| `partial` | `bool` | `true` = substring match, `false` = exact match |
| `case_sensitive` | `bool` | Whether comparison is case-sensitive |
| `clean_match` | `bool` | Normalize whitespace before comparing |

## find_by_regex

Like `find_by_text`, but matches against a regular expression pattern instead of a literal string.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<div><span>Price: $42.99</span><span>Stock: 5</span></div>");
// Find elements whose text contains a dollar amount
let prices = page.find_by_regex(r"\$\d+\.\d+", true, false).unwrap();
assert_eq!(prices.len(), 1);
assert!(prices[0].text().contains("$42.99"));

// Case-insensitive regex
let items = page.find_by_regex(r"(?i)stock", true, false).unwrap();
```

Parameters:

| Parameter | Type | Description |
|---|---|---|
| `pattern` | `&str` | Regex pattern (Rust `regex` crate syntax) |
| `case_sensitive` | `bool` | Whether the regex is case-sensitive |
| `clean_match` | `bool` | Normalize whitespace before matching |

Returns `Result<Selectors>` because the regex pattern can be invalid.

## find_all -- compound filters

`find_all` lets you combine tag names, attribute key/value pairs, regex patterns, and arbitrary closures in a single query. It builds a CSS selector from the structural filters, runs it, then post-filters the results.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<div><span class="price">$10</span><span class="price">$25</span><span class="label">Name</span></div>"#);
// Find all <span> elements with class="price"
let results = page.find_all(
    &["span"],                          // tags
    &[("class", "price")],              // attributes (key=value)
    &[],                                // regex patterns on text
    &[],                                // predicate closures
);
assert_eq!(results.len(), 2);
```

### Filtering by text regex

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<div><p>Price: $10</p><p>Name: Widget</p><p>Price: $25</p></div>"#);
// Find <p> elements whose text matches a price pattern
let prices = page.find_all(
    &["p"],
    &[],
    &[r"\$\d+"],                        // regex applied to element text
    &[],
);
assert_eq!(prices.len(), 2);
```

### Filtering with closures

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<ul><li class="item active">A</li><li class="item">B</li><li class="item active">C</li></ul>"#);
// Find list items that have the "active" class
let active: &dyn Fn(&Selector) -> bool = &|el| el.has_class("active");
let results = page.find_all(&["li"], &[], &[], &[active]);
assert_eq!(results.len(), 2);
```

You can combine all four filter types. Tags and attributes are used to build the CSS query (fast, handled by the selector engine), then regex patterns and closures are applied as post-filters on the results.

### find -- single result

`find` is identical to `find_all` but returns `Option<Selector>` (the first match).

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<div><span class="price">$10</span></div>"#);
let first_price = page.find(&["span"], &[("class", "price")], &[], &[]);
assert!(first_price.is_some());
```

## find_similar -- structural matching

Given a reference element, `find_similar` searches for other elements at the same DOM depth with similar structure. This is useful when you have one example of a repeated element (a product card, a table row, a news article) and want to find all the others.

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html(r#"
    <div class="grid">
        <div class="card"><h3>Widget</h3><span class="price">$10</span></div>
        <div class="card"><h3>Gadget</h3><span class="price">$20</span></div>
        <div class="card"><h3>Gizmo</h3><span class="price">$30</span></div>
    </div>
"#);

let first_card = page.css("div.card").first().unwrap().clone();

// Find other elements that look like this card
let similar = first_card.find_similar(None, false, &["href", "src"]);
assert_eq!(similar.len(), 2); // the other two cards
```

Parameters:

| Parameter | Type | Description |
|---|---|---|
| `similarity_threshold` | `Option<f64>` | Minimum attribute similarity ratio (0.0--1.0). Default: 0.2 |
| `match_text` | `bool` | Include text content in the similarity score |
| `ignore_attributes` | `&[&str]` | Attribute names to skip during comparison (typically `href`, `src`) |

The algorithm compares candidates at the same tree depth, with the same parent tag, using Jaro-Winkler similarity on attribute values. It ignores the reference element itself.

## Chaining on collections

`Selectors` (the collection type) supports `css()` for batch sub-selection and `filter()` for predicate-based narrowing.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<ul><li class="item active"><a href="/a">A</a></li><li class="item"><a href="/b">B</a></li></ul>"#);
let items = page.css("li.item");

// Sub-select within each matched element, results are flattened
let links = items.css("a::attr(href)");

// Filter with a closure
let active_items = items.filter(|el| el.has_class("active"));
assert_eq!(active_items.len(), 1);
```

## Regex on elements and collections

Both `Selector` and `Selectors` support `.re()` and `.re_first()` for running regex directly on element text.

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<div><span>Price: $42.99</span></div>"#);
let span = page.css("span").first().unwrap();

// Run regex on a single element's text
let matches = span.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
assert_eq!(matches[0].as_ref(), "42.99");

// Run regex across all elements in a collection
let all_spans = page.css("span");
let all_prices = all_spans.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
```
