# Core Types

This document covers the primary types in the `scrapling` crate: how to construct them, what they do, and how they relate to each other.

## Selector

`Selector` is the central type. It represents a single node in a parsed HTML document. Multiple `Selector` instances can point to different nodes in the same document cheaply -- they share the underlying tree via `Rc` and differ only by node ID.

### Construction

```rust
use scrapling::selector::Selector;

// Parse a full HTML document
let page = Selector::from_html("<html><body><p>hello</p></body></html>");

// Parse an HTML fragment (no <html><body> wrapper)
let fragment = Selector::from_fragment("<span>hi</span>");

// Parse with a base URL (used by urljoin)
let page = Selector::from_html_with_url(
    r#"<a href="/about">About</a>"#,
    "https://example.com"
);
assert_eq!(page.css("a").first().unwrap().urljoin("/about"), "https://example.com/about");

// Parse from raw bytes with encoding detection
let bytes: &[u8] = b"<p>hello</p>";
let page = Selector::from_bytes(bytes);

// Parse with explicit encoding
let page = Selector::from_bytes_with_encoding(bytes, "utf-8");

// Parse with options (comments, CDATA, URL)
use scrapling::ParseOptions;
let opts = ParseOptions {
    url: Some("https://example.com".into()),
    keep_comments: false,
    keep_cdata: false,
};
let page = Selector::from_html_with_options("<p>hello</p>", &opts);
```

### Basic properties

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html(r#"<div id="main" class="container"><p>Hello</p></div>"#);
let div = page.css("div").first().unwrap();

// Tag name
assert_eq!(div.tag(), "div");

// Direct text content (non-recursive)
assert!(div.text().is_empty()); // text is inside <p>, not directly in <div>
let p = div.css("p").first().unwrap();
assert_eq!(p.text().as_ref(), "Hello");

// Recursive text extraction
let all_text = div.get_all_text(" ", true, &[], true);
assert_eq!(all_text.as_ref(), "Hello");

// Attributes
let attrs = div.attrib();
assert_eq!(attrs["id"].as_ref(), "main");
assert!(div.has_class("container"));

// Inner and outer HTML
let inner = p.html_content(); // "Hello"
let outer = p.outer_html();   // "<p>Hello</p>"
```

### get_all_text parameters

`get_all_text` recursively collects text from all descendants with fine-grained control:

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html(r#"<div>Hello <script>var x=1;</script> <b>World</b></div>"#);
let div = page.css("div").first().unwrap();

// Skip script tags, strip whitespace, only non-empty segments
let clean = div.get_all_text(" ", true, &["script"], true);
assert_eq!(clean.as_ref(), "Hello World");
```

| Parameter | Type | Description |
|---|---|---|
| `separator` | `&str` | Inserted between text segments |
| `strip` | `bool` | Trim each segment before joining |
| `ignore_tags` | `&[&str]` | Skip elements with these tag names |
| `valid_values` | `bool` | Skip segments that are empty or whitespace-only |

### URL resolution

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html_with_url(
    r#"<a href="/products?page=2">Next</a>"#,
    "https://shop.example.com/products"
);

let link = page.css("a").first().unwrap();
let absolute = link.urljoin(&link.attrib()["href"]);
assert_eq!(absolute, "https://shop.example.com/products?page=2");
```

### Selector generation

Generate a unique CSS or XPath selector for any element (useful for debugging or adaptive storage):

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html(r#"<html><body><div><ul><li>First</li><li>Second</li></ul></div></body></html>"#);
let second_li = page.css("li").get(1).unwrap();

let css = second_li.generate_css_selector();
// Something like "div > ul > li:nth-of-type(2)"

let xpath = second_li.generate_xpath_selector();
// Something like "//div/ul/li[2]"
```

## Navigation

Every `Selector` has methods to walk the DOM tree.

```rust
# use scrapling::selector::Selector;
let page = Selector::from_html(r#"
    <div id="wrapper">
        <header>Header</header>
        <main><p>Content</p></main>
        <footer>Footer</footer>
    </div>
"#);

let main = page.css("main").first().unwrap();

// Parent
let parent = main.parent().unwrap();
assert_eq!(parent.tag(), "div");

// Children (direct element children only, no text nodes)
let wrapper = page.css("#wrapper").first().unwrap();
let kids = wrapper.children();
assert_eq!(kids.len(), 3); // header, main, footer

// Siblings (other children of same parent, excluding self)
let sibs = main.siblings();
assert_eq!(sibs.len(), 2); // header, footer

// Next / previous sibling
let next = main.next().unwrap();
assert_eq!(next.tag(), "footer");

let prev = main.previous().unwrap();
assert_eq!(prev.tag(), "header");

// Ancestors (parent, grandparent, ... up to root)
let p = page.css("p").first().unwrap();
let ancestors = p.ancestors();
// [main, div#wrapper, body, html]

// Path (root to element, ancestors reversed)
let path = p.path();
// [html, body, div#wrapper, main, p]

// Descendants (all elements below, depth-first)
let all_below = wrapper.descendants();

// Find first ancestor matching a condition
let ancestor_div = p.find_ancestor(|a| a.tag() == "div");
assert!(ancestor_div.is_some());
```

## Selectors (collection)

`Selectors` wraps `Vec<Selector>` with batch operations. It is what `css()`, `find_all()`, `children()`, `siblings()`, and other multi-result methods return.

### Indexing and iteration

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<ul><li>A</li><li>B</li><li>C</li></ul>");
let items = page.css("li");

// Index access
assert_eq!(items[0].text().as_ref(), "A");
assert_eq!(items.get(1).unwrap().text().as_ref(), "B");

// First and last
assert_eq!(items.first().unwrap().text().as_ref(), "A");
assert_eq!(items.last().unwrap().text().as_ref(), "C");

// Length
assert_eq!(items.len(), 3);
assert!(!items.is_empty());

// Iteration
for item in &items {
    println!("{}", item.text());
}
```

### Filter

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<ul><li class="a">A</li><li class="b">B</li><li class="a">C</li></ul>"#);
let items = page.css("li");

let filtered = items.filter(|el| el.has_class("a"));
assert_eq!(filtered.len(), 2);

// search returns the first match
let found = items.search(|el| el.text().contains("B"));
assert!(found.is_some());
```

### Batch CSS on a collection

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html(r#"<div class="card"><a href="/1">One</a></div><div class="card"><a href="/2">Two</a></div>"#);
let cards = page.css("div.card");

// css() on Selectors runs the selector on each element and flattens
let all_links = cards.css("a::attr(href)");
assert_eq!(all_links.len(), 2);
```

### Batch extraction

```rust
# use scrapling::selector::Selector;
# let page = Selector::from_html("<ul><li>1</li><li>2</li><li>3</li></ul>");
let items = page.css("li");

// getall() serializes each element
let html_list = items.getall();
assert_eq!(html_list.len(), 3);

// get_first() returns the first serialized element
let first = items.get_first();
assert!(first.is_some());

// Batch regex across all elements
let numbers = items.re(r"\d+", false, false, true).unwrap();
assert_eq!(numbers.len(), 3);
```

## TextHandler

`TextHandler` wraps a `String` and adds scraping-specific methods. It implements `Deref<Target = str>`, so all standard string methods work directly.

Returned by `Selector::text()`, `Selector::html_content()`, attribute lookups, and regex results.

### Construction

```rust
use scrapling::TextHandler;

let a = TextHandler::new("hello");
let b = TextHandler::from("hello");
let c: TextHandler = String::from("hello").into();
```

### Regex

```rust
# use scrapling::TextHandler;
let t = TextHandler::new("price: $42.99 and $10.50");

// All matches -- capture groups are extracted if present
let prices = t.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
assert_eq!(prices[0].as_ref(), "42.99");
assert_eq!(prices[1].as_ref(), "10.50");

// First match only
let first = t.re_first(r"\$(\d+\.\d+)", None, false, false, true).unwrap();
assert_eq!(first.unwrap().as_ref(), "42.99");

// With a default for no-match
let fallback = TextHandler::new("N/A");
let result = t.re_first(r"EUR", Some(fallback), false, false, true).unwrap();
assert_eq!(result.unwrap().as_ref(), "N/A");

// Boolean match check (no allocation)
assert!(t.re_matches(r"\$\d+", true).unwrap());
```

Parameters for `re()` / `re_first()`:

| Parameter | Type | Description |
|---|---|---|
| `regex` | `&str` | Pattern (Rust `regex` crate syntax) |
| `replace_entities` | `bool` | Decode HTML entities in each match |
| `clean_match` | `bool` | Normalize whitespace before matching |
| `case_sensitive` | `bool` | Case-sensitive matching |

### Cleaning

```rust
# use scrapling::TextHandler;
let raw = TextHandler::new("  Price:\t$42.99\n\n  ");

// Normalize whitespace + trim
let cleaned = raw.clean(false);
assert_eq!(cleaned.as_ref(), "Price: $42.99");

// Also decode HTML entities
let encoded = TextHandler::new("Price: &#36;42.99 &amp; tax");
let decoded = encoded.clean(true);
assert!(decoded.contains("$42.99 & tax"));
```

### JSON

```rust
# use scrapling::TextHandler;
let json_text = TextHandler::new(r#"{"name": "Widget", "price": 42.99}"#);
let value: serde_json::Value = json_text.json().unwrap();
assert_eq!(value["name"], "Widget");
```

### String transforms

All transforms return `TextHandler`, preserving the enriched type through chains:

```rust
# use scrapling::TextHandler;
let t = TextHandler::new("Hello World");

let upper = t.to_uppercase_text();   // "HELLO WORLD"
let lower = t.to_lowercase_text();   // "hello world"
let replaced = t.replace_text("World", "Rust"); // "Hello Rust"
let trimmed = TextHandler::new("  hi  ").trim_text(); // "hi"

// split returns TextHandlers (collection)
let parts = TextHandler::new("a,b,c").split_text(",");
assert_eq!(parts.len(), 3);
```

## TextHandlers (collection)

`TextHandlers` wraps `Vec<TextHandler>` with batch regex operations. Implements `Deref<Target = Vec<TextHandler>>` so indexing, `len()`, `is_empty()`, iteration all work.

### Batch regex

```rust
use scrapling::{TextHandler, TextHandlers};

let handlers = TextHandlers::new(vec![
    TextHandler::new("item 1 costs $10"),
    TextHandler::new("item 2 costs $20"),
]);

// re() fans out to every element and flattens
let prices = handlers.re(r"\$(\d+)", false, false, true).unwrap();
assert_eq!(prices.len(), 2);
assert_eq!(prices[0].as_ref(), "10");
assert_eq!(prices[1].as_ref(), "20");

// re_first() returns the first match across all elements
let first = handlers.re_first(r"\$(\d+)", None, false, false, true).unwrap();
assert_eq!(first.unwrap().as_ref(), "10");
```

### Construction

```rust
# use scrapling::{TextHandler, TextHandlers};
// From a vector
let a = TextHandlers::new(vec![TextHandler::new("a"), TextHandler::new("b")]);

// From an iterator via collect
let b: TextHandlers = vec!["x", "y", "z"]
    .into_iter()
    .map(TextHandler::new)
    .collect();
assert_eq!(b.len(), 3);
```

## AttributesHandler

A read-only map of HTML element attributes. Values are `TextHandler` instances, so you get regex and cleaning methods directly on attribute values.

### Access patterns

```rust
use scrapling::AttributesHandler;

let attrs = AttributesHandler::new([
    ("class".to_owned(), "main-content featured".to_owned()),
    ("data-price".to_owned(), "42.99".to_owned()),
    ("id".to_owned(), "product-1".to_owned()),
]);

// Direct index (panics on missing key)
assert_eq!(attrs["data-price"].as_ref(), "42.99");

// Fallible access
assert!(attrs.get("missing").is_none());

// Check existence
assert!(attrs.contains_key("class"));

// Length
assert_eq!(attrs.len(), 3);
assert!(!attrs.is_empty());
```

### Search across values

```rust
# use scrapling::AttributesHandler;
# let attrs = AttributesHandler::new([
#     ("class".to_owned(), "main-content featured".to_owned()),
#     ("id".to_owned(), "product-1".to_owned()),
# ]);
// Partial match -- find attributes whose values contain "main"
let results = attrs.search_values("main", true);
assert_eq!(results.len(), 1);
assert!(results[0].contains_key("class"));

// Exact match
let exact = attrs.search_values("product-1", false);
assert_eq!(exact.len(), 1);
assert!(exact[0].contains_key("id"));
```

### Iteration

```rust
# use scrapling::AttributesHandler;
# let attrs = AttributesHandler::new([("class".to_owned(), "main".to_owned())]);
// Keys
for key in attrs.keys() {
    println!("attribute: {}", key);
}

// Key-value pairs
for (key, value) in &attrs {
    println!("{}={}", key, value);
}

// Values (as TextHandler references)
for val in attrs.values() {
    // regex and cleaning are available here
    println!("{}", val);
}
```

### JSON serialization

```rust
# use scrapling::AttributesHandler;
# let attrs = AttributesHandler::new([("id".to_owned(), "test".to_owned())]);
// As a JSON string
let json_str = attrs.json_string().unwrap();

// As a serde_json::Value
let json_val = attrs.json_value().unwrap();
assert_eq!(json_val["id"], "test");
```

### Regex on attribute values

Since values are `TextHandler`, you can run regex directly:

```rust
# use scrapling::AttributesHandler;
let attrs = AttributesHandler::new([
    ("data-info".to_owned(), "price:42.99;stock:5".to_owned()),
]);

let price = attrs["data-info"].re_first(r"price:(\d+\.\d+)", None, false, false, true).unwrap();
assert_eq!(price.unwrap().as_ref(), "42.99");
```
