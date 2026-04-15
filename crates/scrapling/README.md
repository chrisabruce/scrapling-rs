# scrapling

Fast, adaptive web scraping toolkit for Rust. The core crate of [scrapling-rs](https://github.com/chrisabruce/scrapling-rs).

## Features

- **HTML parsing** via html5ever with CSS selector support, including `::text` and `::attr()` pseudo-elements
- **DOM navigation**: parent, children, siblings, ancestors, descendants
- **Find elements** by text content, regex patterns, or compound filters
- **Auto-generate** unique CSS and XPath selectors for any element
- **Adaptive element relocation**: 12-factor structural similarity scoring that survives DOM restructuring, class renames, ID changes, and wrapper additions
- **SQLite-backed** fingerprint storage across scraping sessions
- **HTML to Markdown** and plain text conversion

## Quick start

```rust
use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <h1 class="title">Hello, Scrapling!</h1>
            <div class="products">
                <div class="product" data-id="1"><span class="price">$10.99</span></div>
                <div class="product" data-id="2"><span class="price">$24.99</span></div>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // CSS selectors with pseudo-elements
    let prices = page.css(".price::text");
    for price in prices.iter() {
        println!("{}", price.text());
    }

    // Find elements by text
    let matches = page.find_by_text("$10", true, false, false);
    println!("Found {} elements containing '$10'", matches.len());
}
```

## Adaptive relocation

```rust
use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

fn main() {
    let storage = SqliteStorage::new(":memory:", Some("https://example.com")).unwrap();

    // Save a fingerprint from the original page
    let page = Selector::from_html(r#"<div id="price" class="amount">$42.99</div>"#);
    page.css_adaptive("#price", &storage, false, true, Some("price"), 0.0);

    // Website redesigns, the ID is gone, class changed
    let new_page = Selector::from_html(r#"<span class="cost" data-type="price">$42.99</span>"#);

    // Adaptive finds it by structural similarity
    let found = new_page.css_adaptive("#price", &storage, true, false, Some("price"), 0.0);
    assert!(!found.is_empty());
}
```

## License

MIT
