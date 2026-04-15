use scrapling::TextHandler;
use scrapling::selector::Selector;

#[test]
fn pseudo_element_text() {
    let html = r##"<html><body><p>Hello <b>world</b></p></body></html>"##;
    let page = Selector::from_html(html);
    let texts = page.css("p::text");
    assert!(!texts.is_empty());
    assert_eq!(texts[0].text().as_ref(), "Hello ");
}

#[test]
fn pseudo_element_attr() {
    let html = r##"<html><body><a href="/about" class="link">About</a></body></html>"##;
    let page = Selector::from_html(html);
    let hrefs = page.css("a::attr(href)");
    assert_eq!(hrefs.len(), 1);
    assert_eq!(hrefs[0].text().as_ref(), "/about");
}

#[test]
fn url_joining() {
    let html = r##"<html><body><a href="/about">About</a></body></html>"##;
    let page = Selector::from_html_with_url(html, "https://example.com/page");
    assert_eq!(page.urljoin("/about"), "https://example.com/about");
    assert_eq!(page.urljoin("sibling"), "https://example.com/sibling");
    assert!(
        page.urljoin("https://other.com")
            .starts_with("https://other.com")
    );
}

#[test]
fn get_all_text_with_separator() {
    let html = r##"<html><body><div><p>First</p><p>Second</p><p>Third</p></div></body></html>"##;
    let page = Selector::from_html(html);
    let divs = page.css("div");
    let div = divs.first().unwrap();
    let text = div.get_all_text(" | ", true, &[], true);
    assert!(text.as_ref().contains("First"));
    assert!(text.as_ref().contains("Second"));
    assert!(text.as_ref().contains("Third"));
}

#[test]
fn get_all_text_ignore_tags() {
    let html = r##"<html><body><div><p>Keep</p><script>ignore()</script><p>Also keep</p></div></body></html>"##;
    let page = Selector::from_html(html);
    let divs = page.css("div");
    let div = divs.first().unwrap();
    let text = div.get_all_text(" ", true, &["script"], true);
    assert!(text.as_ref().contains("Keep"));
    assert!(text.as_ref().contains("Also keep"));
    assert!(!text.as_ref().contains("ignore"));
}

#[test]
fn text_handler_string_operations() {
    let t = TextHandler::new("  Hello World  ".to_owned());
    assert_eq!(t.clean(false).as_ref(), "Hello World");

    let upper = TextHandler::new("hello".to_owned());
    assert_eq!(upper.to_uppercase_text().as_ref(), "HELLO");

    let lower = TextHandler::new("HELLO".to_owned());
    assert_eq!(lower.to_lowercase_text().as_ref(), "hello");
}

#[test]
fn text_handler_regex() {
    let t = TextHandler::new("Price: $42.99 and $15.50".to_owned());
    let matches = t.re(r"\$(\d+\.\d+)", false, false, true).unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].as_ref(), "42.99");
    assert_eq!(matches[1].as_ref(), "15.50");
}

#[test]
fn text_handler_regex_case_insensitive() {
    let t = TextHandler::new("Hello World".to_owned());
    let matches = t.re(r"hello", false, false, false).unwrap();
    assert_eq!(matches.len(), 1);
}

#[test]
fn text_handler_re_first_with_default() {
    let t = TextHandler::new("no numbers here".to_owned());
    let default = TextHandler::new("N/A".to_owned());
    let result = t
        .re_first(r"\d+", Some(default), false, false, true)
        .unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().as_ref(), "N/A");
}

#[test]
fn text_handler_clean_with_entities() {
    let t = TextHandler::new("Hello &amp; World".to_owned());
    let cleaned = t.clean(true);
    assert!(cleaned.as_ref().contains("Hello"));
    assert!(cleaned.as_ref().contains("World"));
}

#[test]
fn text_handler_json() {
    let t = TextHandler::new(r#"{"key": "value"}"#.to_owned());
    let val: serde_json::Value = t.json().unwrap();
    assert_eq!(val["key"], "value");

    let invalid = TextHandler::new("not json".to_owned());
    let result: Result<serde_json::Value, _> = invalid.json();
    assert!(result.is_err());
}

#[test]
fn text_handler_split() {
    let t = TextHandler::new("a,b,c,d".to_owned());
    let parts = t.split_text(",");
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].as_ref(), "a");
    assert_eq!(parts[3].as_ref(), "d");
}

#[test]
fn selectors_properties() {
    let html = r##"<html><body><ul><li>A</li><li>B</li><li>C</li></ul></body></html>"##;
    let page = Selector::from_html(html);
    let items = page.css("li");

    assert_eq!(items.len(), 3);
    assert!(items.first().is_some());
    assert_eq!(items.first().unwrap().text().as_ref(), "A");
    assert!(!items.is_empty());
}

#[test]
fn from_bytes_utf8() {
    let html = b"<html><body><p>Hello</p></body></html>";
    let page = Selector::from_bytes(html);
    let ps = page.css("p");
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].text().as_ref(), "Hello");
}

#[test]
fn from_bytes_with_encoding() {
    let html = b"<html><body><p>Hello</p></body></html>";
    let page = Selector::from_bytes_with_encoding(html, "utf-8");
    let ps = page.css("p");
    assert_eq!(ps.len(), 1);
}
