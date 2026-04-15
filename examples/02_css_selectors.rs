//! CSS selectors — select elements with full CSS selector syntax.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div class="products">
                <div class="product" data-id="1"><h3>Laptop</h3><span class="price">$999</span></div>
                <div class="product" data-id="2"><h3>Mouse</h3><span class="price">$29</span></div>
                <div class="product" data-id="3"><h3>Keyboard</h3><span class="price">$79</span></div>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // Select all products
    let products = page.css("div.product");
    println!("Found {} products", products.len());

    // Extract names and prices
    for product in products.iter() {
        let name = product.css("h3").first().unwrap().text();
        let price = product.css(".price").first().unwrap().text();
        let id = &product.attrib()["data-id"];
        println!("  #{id}: {name} — {price}");
    }

    // Pseudo-elements: extract text directly
    let prices = page.css(".price::text");
    println!(
        "\nAll prices: {:?}",
        prices
            .iter()
            .map(|p| p.text().into_inner())
            .collect::<Vec<_>>()
    );

    // Pseudo-elements: extract attribute values
    let ids = page.css(".product::attr(data-id)");
    println!(
        "All IDs: {:?}",
        ids.iter()
            .map(|i| i.text().into_inner())
            .collect::<Vec<_>>()
    );
}
