#![no_main]

use libfuzzer_sys::fuzz_target;
use scrapling::selector::Selector;

fuzz_target!(|data: &[u8]| {
    if let Ok(html) = std::str::from_utf8(data) {
        let sel = Selector::from_html(html);
        let _ = sel.css("*");
        let _ = sel.text();
        let _ = sel.get_all_text(" ", true, &[], true);
    }
});
