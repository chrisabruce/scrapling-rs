# Migrating from BeautifulSoup

If you're coming from Python's BeautifulSoup, scrapling-rs will feel different but more powerful. This guide maps common BeautifulSoup operations to their scrapling-rs equivalents.

## Parsing HTML

### BeautifulSoup
```python
from bs4 import BeautifulSoup
soup = BeautifulSoup(html, "html.parser")
```

### scrapling-rs
```rust
use scrapling::selector::Selector;
let page = Selector::from_html(html);
```

## Finding elements

| BeautifulSoup | scrapling-rs |
|---------------|-------------|
| `soup.find("h1")` | `page.css("h1").first()` |
| `soup.find_all("p")` | `page.css("p")` |
| `soup.find("div", class_="item")` | `page.css("div.item")` |
| `soup.find("a", {"href": "/about"})` | `page.css(r#"a[href="/about"]"#)` |
| `soup.find("div", id="main")` | `page.css("#main")` |
| `soup.select("ul > li.active")` | `page.css("ul > li.active")` |

## Text extraction

### BeautifulSoup
```python
element.string           # direct text only
element.get_text()        # all text recursively
element.get_text(" ")     # with separator
element.get_text(strip=True)
```

### scrapling-rs
```rust
element.text()                              // direct text
element.get_all_text(" ", true, &[], true)  // all text, stripped
element.get_all_text(" | ", true, &["script"], true)  // custom separator, ignore script tags
```

## Attributes

### BeautifulSoup
```python
element["href"]
element.get("class", [])
element.attrs
```

### scrapling-rs
```rust
element.attrib()["href"]
element.attrib().get("class")
element.attrib().iter()
```

## Navigation

| BeautifulSoup | scrapling-rs |
|---------------|-------------|
| `element.parent` | `element.parent()` |
| `element.children` | `element.children()` |
| `element.next_sibling` | `element.next()` |
| `element.previous_sibling` | `element.previous()` |
| `element.find_parent("div")` | `element.find_ancestor(\|e\| e.tag() == "div")` |

## What scrapling-rs adds

Things you get that BeautifulSoup doesn't have:

- **Adaptive element relocation** — find elements even after a site redesign
- **Browser impersonation** — HTTP requests that look like real Chrome/Firefox
- **Headless browser** — render JavaScript, solve Cloudflare challenges
- **Spider framework** — concurrent crawling with checkpointing
- **Performance** — orders of magnitude faster than BeautifulSoup

## Common patterns

### Extract all links
```rust
let links: Vec<String> = page.css("a::attr(href)")
    .iter()
    .map(|a| a.text().into_inner())
    .collect();
```

### Extract a table
```rust
for row in page.css("table tr").iter() {
    let cells: Vec<String> = row.css("td")
        .iter()
        .map(|td| td.text().into_inner())
        .collect();
    println!("{:?}", cells);
}
```

### Find by text content
```rust
// BeautifulSoup: soup.find("p", string="hello")
let matches = page.find_by_text("hello", false, false, false);
```
