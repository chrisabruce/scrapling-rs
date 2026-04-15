//! Regex extraction — extract patterns from text using TextHandler.

use scrapling::TextHandler;
use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div class="listing">
                <p>Price: $42.99 (was $59.99)</p>
                <p>SKU: ABC-12345-XYZ</p>
                <p>Email: support@example.com</p>
                <p>Phone: (555) 123-4567</p>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // Extract prices with capture groups
    let price_text = page.css(".listing p").first().unwrap().text();
    let prices = price_text.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
    println!(
        "Prices found: {:?}",
        prices.iter().map(|p| p.as_ref()).collect::<Vec<_>>()
    );

    // Extract first match
    let sku_text = page.css(".listing p:nth-child(2)");
    let sku = sku_text[0]
        .re_first(r"[A-Z]+-\d+-[A-Z]+", None, false, false, true)
        .unwrap();
    println!("SKU: {}", sku.unwrap());

    // Case-insensitive regex
    let email = TextHandler::new("Contact: Support@Example.COM");
    let matches = email
        .re(r"[a-z]+@[a-z]+\.[a-z]+", false, false, false)
        .unwrap();
    println!("Email: {}", matches[0]);

    // Find elements by regex pattern
    let phone_elements = page.find_by_regex(r"\(\d{3}\)", true, false).unwrap();
    println!("Elements with phone pattern: {}", phone_elements.len());

    // TextHandler string operations
    let t = TextHandler::new("  Hello, World!  ");
    println!("\nOriginal: '{t}'");
    println!("Cleaned: '{}'", t.clean(false));
    println!("Upper: '{}'", t.to_uppercase_text());
    println!(
        "Split: {:?}",
        t.clean(false)
            .split_text(" ")
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
    );
}
