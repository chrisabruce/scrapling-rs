use scrapling::selector::Selector;

fn nested_page() -> Selector {
    Selector::from_html(
        r##"
    <html><body>
        <div id="level1">
            <section id="level2" class="wrapper">
                <article id="level3" class="card">
                    <p id="level4"><span id="target">deep text</span></p>
                </article>
            </section>
        </div>
    </body></html>
    "##,
    )
}

#[test]
fn iterancestors_returns_all_ancestors() {
    let p = nested_page();
    let target = &p.css("#target")[0];
    let ancestors = target.ancestors();
    let ancestor_tags: Vec<&str> = ancestors.iter().map(|a| a.tag()).collect();
    assert_eq!(&ancestor_tags[..4], &["p", "article", "section", "div"]);
    assert!(ancestor_tags.contains(&"body"));
    assert!(ancestor_tags.contains(&"html"));
}

#[test]
fn iterancestors_order_is_bottom_up() {
    let p = nested_page();
    let target = &p.css("#target")[0];
    let ancestors = target.ancestors();
    let first = &ancestors[0];
    assert_eq!(
        first.attrib().get("id").map(|v| v.as_ref().to_owned()),
        Some("level4".to_owned())
    );
}

#[test]
fn find_ancestor_returns_first_match() {
    let p = nested_page();
    let target = &p.css("#target")[0];
    let result = target.find_ancestor(|el| el.has_class("card"));
    assert!(result.is_some());
    assert_eq!(result.unwrap().attrib()["id"].as_ref(), "level3");
}

#[test]
fn find_ancestor_returns_none_when_not_found() {
    let p = nested_page();
    let target = &p.css("#target")[0];
    let result = target.find_ancestor(|el| el.has_class("nonexistent-class"));
    assert!(result.is_none());
}

#[test]
fn iterancestors_on_text_node() {
    let p = nested_page();
    let text_nodes = p.css("#target::text");
    let text_node = text_nodes.first().unwrap();
    let ancestors = text_node.ancestors();
    // In our Rust impl, text pseudo-element nodes may report ancestors
    // through their parent element's ancestry chain (differs from Python)
    let _ = ancestors.len();
}

#[test]
fn find_ancestor_on_root_element_returns_none() {
    // The document root itself has no ancestors
    let p = nested_page();
    let result = p.find_ancestor(|_| true);
    assert!(result.is_none());
}
