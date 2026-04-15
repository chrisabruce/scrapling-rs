//! DOM navigation — traverse parent, children, siblings, ancestors.

use scrapling::selector::Selector;

fn main() {
    let html = r#"
        <html><body>
            <div id="container">
                <header><h1>Site Title</h1></header>
                <nav>
                    <a href="/">Home</a>
                    <a href="/about">About</a>
                    <a href="/contact">Contact</a>
                </nav>
                <main>
                    <article id="post">
                        <p>First paragraph</p>
                        <p>Second paragraph</p>
                    </article>
                </main>
            </div>
        </body></html>
    "#;

    let page = Selector::from_html(html);
    let article = &page.css("#post")[0];

    // Parent
    let parent = article.parent().unwrap();
    println!("Parent tag: {}", parent.tag());

    // Children
    let children = article.children();
    println!("Children: {} paragraphs", children.len());

    // Siblings
    let nav = &page.css("nav")[0];
    let siblings = nav.siblings();
    println!(
        "Nav siblings: {:?}",
        siblings.iter().map(|s| s.tag()).collect::<Vec<_>>()
    );

    // Next / Previous
    let links = page.css("a");
    let second_link = &links[1];
    println!("\nSecond link: {}", second_link.text());
    println!("Previous: {}", second_link.previous().unwrap().text());
    println!("Next: {}", second_link.next().unwrap().text());

    // Ancestors
    let p = &article.css("p")[0];
    let ancestors: Vec<String> = p.ancestors().iter().map(|a| a.tag().to_owned()).collect();
    println!("\nAncestors of <p>: {ancestors:?}");

    // Find ancestor by predicate
    let container = p.find_ancestor(|el| {
        el.attrib()
            .get("id")
            .is_some_and(|v| v.as_ref() == "container")
    });
    println!("Found container: {}", container.is_some());
}
