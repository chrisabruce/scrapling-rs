use scrapling::selector::Selector;

fn product_page() -> Selector {
    Selector::from_html(
        r##"
    <html><body>
        <div class="products">
            <div class="product" data-category="electronics" data-price="100">
                <h3>Laptop</h3>
                <p>A nice laptop</p>
            </div>
            <div class="product" data-category="electronics" data-price="50">
                <h3>Mouse</h3>
                <p>A wireless mouse</p>
            </div>
            <div class="product" data-category="clothing" data-price="30">
                <h3>T-Shirt</h3>
                <p>A cotton t-shirt</p>
            </div>
            <section class="product" data-category="books" data-price="15">
                <h3>Novel</h3>
                <p>A fiction novel</p>
            </section>
        </div>
    </body></html>
    "##,
    )
}

#[test]
fn find_similar_default_finds_same_tag_siblings() {
    let p = product_page();
    let first = p.css("div.product").first().unwrap().clone();
    let similar = first.find_similar(None, false, &["href", "src"]);
    assert_eq!(similar.len(), 2);
    for s in similar.iter() {
        assert_eq!(s.tag(), "div");
    }
}

#[test]
fn find_similar_high_threshold_filters_more() {
    let p = product_page();
    let first = p.css("div.product").first().unwrap().clone();
    let low = first.find_similar(Some(0.1), false, &["href", "src"]);
    let high = first.find_similar(Some(0.8), false, &["href", "src"]);
    assert!(high.len() <= low.len());
}

#[test]
fn find_similar_match_text_affects_scoring() {
    let p = product_page();
    let first = p.css("div.product").first().unwrap().clone();
    let without_text = first.find_similar(Some(0.5), false, &["href", "src"]);
    let with_text = first.find_similar(Some(0.5), true, &["href", "src"]);
    // With text matching the threshold is harder to reach since text differs
    assert!(with_text.len() <= without_text.len());
}

#[test]
fn find_similar_on_text_node_returns_empty() {
    let p = product_page();
    let text_nodes = p.css("h3::text");
    let text_node = text_nodes.first().unwrap().clone();
    let similar = text_node.find_similar(None, false, &[]);
    assert_eq!(similar.len(), 0);
}

// ---------------------------------------------------------------------------
// find_all / find tests
// ---------------------------------------------------------------------------

fn nav_page() -> Selector {
    Selector::from_html(
        r##"
    <html><body>
        <nav>
            <a href="/home" class="link active">Home</a>
            <a href="/about" class="link">About</a>
            <a href="/contact" class="link">Contact</a>
            <button class="btn">Menu</button>
        </nav>
        <main>
            <div class="card" data-type="info">
                <h2>Title</h2>
                <p>Content here</p>
            </div>
            <div class="card" data-type="warning">
                <h2>Warning</h2>
                <p>Be careful</p>
            </div>
        </main>
    </body></html>
    "##,
    )
}

#[test]
fn find_all_by_tag() {
    let p = nav_page();
    let links = p.find_all(&["a"], &[], &[], &[]);
    assert_eq!(links.len(), 3);
}

#[test]
fn find_all_by_tag_and_attributes() {
    let p = nav_page();
    let cards = p.find_all(&["div"], &[("class", "card")], &[], &[]);
    assert_eq!(cards.len(), 2);
}

#[test]
fn find_all_by_attribute_only() {
    let p = nav_page();
    let warnings = p.find_all(&[], &[("data-type", "warning")], &[], &[]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].css("h2").first().unwrap().text().as_ref(),
        "Warning"
    );
}

#[test]
fn find_all_by_regex_pattern() {
    let p = nav_page();
    let matches = p.find_all(&["p"], &[], &[r"(?i)careful"], &[]);
    assert_eq!(matches.len(), 1);
}

#[test]
fn find_all_by_predicate() {
    let p = nav_page();
    let active: &dyn Fn(&Selector) -> bool = &|el: &Selector| el.has_class("active");
    let results = p.find_all(&["a"], &[], &[], &[active]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text().as_ref(), "Home");
}

#[test]
fn find_all_multiple_tags() {
    let p = nav_page();
    let results = p.find_all(&["a", "button"], &[], &[], &[]);
    assert_eq!(results.len(), 4);
}

#[test]
fn find_all_empty_args_returns_empty() {
    let p = nav_page();
    let results = p.find_all(&[], &[], &[], &[]);
    assert_eq!(results.len(), 0);
}

#[test]
fn find_returns_first_match() {
    let p = nav_page();
    let result = p.find(&["a"], &[], &[], &[]);
    assert!(result.is_some());
    assert_eq!(result.unwrap().text().as_ref(), "Home");
}

#[test]
fn find_returns_none_when_no_match() {
    let p = nav_page();
    let result = p.find(&["table"], &[], &[], &[]);
    assert!(result.is_none());
}
