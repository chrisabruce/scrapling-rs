//! Adaptive element relocation — scrapling's headline feature.
//! When a page's DOM structure changes, find elements by structural similarity
//! instead of brittle CSS selectors.

use scrapling::selector::Selector;
use scrapling::storage::sqlite::SqliteStorage;

fn main() {
    // Step 1: Parse the original page and save a fingerprint
    let original_html = r#"
        <html><body>
            <div class="products">
                <div class="product" id="target-item">
                    <h3>Widget Pro</h3>
                    <span class="price">$49.99</span>
                </div>
            </div>
        </body></html>
    "#;

    let storage = SqliteStorage::new(":memory:", Some("https://shop.example.com")).unwrap();
    let page1 = Selector::from_html(original_html);

    // css_adaptive with auto_save=true saves the element's fingerprint
    let results = page1.css_adaptive("#target-item", &storage, false, true, Some("widget"), 0.0);
    println!("Original: found {} element(s)", results.len());
    println!("  Tag: {}", results[0].tag());
    println!("  Text: {}", results[0].get_all_text(" ", true, &[], true));

    // Step 2: The website redesigns — different classes, IDs, structure
    let redesigned_html = r#"
        <html><body>
            <section class="product-grid">
                <article class="product-card" data-sku="widget-pro">
                    <h3>Widget Pro</h3>
                    <div class="pricing">$49.99</div>
                </article>
            </section>
        </body></html>
    "#;

    let page2 = Selector::from_html(redesigned_html);

    // The old selector fails completely
    let direct = page2.css("#target-item");
    println!(
        "\nDirect '#target-item' on new page: {} matches",
        direct.len()
    );

    // But adaptive relocation finds it by structural similarity
    let relocated = page2.css_adaptive("#target-item", &storage, true, false, Some("widget"), 0.0);
    println!("Adaptive relocation: {} match(es)", relocated.len());
    if let Some(found) = relocated.first() {
        println!("  Tag: {}", found.tag());
        println!("  Text: {}", found.get_all_text(" ", true, &[], true));
    }
}
