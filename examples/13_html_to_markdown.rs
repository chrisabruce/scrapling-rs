//! HTML to Markdown/Text conversion — clean content extraction.

use scrapling::selector::Selector;
use scrapling::shell::Convertor;

fn main() {
    let html = r#"
        <html><body>
            <article>
                <h1>Getting Started with Rust</h1>
                <p>Rust is a <strong>systems programming language</strong> focused on
                <em>safety</em>, <em>speed</em>, and <em>concurrency</em>.</p>

                <h2>Installation</h2>
                <p>Install via <a href="https://rustup.rs">rustup</a>:</p>
                <pre><code>curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></pre>

                <h2>Hello World</h2>
                <pre><code>fn main() {
    println!("Hello, world!");
}</code></pre>

                <script>analytics.track('page_view')</script>
                <style>.hidden { display: none; }</style>
            </article>
        </body></html>
    "#;

    // Convert to Markdown (strips script/style/noscript/svg)
    let markdown = Convertor::to_markdown(html);
    println!("=== MARKDOWN ===\n{markdown}");

    // Convert to plain text
    let text = Convertor::to_text(html);
    println!("\n=== PLAIN TEXT ===\n{text}");

    // Use with Selector for targeted extraction
    let page = Selector::from_html(html);
    let articles = page.css("article");
    let article = articles.first().unwrap();
    let article_html = article.html_content().into_inner();
    let article_md = Convertor::to_markdown(&article_html);
    println!("\n=== ARTICLE ONLY (MD) ===\n{article_md}");
}
