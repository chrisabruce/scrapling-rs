#![no_main]

use libfuzzer_sys::fuzz_target;
use scrapling::selector::Selector;

const SAMPLE_HTML: &str = r#"<html><body><div class="a"><p id="b">text</p><span>more</span></div></body></html>"#;

fuzz_target!(|data: &[u8]| {
    if let Ok(selector) = std::str::from_utf8(data) {
        let page = Selector::from_html(SAMPLE_HTML);
        let _ = page.css(selector);
    }
});
