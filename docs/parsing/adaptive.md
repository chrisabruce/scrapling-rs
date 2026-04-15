# Adaptive Scraping

Web pages change. Classes get renamed, elements move around, redesigns happen. A CSS selector that works today may break tomorrow. Adaptive scraping solves this by storing a structural fingerprint of each element you care about and using similarity scoring to relocate it when the original selector fails.

## The problem

Say you are scraping product prices from an e-commerce site:

```rust,ignore
let price = page.css("div.price-tag").first().unwrap();
```

The site redesigns. `div.price-tag` becomes `span.product-cost`. Your scraper breaks. You fix the selector, redeploy, and wait for the next redesign.

Adaptive scraping automates the relocation step. Instead of hard-failing when a selector returns nothing, it searches the entire page for the element that most closely resembles the one you found last time.

## How it works

1. **Save**: When a CSS selector matches, scrapling saves the element's structural fingerprint to persistent storage (SQLite by default).
2. **Miss**: On a subsequent page load, if the same selector returns no results, scrapling retrieves the stored fingerprint.
3. **Relocate**: Every element in the current page is scored against the fingerprint using a 12-factor similarity algorithm. The best match(es) above a configurable threshold are returned.
4. **Update**: The relocated element's fingerprint replaces the stale one in storage, so future lookups benefit from the latest structure.

## The 12-factor similarity algorithm

Each candidate element is scored against the stored fingerprint across up to 12 dimensions. Only factors where the original element has data count toward the denominator, so the algorithm adapts to sparse elements naturally.

| # | Factor | Comparison method |
|---|---|---|
| 1 | Tag name | Exact match (0 or 1) |
| 2 | Text content | Jaro-Winkler similarity |
| 3 | All attributes | Dict diff (0.5 x key similarity + 0.5 x value similarity) |
| 4 | `class` attribute | Jaro-Winkler similarity |
| 5 | `id` attribute | Jaro-Winkler similarity |
| 6 | `href` attribute | Jaro-Winkler similarity |
| 7 | `src` attribute | Jaro-Winkler similarity |
| 8 | Ancestor path | Jaro-Winkler on the chain of ancestor tag names |
| 9 | Parent tag name | Jaro-Winkler similarity |
| 10 | Parent attributes | Dict diff |
| 11 | Parent text | Jaro-Winkler similarity |
| 12 | Sibling tags | Jaro-Winkler on sibling tag names |

Final score = `(sum / active_checks) * 100`, giving a percentage from 0 to 100.

The Python library uses `difflib.SequenceMatcher`. The Rust port uses Jaro-Winkler as a fast approximation that weights prefix matches more heavily -- a good fit for DOM attribute values where prefixes tend to be stable across redesigns.

## SqliteStorage

`SqliteStorage` is the default persistent backend. It stores element fingerprints in a SQLite database with WAL mode enabled for concurrent read access.

```rust,no_run
use scrapling::storage::sqlite::SqliteStorage;
use scrapling::storage::StorageSystem;

// Open (or create) a database, scoped to a specific URL
let storage = SqliteStorage::new("scraper_data.db", Some("https://shop.example.com")).unwrap();
```

Data is automatically scoped by the base domain extracted from the URL. Elements stored for `shop.example.com` are isolated from those stored for `news.example.org`. If you pass `None` for the URL, the key `"default"` is used.

The storage system is trait-based (`StorageSystem`), so you can implement your own backend (Redis, Postgres, in-memory, etc.) by implementing `save()` and `retrieve()`:

```rust,ignore
pub trait StorageSystem {
    fn save(&self, data: &ElementData, identifier: &str) -> Result<()>;
    fn retrieve(&self, identifier: &str) -> Result<Option<ElementData>>;
}
```

## Using css_adaptive()

`css_adaptive()` is the main entry point. It wraps `css()` with automatic save-on-match and relocate-on-miss behavior.

```rust,no_run
use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

let storage = SqliteStorage::new("elements.db", Some("https://shop.example.com")).unwrap();
let page = Selector::from_html(r#"<div class="price">$42.99</div>"#);

let results = page.css_adaptive(
    "div.price",       // CSS selector
    &storage,          // storage backend
    true,              // adaptive: try relocation on miss
    true,              // auto_save: save fingerprint on match
    None,              // identifier: defaults to selector string
    50.0,              // percentage: minimum similarity score (0-100)
);
```

Parameters:

| Parameter | Type | Description |
|---|---|---|
| `selector` | `&str` | The CSS selector to try first |
| `storage` | `&dyn StorageSystem` | Where to save/retrieve fingerprints |
| `adaptive` | `bool` | If `true`, attempt relocation when the selector finds nothing |
| `auto_save` | `bool` | If `true`, save the first matched element's fingerprint |
| `identifier` | `Option<&str>` | Storage key. Defaults to the selector string if `None` |
| `percentage` | `f64` | Minimum similarity score (0--100) for relocation to succeed |

The flow:

1. Run `css(selector)` normally.
2. If results are found and `auto_save` is true, save the first result's fingerprint.
3. If no results and `adaptive` is true, retrieve the stored fingerprint and run `relocate()`.
4. If relocation succeeds and `auto_save` is true, save the relocated element's updated fingerprint.

## relocate() for manual relocation

If you need more control, use `relocate()` directly with an `ElementData` fingerprint.

```rust
use scrapling::selector::Selector;
use scrapling::storage::ElementData;

// Step 1: Build a fingerprint from a known-good page
let old_page = Selector::from_html(r#"
    <html><body>
        <div class="price-tag">$42.99</div>
    </body></html>
"#);
let old_element = old_page.css("div.price-tag").first().unwrap();
let fingerprint = ElementData::from_selector(old_element);

// Step 2: On a redesigned page, relocate
let new_page = Selector::from_html(r#"
    <html><body>
        <span class="product-cost">$42.99</span>
    </body></html>
"#);

let found = new_page.relocate(&fingerprint, 0.0);
assert!(!found.is_empty());
assert_eq!(found[0].text().as_ref(), "$42.99");
```

The `min_percentage` parameter (0--100) controls how picky the match is. Use 0 to always return the best match regardless of score. Use 80+ for high-confidence matches only.

## Step-by-step example: surviving a redesign

Here is a complete workflow showing how adaptive scraping handles a page redesign.

### Day 1: Original page

```rust,no_run
use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

let storage = SqliteStorage::new("product_scraper.db", Some("https://shop.example.com")).unwrap();

let html = r#"
    <html><body>
        <div class="product-card">
            <h2 class="product-name">Mechanical Keyboard</h2>
            <div class="price-tag">$89.99</div>
            <span class="stock-count">In Stock (12)</span>
        </div>
    </body></html>
"#;

let page = Selector::from_html(html);

// auto_save=true stores the fingerprint of div.price-tag
let price = page.css_adaptive("div.price-tag", &storage, true, true, None, 50.0);
assert_eq!(price[0].text().as_ref(), "$89.99");
// At this point, storage contains the structural fingerprint of the price element:
// tag="div", class="price-tag", parent="div.product-card", text="$89.99", etc.
```

### Day 30: Site redesign

The site renames `div.price-tag` to `span.cost`. Your original selector `div.price-tag` will return nothing.

```rust,no_run
# use scrapling::selector::Selector;
# use scrapling::storage::sqlite::SqliteStorage;
# let storage = SqliteStorage::new("product_scraper.db", Some("https://shop.example.com")).unwrap();
let new_html = r#"
    <html><body>
        <div class="product-card">
            <h2 class="product-name">Mechanical Keyboard</h2>
            <span class="cost">$89.99</span>
            <span class="availability">In Stock (12)</span>
        </div>
    </body></html>
"#;

let page = Selector::from_html(new_html);

// css("div.price-tag") finds nothing.
// adaptive=true kicks in: retrieves stored fingerprint, scores every element.
// span.cost scores highest because it shares parent, position, text content, etc.
let price = page.css_adaptive("div.price-tag", &storage, true, true, None, 50.0);
assert_eq!(price[0].text().as_ref(), "$89.99");
// auto_save updates the stored fingerprint to match span.cost
```

The scraper keeps working without any code changes. The stored fingerprint is updated to reflect the new structure, so subsequent runs are fast lookups rather than full-page scans.

## ElementData -- the fingerprint struct

`ElementData` captures everything the similarity algorithm needs:

```rust,ignore
pub struct ElementData {
    pub tag: String,                              // "div"
    pub attributes: HashMap<String, String>,      // {"class": "price-tag"}
    pub text: Option<String>,                     // "$42.99"
    pub path: Vec<String>,                        // ["html", "body", "div", "div"]
    pub parent_name: Option<String>,              // "div"
    pub parent_attribs: Option<HashMap<String, String>>,
    pub parent_text: Option<String>,
    pub siblings: Vec<String>,                    // ["h2", "span"]
    pub children: Vec<String>,                    // []
}
```

Build one from any `Selector`:

```rust
# use scrapling::selector::Selector;
# use scrapling::storage::ElementData;
let page = Selector::from_html(r#"<div id="x"><p class="intro">Hello</p></div>"#);
let p = page.css("p").first().unwrap();

let data = ElementData::from_selector(p);
assert_eq!(data.tag, "p");
assert_eq!(data.attributes["class"], "intro");
assert_eq!(data.text, Some("Hello".into()));
assert_eq!(data.parent_name, Some("div".into()));
```

`ElementData` is `Serialize` / `Deserialize`, so it can be stored as JSON in any backend.

## Tuning the similarity threshold

The `percentage` / `min_percentage` parameter controls the confidence threshold:

- **0--30**: Very loose. Will match elements that share only a few characteristics. Useful when the page structure changes dramatically but the text content is stable.
- **50--70**: Moderate. Good default for most sites. Catches renames and minor restructuring.
- **80--100**: Strict. Only matches elements that are nearly identical in structure. Use when you need high confidence and would rather fail than match the wrong element.

If `relocate()` returns an empty `Selectors`, no candidate met the threshold. Either the element was removed entirely or the threshold is too strict for the degree of change.
