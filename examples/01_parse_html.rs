//! Basic HTML parsing — parse a string and access the DOM tree.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html>
        <body>
            <h1>Welcome to Scrapling</h1>
            <p class="intro">A fast web scraping toolkit for Rust.</p>
            <ul>
                <li>Fast HTML parsing</li>
                <li>CSS selector support</li>
                <li>Adaptive element relocation</li>
            </ul>
        </body>
        </html>
    "#;

    let page = Selector::from_html(html);

    println!("Tag: {}", page.tag());
    println!("Children: {}", page.children().len());

    let body = page.css("body").first().unwrap().clone();
    println!("Body has {} children", body.children().len());
}
