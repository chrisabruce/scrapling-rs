use criterion::{Criterion, black_box, criterion_group, criterion_main};
use scrapling::selector::Selector;

fn bench_parse_small(c: &mut Criterion) {
    let html = r#"<html><body><div class="item"><h3>Title</h3><p>Content</p></div></body></html>"#;
    c.bench_function("parse_small_html", |b| {
        b.iter(|| Selector::from_html(black_box(html)))
    });
}

fn bench_parse_large(c: &mut Criterion) {
    let mut html = String::from("<html><body>");
    for i in 0..5000 {
        html.push_str(&format!(
            r#"<div class="item" data-id="{i}"><span>Item {i}</span></div>"#
        ));
    }
    html.push_str("</body></html>");

    c.bench_function("parse_5000_elements", |b| {
        b.iter(|| Selector::from_html(black_box(&html)))
    });
}

fn bench_css_select(c: &mut Criterion) {
    let mut html = String::from("<html><body>");
    for i in 0..1000 {
        html.push_str(&format!(
            r#"<div class="item" data-id="{i}"><span>Item {i}</span></div>"#
        ));
    }
    html.push_str("</body></html>");
    let page = Selector::from_html(&html);

    c.bench_function("css_select_1000_items", |b| {
        b.iter(|| page.css(black_box(".item")))
    });
}

fn bench_text_extraction(c: &mut Criterion) {
    let mut html = String::from("<html><body>");
    for i in 0..500 {
        html.push_str(&format!(
            "<p>Paragraph {i} with some text content here.</p>"
        ));
    }
    html.push_str("</body></html>");
    let page = Selector::from_html(&html);

    c.bench_function("get_all_text_500_paragraphs", |b| {
        b.iter(|| page.get_all_text(" ", true, &[], true))
    });
}

fn bench_adaptive_similarity(c: &mut Criterion) {
    let html = r#"<html><body><div class="product" id="p1"><h3>Widget</h3><span class="price">$10</span></div></body></html>"#;
    let page = Selector::from_html(html);
    let target = page.css("#p1").first().unwrap().clone();
    let data = scrapling::storage::ElementData::from_selector(&target);

    let html2 = r#"<html><body><div class="product-card" data-id="p1"><h3>Widget</h3><span class="cost">$10</span></div></body></html>"#;
    let page2 = Selector::from_html(html2);

    c.bench_function("adaptive_relocate", |b| {
        b.iter(|| page2.relocate(black_box(&data), 0.0))
    });
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_large,
    bench_css_select,
    bench_text_extraction,
    bench_adaptive_similarity,
);
criterion_main!(benches);
