//! Text extraction — get text content with various options.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <article>
                <h1>Breaking News</h1>
                <p>The <strong>Rust</strong> programming language has released version 2.0.</p>
                <p>Developers are <em>excited</em> about the new features.</p>
                <script>console.log("ignore me")</script>
                <footer>Published 2026-04-15</footer>
            </article>
        </body></html>
    "#;

    let page = Selector::from_html(html);
    let articles = page.css("article");
    let article = articles.first().unwrap();

    // Direct text of the first child
    println!("Title: {}", article.css("h1").first().unwrap().text());

    // All text recursively, ignoring script tags
    let all_text = article.get_all_text(" ", true, &["script"], true);
    println!("\nFull article text:\n{all_text}");

    // Text with custom separator
    let lined = article.get_all_text("\n", true, &["script", "footer"], true);
    println!("\nLine-separated:\n{lined}");
}
