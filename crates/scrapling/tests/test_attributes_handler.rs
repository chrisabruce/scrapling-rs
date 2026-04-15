use scrapling::selector::Selector;

fn sample_html() -> &'static str {
    r##"
    <html><body>
        <div id="main"
             class="container active"
             data-config='{"theme": "dark", "version": 2.5}'
             data-items='[1, 2, 3, 4, 5]'
             data-invalid-json='{"broken: json}'
             title="Main Container"
             style="color: red; background: blue;"
             data-empty=""
             data-number="42"
             data-bool="true"
             data-url="https://example.com/page?param=value"
             custom-attr="custom-value"
             data-nested='{"user": {"name": "John", "age": 30}}'
             onclick="handleClick()"
             data-null="null">
            Content
        </div>
        <input type="text" name="username" value="test@example.com"
               placeholder="Enter email" required disabled>
        <img src="/images/photo.jpg" alt="Photo" width="100" height="100" loading="lazy">
    </body></html>
    "##
}

fn attribs() -> scrapling::AttributesHandler {
    let page = Selector::from_html(sample_html());
    page.css("#main")[0].attrib().clone()
}

#[test]
fn basic_attribute_access() {
    let a = attribs();
    assert_eq!(a["id"].as_ref(), "main");
    assert_eq!(a["class"].as_ref(), "container active");
    assert_eq!(a["title"].as_ref(), "Main Container");
    assert!(a.get("id").is_some());
    assert!(a.get("nonexistent").is_none());
}

#[test]
fn iteration_methods() {
    let a = attribs();
    let keys: Vec<&str> = a.keys().collect();
    assert!(keys.contains(&"id"));
    assert!(keys.contains(&"class"));
    assert!(keys.contains(&"data-config"));
    assert!(!a.is_empty());
}

#[test]
fn json_parsing() {
    let a = attribs();
    let config: serde_json::Value = a["data-config"].json().unwrap();
    assert_eq!(config["theme"], "dark");
    assert_eq!(config["version"], 2.5);

    let items: Vec<i32> = a["data-items"].json().unwrap();
    assert_eq!(items, vec![1, 2, 3, 4, 5]);

    let nested: serde_json::Value = a["data-nested"].json().unwrap();
    assert_eq!(nested["user"]["name"], "John");
    assert_eq!(nested["user"]["age"], 30);

    let null: serde_json::Value = a["data-null"].json().unwrap();
    assert!(null.is_null());
}

#[test]
fn json_error_handling() {
    let a = attribs();
    let result: Result<serde_json::Value, _> = a["data-invalid-json"].json();
    assert!(result.is_err());
}

#[test]
fn search_values() {
    let a = attribs();

    let exact = a.search_values("main", false);
    assert!(!exact.is_empty());

    let partial = a.search_values("container", true);
    assert!(!partial.is_empty());

    let no_match = a.search_values("nonexistent", false);
    assert!(no_match.is_empty());
}

#[test]
fn special_attribute_types() {
    let page = Selector::from_html(sample_html());

    let input = &page.css("input")[0];
    assert!(input.attrib().get("required").is_some());
    assert!(input.attrib().get("disabled").is_some());

    let main_elem = &page.css("#main")[0];
    assert_eq!(main_elem.attrib()["data-empty"].as_ref(), "");
    assert_eq!(main_elem.attrib()["data-number"].as_ref(), "42");
    assert_eq!(main_elem.attrib()["data-bool"].as_ref(), "true");
}

#[test]
fn string_representation() {
    let a = attribs();
    let s = format!("{a}");
    assert!(!s.is_empty());
    let r = format!("{a:?}");
    assert!(!r.is_empty());
}

#[test]
fn edge_cases() {
    let page = Selector::from_html("<div>Content</div>");
    let div = &page.css("div")[0];
    assert_eq!(div.attrib().len(), 0);
    assert!(div.attrib().get("any").is_none());

    let page = Selector::from_html(sample_html());
    let main_elem = &page.css("#main")[0];
    let attrib = main_elem.attrib();
    let style = attrib["style"].as_ref();
    assert!(style.contains("color: red"));
    assert!(style.contains("background: blue"));
}

#[test]
fn url_attribute() {
    let a = attribs();
    assert_eq!(
        a["data-url"].as_ref(),
        "https://example.com/page?param=value"
    );
}

#[test]
fn comparison_operations() {
    let page = Selector::from_html(sample_html());
    let main_attribs = page.css("#main")[0].attrib().clone();
    let input_attribs = page.css("input")[0].attrib().clone();
    assert_ne!(main_attribs, input_attribs);

    let main_again = page.css("#main")[0].attrib().clone();
    assert_eq!(main_attribs, main_again);
}

#[test]
fn attribute_filtering() {
    let a = attribs();
    let data_attrs: Vec<&str> = a.keys().filter(|k| k.starts_with("data-")).collect();
    assert!(data_attrs.len() > 5);
    assert!(data_attrs.contains(&"data-config"));

    let event_attrs: Vec<&str> = a.keys().filter(|k| k.starts_with("on")).collect();
    assert!(event_attrs.contains(&"onclick"));
}

#[test]
fn performance_with_many_attributes() {
    let attrs_str: String = (0..100)
        .map(|i| format!(r#"data-attr{i}="value{i}""#))
        .collect::<Vec<_>>()
        .join(" ");
    let html = format!(r#"<div id="test" {attrs_str}>Content</div>"#);
    let page = Selector::from_html(&html);
    let a = page.css("#test")[0].attrib();
    assert_eq!(a.len(), 101);

    let results = a.search_values("value50", false);
    assert_eq!(results.len(), 1);
}

#[test]
fn unicode_attributes() {
    let html = r##"
    <div id="unicode-test"
         data-emoji="😀🎉"
         data-chinese="你好世界"
         data-arabic="مرحبا بالعالم"
         data-special="café naïve">
    </div>
    "##;
    let page = Selector::from_html(html);
    let a = page.css("#unicode-test")[0].attrib();

    assert_eq!(a["data-emoji"].as_ref(), "😀🎉");
    assert_eq!(a["data-chinese"].as_ref(), "你好世界");
    assert_eq!(a["data-arabic"].as_ref(), "مرحبا بالعالم");
    assert_eq!(a["data-special"].as_ref(), "café naïve");

    let results = a.search_values("你好", true);
    assert_eq!(results.len(), 1);
}

#[test]
fn malformed_attributes() {
    let cases = [
        r#"<div id="test" class=>Content</div>"#,
        r#"<div id="test" class>Content</div>"#,
        r#"<div id=test class=no-quotes>Content</div>"#,
    ];

    for html in &cases {
        let page = Selector::from_html(html);
        let divs = page.css("div");
        if !divs.is_empty() {
            let _ = divs[0].attrib();
        }
    }
}
