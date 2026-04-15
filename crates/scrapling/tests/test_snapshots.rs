use scrapling::selector::Selector;

#[test]
fn snapshot_css_selection() {
    let html = r##"
    <html><body>
        <div class="products">
            <div class="product" data-id="1"><h3>Widget</h3><span class="price">$10</span></div>
            <div class="product" data-id="2"><h3>Gadget</h3><span class="price">$20</span></div>
        </div>
    </body></html>
    "##;
    let page = Selector::from_html(html);
    let products = page.css(".product");

    let result: Vec<String> = products
        .iter()
        .map(|p| {
            format!(
                "id={} name={} price={}",
                p.attrib()["data-id"].as_ref(),
                p.css("h3").first().unwrap().text().as_ref(),
                p.css(".price").first().unwrap().text().as_ref(),
            )
        })
        .collect();

    insta::assert_yaml_snapshot!(result);
}

#[test]
fn snapshot_selector_generation() {
    let html = r##"<html><body><div id="main"><ul><li class="item">A</li><li class="item">B</li></ul></div></body></html>"##;
    let page = Selector::from_html(html);
    let items = page.css("li.item");

    let selectors: Vec<String> = items.iter().map(|el| el.generate_css_selector()).collect();

    insta::assert_yaml_snapshot!(selectors);
}

#[test]
fn snapshot_text_extraction() {
    let html = r##"
    <html><body>
        <article>
            <h1>Title</h1>
            <p>First paragraph with <strong>bold</strong> text.</p>
            <p>Second paragraph.</p>
        </article>
    </body></html>
    "##;
    let page = Selector::from_html(html);
    let articles = page.css("article");
    let article = articles.first().unwrap();
    let text = article.get_all_text(" ", true, &[], true);

    insta::assert_snapshot!(text.as_ref());
}
