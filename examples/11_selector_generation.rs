//! Selector generation — auto-generate unique CSS/XPath selectors for elements.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div id="main">
                <ul class="nav">
                    <li><a href="/">Home</a></li>
                    <li><a href="/about">About</a></li>
                    <li><a href="/contact">Contact</a></li>
                </ul>
                <div class="content">
                    <p>First paragraph</p>
                    <p>Second paragraph</p>
                </div>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);

    // Generate selectors for each link
    let links = page.css("a");
    println!("Generated selectors for {} links:\n", links.len());

    for link in links.iter() {
        let text = link.text();
        let css = link.generate_css_selector();
        let xpath = link.generate_xpath_selector();
        let full_css = link.generate_full_css_selector();

        println!("'{text}':");
        println!("  CSS:      {css}");
        println!("  XPath:    {xpath}");
        println!("  Full CSS: {full_css}");
        println!();
    }

    // Verify generated selectors work
    let second_p = &page.css(".content p")[1];
    let generated = second_p.generate_css_selector();
    let found = page.css(&generated);
    println!("Generated '{generated}' selects {} element(s)", found.len());
    println!("Text: {}", found[0].text());
}
