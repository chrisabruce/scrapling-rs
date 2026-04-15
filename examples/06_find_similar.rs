//! Find similar elements — AutoScraper-inspired structural matching.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div class="results">
                <div class="result" data-type="organic">
                    <h3><a href="/page1">First Result</a></h3>
                    <p>Description of the first result.</p>
                </div>
                <div class="result" data-type="organic">
                    <h3><a href="/page2">Second Result</a></h3>
                    <p>Description of the second result.</p>
                </div>
                <div class="result" data-type="organic">
                    <h3><a href="/page3">Third Result</a></h3>
                    <p>Description of the third result.</p>
                </div>
                <div class="ad" data-type="sponsored">
                    <h3><a href="/ad">Sponsored</a></h3>
                    <p>This is an ad.</p>
                </div>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // Find the first result
    let first = page.css("div.result").first().unwrap().clone();
    println!(
        "Reference element: {}",
        first.css("h3 a").first().unwrap().text()
    );

    // Find all similar elements (same tag, same depth, similar structure)
    let similar = first.find_similar(None, false, &["href"]);
    println!("Found {} similar elements:", similar.len());

    for el in similar.iter() {
        let title = el.css("h3 a").first().unwrap().text();
        println!("  - {title}");
    }

    // With higher threshold
    let strict = first.find_similar(Some(0.8), false, &["href"]);
    println!("\nWith 0.8 threshold: {} matches", strict.len());
}
