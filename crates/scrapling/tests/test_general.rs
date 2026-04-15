use scrapling::selector::{Selector, Selectors};

fn html_content() -> &'static str {
    r##"
    <html>
    <head>
        <title>Complex Web Page</title>
        <style>
            .hidden { display: none; }
        </style>
    </head>
    <body>
        <header>
            <nav>
                <ul>
                    <li><a href="#home">Home</a></li>
                    <li><a href="#about">About</a></li>
                    <li><a href="#contact">Contact</a></li>
                </ul>
            </nav>
        </header>
        <main>
            <section id="products" schema='{"jsonable": "data"}'>
                <h2>Products</h2>
                <div class="product-list">
                    <article class="product" data-id="1">
                        <h3>Product 1</h3>
                        <p class="description">This is product 1</p>
                        <span class="price">$10.99</span>
                        <div class="hidden stock">In stock: 5</div>
                    </article>
                    <article class="product" data-id="2">
                        <h3>Product 2</h3>
                        <p class="description">This is product 2</p>
                        <span class="price">$20.99</span>
                        <div class="hidden stock">In stock: 3</div>
                    </article>
                    <article class="product" data-id="3">
                        <h3>Product 3</h3>
                        <p class="description">This is product 3</p>
                        <span class="price">$15.99</span>
                        <div class="hidden stock">Out of stock</div>
                    </article>
                </div>
            </section>
            <section id="reviews">
                <h2>Customer Reviews</h2>
                <div class="review-list">
                    <div class="review" data-rating="5">
                        <p class="review-text">Great product!</p>
                        <span class="reviewer">John Doe</span>
                    </div>
                    <div class="review" data-rating="4">
                        <p class="review-text">Good value for money.</p>
                        <span class="reviewer">Jane Smith</span>
                    </div>
                </div>
            </section>
        </main>
        <footer>
            <p>&copy; 2024 Our Company</p>
        </footer>
        <script id="page-data" type="application/json">
            {"lastUpdated": "2024-09-22T10:30:00Z", "totalProducts": 3}
        </script>
    </body>
    </html>
    "##
}

fn page() -> Selector {
    Selector::from_html(html_content())
}

// ---------------------------------------------------------------------------
// CSS Selector Tests
// ---------------------------------------------------------------------------

#[test]
fn css_basic_product_selection() {
    let p = page();
    let elements = p.css("main #products .product-list article.product");
    assert_eq!(elements.len(), 3);
}

// ---------------------------------------------------------------------------
// Text Matching Tests
// ---------------------------------------------------------------------------

#[test]
fn text_regex_multiple_matches() {
    let p = page();
    let stock_info = p.find_by_regex(r"In stock: \d+", true, false).unwrap();
    assert_eq!(stock_info.len(), 2);
}

#[test]
fn text_regex_first_match() {
    let p = page();
    let stock_info = p.find_by_regex(r"In stock: \d+", true, false).unwrap();
    assert_eq!(stock_info.first().unwrap().text().as_ref(), "In stock: 5");
}

#[test]
fn text_partial_match() {
    let p = page();
    let stock_info = p.find_by_text("In stock:", true, false, false);
    assert_eq!(stock_info.len(), 2);
}

#[test]
fn text_exact_match() {
    let p = page();
    let out_of_stock = p.find_by_text("Out of stock", false, false, false);
    assert_eq!(out_of_stock.len(), 1);
}

// ---------------------------------------------------------------------------
// Error Handling Tests
// ---------------------------------------------------------------------------

#[test]
fn css_bad_selector_returns_empty() {
    let p = page();
    let result = p.css("4 ayo");
    assert_eq!(result.len(), 0);
}

// ---------------------------------------------------------------------------
// String Representation Tests
// ---------------------------------------------------------------------------

#[test]
fn string_representations() {
    let p = page();
    let table = &p.css(".product-list")[0];
    let display = format!("{table}");
    assert!(!display.is_empty());
    let debug = format!("{table:?}");
    assert!(!debug.is_empty());
    let attrib_display = format!("{}", table.attrib());
    assert!(!attrib_display.is_empty());
}

// ---------------------------------------------------------------------------
// Navigation Tests
// ---------------------------------------------------------------------------

#[test]
fn basic_navigation_properties() {
    let p = page();
    let table = &p.css(".product-list")[0];
    assert!(!table.path().is_empty());
    assert!(!table.html_content().as_ref().is_empty());
}

#[test]
fn parent_and_sibling_navigation() {
    let p = page();
    let table = &p.css(".product-list")[0];
    let parent = table.parent().unwrap();
    assert_eq!(parent.attrib()["id"].as_ref(), "products");

    let parent_siblings = parent.siblings();
    assert_eq!(parent_siblings.len(), 1);
}

#[test]
fn child_navigation() {
    let p = page();
    let table = &p.css(".product-list")[0];
    let children = table.children();
    assert_eq!(children.len(), 3);
}

#[test]
fn next_and_previous_navigation() {
    let p = page();
    let products = p.css(".product");
    let first = &products[0];
    let next = first.next().unwrap();
    assert_eq!(next.attrib()["data-id"].as_ref(), "2");

    let prev = next.previous().unwrap();
    assert_eq!(prev.tag(), first.tag());
}

#[test]
fn ancestor_finding() {
    let p = page();
    let all_prices = p.css(".price");
    let products_with_prices: Vec<_> = all_prices
        .iter()
        .filter_map(|price| price.find_ancestor(|a| a.has_class("product")))
        .collect();
    assert_eq!(products_with_prices.len(), 3);
}

// ---------------------------------------------------------------------------
// JSON and Attribute Tests
// ---------------------------------------------------------------------------

#[test]
fn json_conversion() {
    let p = page();
    let texts = p.css("#page-data::text");
    let script_content = texts.first().unwrap().text();
    let page_data: serde_json::Value = script_content.json().unwrap();
    assert_eq!(page_data["totalProducts"], 3);
    assert!(page_data.get("lastUpdated").is_some());
}

#[test]
fn attribute_operations() {
    let p = page();
    let products = p.css(".product");
    let product_ids: Vec<String> = products
        .iter()
        .map(|prod| prod.attrib()["data-id"].as_ref().to_owned())
        .collect();
    assert_eq!(product_ids, vec!["1", "2", "3"]);

    assert!(products[0].attrib().get("data-id").is_some());
}

#[test]
fn review_rating_calculations() {
    let p = page();
    let reviews = p.css(".review");
    let ratings: Vec<f64> = reviews
        .iter()
        .map(|r| r.attrib()["data-rating"].as_ref().parse::<f64>().unwrap())
        .collect();
    let avg = ratings.iter().sum::<f64>() / ratings.len() as f64;
    assert!((avg - 4.5).abs() < f64::EPSILON);
}

#[test]
fn attribute_search_values() {
    let p = page();
    let products = p.css(".product");
    let matches = products[0].attrib().search_values("1", false);
    assert!(!matches.is_empty());
}

#[test]
fn json_attribute() {
    let p = page();
    let sections = p.css("#products");
    let section = sections.first().unwrap();
    let attr_json: serde_json::Value = section.attrib()["schema"].json().unwrap();
    assert_eq!(attr_json, serde_json::json!({"jsonable": "data"}));
}

// ---------------------------------------------------------------------------
// Performance Test
// ---------------------------------------------------------------------------

#[test]
fn large_html_parsing_performance() {
    let mut html = String::from("<html><body>");
    for _ in 0..5000 {
        html.push_str(r#"<div class="item"></div>"#);
    }
    html.push_str("</body></html>");

    let start = std::time::Instant::now();
    let parsed = Selector::from_html(&html);
    let elements = parsed.css(".item");
    let elapsed = start.elapsed();

    assert_eq!(elements.len(), 5000);
    assert!(
        elapsed.as_secs_f64() < 0.5,
        "parsing took {elapsed:?}, expected < 500ms"
    );
}

// ---------------------------------------------------------------------------
// Selector Generation Tests
// ---------------------------------------------------------------------------

#[test]
fn selector_generation_traversal() {
    let p = page();
    fn traverse(element: &Selector) {
        // Only test elements with a real tag (skip document root)
        if element.tag() != "[document]" {
            let css = element.generate_css_selector();
            let xpath = element.generate_xpath_selector();
            assert!(
                !css.is_empty() || !xpath.is_empty(),
                "empty selector for tag={}",
                element.tag()
            );
        }
        for child in element.children().iter() {
            traverse(child);
        }
    }
    // Start from children of root to skip document node
    for child in p.children().iter() {
        traverse(child);
    }
}

#[test]
fn full_path_selector_no_duplicate_ids() {
    let html = r##"<html><body><div id="main"><p id="target">Hello</p></div></body></html>"##;
    let p = Selector::from_html(html);
    let targets = p.css("#target");
    let target = targets.first().unwrap();

    let css_full = target.generate_full_css_selector();
    assert_eq!(
        css_full.matches("#target").count(),
        1,
        "duplicate #target in CSS full path: {css_full}"
    );
    assert_eq!(
        css_full.matches("#main").count(),
        1,
        "duplicate #main in CSS full path: {css_full}"
    );

    let result = p.css(&css_full);
    assert_eq!(result.len(), 1);
    assert_eq!(result.first().unwrap().text().as_ref(), "Hello");
}

#[test]
fn full_path_selector_mixed_id_and_no_id() {
    let html =
        r##"<html><body><div id="wrapper"><section><p>Text</p></section></div></body></html>"##;
    let p = Selector::from_html(html);
    let ps = p.css("p");
    let target = ps.first().unwrap();

    let css_full = target.generate_full_css_selector();
    assert!(css_full.contains("#wrapper"));
    assert_eq!(css_full.matches("#wrapper").count(), 1);

    let result = p.css(&css_full);
    assert_eq!(result.len(), 1);
    assert_eq!(result.first().unwrap().text().as_ref(), "Text");
}

// ---------------------------------------------------------------------------
// Text Extraction Tests
// ---------------------------------------------------------------------------

#[test]
fn getting_all_text() {
    let p = page();
    let all_text = p.get_all_text(" ", true, &[], true);
    assert!(!all_text.as_ref().is_empty());
}

#[test]
fn regex_on_text() {
    let p = page();
    let prices = p.css(r#"[data-id="1"] .price"#);
    let element = &prices[0];
    let m = element
        .re_first(r"[\.\d]+", None, false, false, true)
        .unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().as_ref(), "10.99");

    let matches = element.text().re(r"(\d+)", false, false, true).unwrap();
    assert_eq!(matches.len(), 2);
}

// ---------------------------------------------------------------------------
// Selectors.filter() Tests (from test_selectors_filter.py)
// ---------------------------------------------------------------------------

#[test]
fn filter_basic() {
    let html = r##"
    <html><body>
        <ul>
            <li class="item" data-value="10">Apple</li>
            <li class="item" data-value="5">Banana</li>
            <li class="item" data-value="20">Cherry</li>
            <li class="item disabled" data-value="0">Durian</li>
        </ul>
    </body></html>
    "##;
    let p = Selector::from_html(html);
    let items = p.css("li.item");
    let expensive = items.filter(|el| {
        el.attrib()
            .get("data-value")
            .and_then(|v| v.as_ref().parse::<i32>().ok())
            .unwrap_or(0)
            >= 10
    });
    assert_eq!(expensive.len(), 2);
}

#[test]
fn filter_returns_empty_selectors_when_no_match() {
    let html = r##"
    <html><body>
        <ul>
            <li class="item" data-value="10">Apple</li>
        </ul>
    </body></html>
    "##;
    let p = Selector::from_html(html);
    let items = p.css("li.item");
    let result = items.filter(|el| {
        el.attrib()
            .get("data-value")
            .and_then(|v| v.as_ref().parse::<i32>().ok())
            .unwrap_or(0)
            > 9999
    });
    assert_eq!(result.len(), 0);
    assert!(result.first().is_none());
}

#[test]
fn filter_all_pass() {
    let html = r##"
    <html><body>
        <ul>
            <li class="item">A</li>
            <li class="item">B</li>
        </ul>
    </body></html>
    "##;
    let p = Selector::from_html(html);
    let items = p.css("li.item");
    let result = items.filter(|_| true);
    assert_eq!(result.len(), items.len());
}

#[test]
fn filter_chained() {
    let html = r##"
    <html><body>
        <ul>
            <li class="item" data-value="10">Apple</li>
            <li class="item" data-value="5">Banana</li>
            <li class="item" data-value="20">Cherry</li>
            <li class="item disabled" data-value="0">Durian</li>
        </ul>
    </body></html>
    "##;
    let p = Selector::from_html(html);
    let items = p.css("li.item");
    let result = items
        .filter(|el| {
            el.attrib()
                .get("data-value")
                .and_then(|v| v.as_ref().parse::<i32>().ok())
                .unwrap_or(0)
                > 0
        })
        .filter(|el| !el.has_class("disabled"));
    assert_eq!(result.len(), 3);
}

#[test]
fn filter_on_empty_selectors() {
    let empty = Selectors::empty();
    let result = empty.filter(|_| true);
    assert_eq!(result.len(), 0);
}
