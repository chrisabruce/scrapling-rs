//! JSON extraction — parse JSON from script tags and attributes.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <script id="app-data" type="application/json">
                {
                    "products": [
                        {"name": "Widget", "price": 9.99, "stock": true},
                        {"name": "Gadget", "price": 24.99, "stock": false},
                        {"name": "Doohickey", "price": 14.99, "stock": true}
                    ],
                    "total": 3
                }
            </script>
            <div data-config='{"apiUrl": "https://api.example.com", "version": "2.1"}'>
                App container
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // Extract JSON from script tag
    let script_text = page.css("#app-data::text");
    let data: serde_json::Value = script_text.first().unwrap().text().json().unwrap();

    println!("Total products: {}", data["total"]);
    println!("\nIn-stock products:");
    for product in data["products"].as_array().unwrap() {
        if product["stock"].as_bool().unwrap_or(false) {
            println!("  {} — ${}", product["name"], product["price"]);
        }
    }

    // Extract JSON from data attribute
    let config: serde_json::Value =
        page.css("[data-config]").first().unwrap().attrib()["data-config"]
            .json()
            .unwrap();

    println!("\nAPI URL: {}", config["apiUrl"]);
    println!("Version: {}", config["version"]);
}
