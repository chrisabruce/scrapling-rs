use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

fn original_html() -> &'static str {
    r##"
    <html><body>
        <div class="products">
            <div class="product" id="p1">
                <h3>Product 1</h3>
                <span class="price">$10</span>
            </div>
            <div class="product" id="p2">
                <h3>Product 2</h3>
                <span class="price">$20</span>
            </div>
        </div>
    </body></html>
    "##
}

fn changed_html() -> &'static str {
    r##"
    <html><body>
        <div class="product-container">
            <section class="items">
                <div class="product-card" data-id="p1">
                    <h3>Product 1</h3>
                    <span class="cost">$10</span>
                </div>
                <div class="product-card" data-id="p2">
                    <h3>Product 2</h3>
                    <span class="cost">$20</span>
                </div>
            </section>
        </div>
    </body></html>
    "##
}

#[test]
fn adaptive_relocates_after_dom_change() {
    let storage = SqliteStorage::new(":memory:", Some("https://test.com")).unwrap();

    // Step 1: Parse original HTML and select with auto_save
    let page1 = Selector::from_html(original_html());
    let results = page1.css_adaptive("#p1", &storage, false, true, Some("target"), 0.0);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].attrib()["id"].as_ref(), "p1");

    // Step 2: Parse changed HTML — the #p1 selector no longer exists
    let page2 = Selector::from_html(changed_html());
    let direct = page2.css("#p1");
    assert_eq!(
        direct.len(),
        0,
        "direct selector should fail on changed HTML"
    );

    // Step 3: Use adaptive mode to relocate the element
    let relocated = page2.css_adaptive("#p1", &storage, true, false, Some("target"), 0.0);
    assert!(
        !relocated.is_empty(),
        "adaptive should find the relocated element"
    );
    // The relocated element should be a div (same tag as original)
    assert_eq!(relocated[0].tag(), "div");
    // Should contain "Product 1" somewhere in its subtree
    let all_text = relocated[0].get_all_text(" ", true, &[], true);
    assert!(
        all_text.as_ref().contains("Product"),
        "relocated element should contain product text, got: {all_text}"
    );
}

#[test]
fn adaptive_auto_save_updates_fingerprint() {
    let storage = SqliteStorage::new(":memory:", Some("https://test.com")).unwrap();

    let page = Selector::from_html(original_html());

    // First call with auto_save saves the fingerprint
    let r1 = page.css_adaptive(".price", &storage, false, true, Some("prices"), 0.0);
    assert_eq!(r1.len(), 2);

    // Retrieve should now return stored data
    let stored = Selector::retrieve(&storage, "prices").unwrap();
    assert!(stored.is_some(), "fingerprint should be saved");
    assert_eq!(stored.unwrap().tag, "span");
}

#[test]
fn adaptive_no_storage_returns_empty_on_miss() {
    let storage = SqliteStorage::new(":memory:", Some("https://test.com")).unwrap();
    let page = Selector::from_html(original_html());

    // Adaptive=true but nothing stored — should return empty
    let results = page.css_adaptive(".nonexistent", &storage, true, false, None, 0.0);
    assert!(results.is_empty());
}

#[test]
fn relocate_method_works_directly() {
    let page = Selector::from_html(original_html());
    let target = &page.css("#p1")[0];
    let data = scrapling::storage::ElementData::from_selector(target);

    let page2 = Selector::from_html(changed_html());
    let relocated = page2.relocate(&data, 0.0);
    assert!(
        !relocated.is_empty(),
        "relocate should find similar element"
    );
}

#[test]
fn css_adaptive_falls_back_to_normal_when_not_adaptive() {
    let storage = SqliteStorage::new(":memory:", Some("https://test.com")).unwrap();
    let page = Selector::from_html(original_html());

    // adaptive=false, selector works — should return normally
    let results = page.css_adaptive(".product", &storage, false, false, None, 0.0);
    assert_eq!(results.len(), 2);

    // adaptive=false, selector fails — should return empty (no fallback)
    let results = page.css_adaptive(".nonexistent", &storage, false, false, None, 0.0);
    assert!(results.is_empty());
}
