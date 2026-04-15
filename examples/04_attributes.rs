//! Attributes — access, search, and parse element attributes.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div id="main"
                 class="container active"
                 data-config='{"theme":"dark","version":2}'
                 data-count="42"
                 style="color: red; font-size: 14px;">
                Content
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);
    let div = &page.css("#main")[0];
    let attrs = div.attrib();

    // Direct access
    println!("id: {}", attrs["id"]);
    println!("class: {}", attrs["class"]);

    // Check existence
    println!("has data-config: {}", attrs.get("data-config").is_some());
    println!("has nonexistent: {}", attrs.get("nonexistent").is_some());

    // Parse JSON from attribute
    let config: serde_json::Value = attrs["data-config"].json().unwrap();
    println!("theme: {}", config["theme"]);
    println!("version: {}", config["version"]);

    // Search values
    let matches = attrs.search_values("container", true);
    println!("\nAttributes containing 'container': {}", matches.len());

    // Iterate all
    println!("\nAll attributes:");
    for (key, value) in attrs.iter() {
        println!("  {key} = {value}");
    }

    // Filter data-* attributes
    let data_attrs: Vec<&str> = attrs.keys().filter(|k| k.starts_with("data-")).collect();
    println!("\ndata-* attributes: {data_attrs:?}");
}
