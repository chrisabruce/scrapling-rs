//! Adaptive element relocation via structural similarity scoring.
//!
//! When a page's DOM structure changes (classes renamed, elements moved, etc.),
//! a CSS selector that worked before may return no results. The adaptive engine
//! solves this by comparing the **structural fingerprint** of the previously
//! matched element against every element in the new page, scoring each
//! candidate on multiple dimensions and returning the best match(es).
//!
//! # Scoring algorithm
//!
//! The similarity score is computed across up to 12 equally-weighted factors:
//!
//! | # | Factor | Method |
//! |---|---|---|
//! | 1 | Tag name | exact match (0 or 1) |
//! | 2 | Text content | `SequenceMatcher` ratio |
//! | 3 | Attributes (all) | `dict_diff` (0.5×key_sim + 0.5×value_sim) |
//! | 4 | `class` attribute | `SequenceMatcher` ratio |
//! | 5 | `id` attribute | `SequenceMatcher` ratio |
//! | 6 | `href` attribute | `SequenceMatcher` ratio |
//! | 7 | `src` attribute | `SequenceMatcher` ratio |
//! | 8 | Ancestor path | `SequenceMatcher` ratio on tag chains |
//! | 9 | Parent tag | `SequenceMatcher` ratio |
//! | 10 | Parent attributes | `dict_diff` |
//! | 11 | Parent text | `SequenceMatcher` ratio |
//! | 12 | Sibling tags | `SequenceMatcher` ratio |
//!
//! Final score: `(sum / checks) * 100`, rounded to 2 decimal places.
//!
//! Only factors where the original element has data are counted, so the
//! denominator varies per element.
//!
//! # Example flow
//!
//! ```text
//! 1. User saved element with css("div.price", auto_save=true)
//! 2. Page redesign: "div.price" → "span.cost"
//! 3. css("div.price", adaptive=true) finds no match
//! 4. Engine retrieves stored ElementData for "div.price"
//! 5. Scores every element in the page against the stored fingerprint
//! 6. Returns the best match(es) above the threshold percentage
//! ```

use std::collections::HashMap;

use crate::selector::{Selector, Selectors};
use crate::storage::ElementData;

/// Calculate the similarity score between a stored element fingerprint and
/// a candidate element in the current page.
///
/// Returns a percentage (0.0–100.0) indicating how similar the candidate
/// is to the original element.
pub fn calculate_similarity_score(original: &ElementData, candidate: &ElementData) -> f64 {
    let mut score: f64 = 0.0;
    let mut checks: u32 = 0;

    // 1. Tag name — exact match
    score += if original.tag == candidate.tag {
        1.0
    } else {
        0.0
    };
    checks += 1;

    // 2. Text content — sequence similarity
    if let Some(orig_text) = &original.text {
        let cand_text = candidate.text.as_deref().unwrap_or("");
        score += sequence_ratio(orig_text, cand_text);
        checks += 1;
    }

    // 3. Attributes (all) — dict_diff
    score += dict_diff(&original.attributes, &candidate.attributes);
    checks += 1;

    // 4–7. Specific attributes: class, id, href, src
    for attr in &["class", "id", "href", "src"] {
        if let Some(orig_val) = original.attributes.get(*attr) {
            let cand_val = candidate
                .attributes
                .get(*attr)
                .map(|s| s.as_str())
                .unwrap_or("");
            score += sequence_ratio(orig_val, cand_val);
            checks += 1;
        }
    }

    // 8. Path (ancestor tag chain) — sequence similarity
    score += sequence_ratio_slices(&original.path, &candidate.path);
    checks += 1;

    // 9–11. Parent info
    if let Some(orig_parent) = &original.parent_name {
        if let Some(cand_parent) = &candidate.parent_name {
            // 9. Parent tag
            score += sequence_ratio(orig_parent, cand_parent);
            checks += 1;

            // 10. Parent attributes
            if let (Some(orig_pattribs), Some(cand_pattribs)) =
                (&original.parent_attribs, &candidate.parent_attribs)
            {
                score += dict_diff(orig_pattribs, cand_pattribs);
                checks += 1;
            }

            // 11. Parent text
            if let Some(orig_ptext) = &original.parent_text {
                let cand_ptext = candidate.parent_text.as_deref().unwrap_or("");
                score += sequence_ratio(orig_ptext, cand_ptext);
                checks += 1;
            }
        }
    }

    // 12. Siblings
    if !original.siblings.is_empty() {
        score += sequence_ratio_slices(&original.siblings, &candidate.siblings);
        checks += 1;
    }

    if checks == 0 {
        return 0.0;
    }

    ((score / checks as f64) * 100.0 * 100.0).round() / 100.0
}

/// Relocate an element by finding the best structural match in the page.
///
/// Scores every descendant element in `root` against the stored `original`
/// fingerprint and returns those with the highest score, provided it meets
/// the `min_percentage` threshold.
///
/// # Returns
///
/// A `Selectors` collection of the best-matching elements, or an empty
/// collection if no element meets the threshold.
pub fn relocate(root: &Selector, original: &ElementData, min_percentage: f64) -> Selectors {
    let mut best_score: f64 = 0.0;
    let mut best_elements: Vec<Selector> = Vec::new();

    for candidate in root.descendants() {
        let candidate_data = ElementData::from_selector(&candidate);
        let score = calculate_similarity_score(original, &candidate_data);

        if score > best_score {
            best_score = score;
            best_elements = vec![candidate];
        } else if (score - best_score).abs() < f64::EPSILON {
            best_elements.push(candidate);
        }
    }

    if best_score >= min_percentage {
        Selectors::new(best_elements)
    } else {
        Selectors::empty()
    }
}

// ---------------------------------------------------------------------------
// Similarity helpers
// ---------------------------------------------------------------------------

/// Compute similarity ratio between two strings using `strsim::jaro_winkler`.
///
/// Returns a value in [0.0, 1.0] where 1.0 is an exact match.
///
/// This is the Rust equivalent of Python's `difflib.SequenceMatcher.ratio()`.
/// We use Jaro-Winkler as a fast approximation — it weights prefix matches
/// more heavily, which works well for DOM attribute values.
fn sequence_ratio(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    strsim::jaro_winkler(a, b)
}

/// Compute similarity ratio between two slices of strings.
///
/// Converts each slice to a single space-separated string and compares.
fn sequence_ratio_slices(a: &[String], b: &[String]) -> f64 {
    if a == b {
        return 1.0;
    }
    let a_str = a.join(" ");
    let b_str = b.join(" ");
    sequence_ratio(&a_str, &b_str)
}

/// Compute similarity between two attribute dictionaries.
///
/// Score = 0.5 × (key similarity) + 0.5 × (value similarity).
///
/// Mirrors Python's `__calculate_dict_diff()`.
fn sorted_keys_and_values(map: &HashMap<String, String>) -> (String, String) {
    let mut pairs: Vec<(&str, &str)> = map.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    pairs.sort();
    let keys = pairs.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(" ");
    let vals = pairs.iter().map(|(_, v)| *v).collect::<Vec<_>>().join(" ");
    (keys, vals)
}

fn dict_diff(a: &HashMap<String, String>, b: &HashMap<String, String>) -> f64 {
    let (a_keys, a_vals) = sorted_keys_and_values(a);
    let (b_keys, b_vals) = sorted_keys_and_values(b);

    let key_sim = sequence_ratio(&a_keys, &b_keys);
    let val_sim = sequence_ratio(&a_vals, &b_vals);

    key_sim * 0.5 + val_sim * 0.5
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_data(tag: &str, class: &str, text: Option<&str>) -> ElementData {
        let mut attributes = HashMap::new();
        if !class.is_empty() {
            attributes.insert("class".to_owned(), class.to_owned());
        }
        ElementData {
            tag: tag.to_owned(),
            attributes,
            text: text.map(|s| s.to_owned()),
            path: vec!["html".to_owned(), "body".to_owned(), tag.to_owned()],
            parent_name: Some("body".to_owned()),
            parent_attribs: Some(HashMap::new()),
            parent_text: None,
            siblings: vec![],
            children: vec![],
        }
    }

    #[test]
    fn identical_elements_score_100() {
        let data = make_data("div", "price", Some("$42.99"));
        let score = calculate_similarity_score(&data, &data);
        assert!((score - 100.0).abs() < 0.01, "score was {score}");
    }

    #[test]
    fn completely_different_elements_score_low() {
        let a = make_data("div", "price", Some("$42.99"));
        let b = make_data("span", "title", Some("Hello World"));
        let score = calculate_similarity_score(&a, &b);
        assert!(score < 70.0, "score was {score}");
    }

    #[test]
    fn score_is_symmetric() {
        let a = make_data("div", "price-tag", Some("$10"));
        let b = make_data("div", "cost-label", Some("$20"));
        let s1 = calculate_similarity_score(&a, &b);
        let s2 = calculate_similarity_score(&b, &a);
        assert!((s1 - s2).abs() < 0.01, "scores differ: {s1} vs {s2}");
    }

    #[test]
    fn score_in_range() {
        let a = make_data("div", "a", None);
        let b = make_data("span", "b", Some("text"));
        let score = calculate_similarity_score(&a, &b);
        assert!((0.0..=100.0).contains(&score), "score was {score}");
    }

    #[test]
    fn dict_diff_identical() {
        let a = HashMap::from([("class".to_owned(), "foo".to_owned())]);
        assert!((dict_diff(&a, &a) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dict_diff_empty() {
        let a: HashMap<String, String> = HashMap::new();
        let b: HashMap<String, String> = HashMap::new();
        assert!((dict_diff(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn relocate_finds_renamed_element() {
        let old_html = r#"<html><body><div class="price">$42</div></body></html>"#;
        let old_sel = Selector::from_html(old_html);
        let old_elem = old_sel.css("div.price").first().unwrap().clone();
        let old_data = ElementData::from_selector(&old_elem);

        // Page redesign: class renamed
        let new_html = r#"<html><body><div class="cost">$42</div></body></html>"#;
        let new_sel = Selector::from_html(new_html);

        let found = relocate(&new_sel, &old_data, 0.0);
        assert!(!found.is_empty(), "should find the relocated element");
        assert_eq!(found[0].text().as_ref(), "$42");
    }

    #[test]
    fn relocate_respects_threshold() {
        let data = make_data("div", "price", Some("$42"));

        let html = r#"<html><body><span class="totally-different">other</span></body></html>"#;
        let sel = Selector::from_html(html);

        let found = relocate(&sel, &data, 95.0);
        assert!(found.is_empty(), "should not match at 95% threshold");
    }

    #[test]
    fn element_data_from_selector() {
        let html = r#"<html><body><div id="main"><p class="intro">Hello</p></div></body></html>"#;
        let sel = Selector::from_html(html);
        let p = sel.css("p.intro").first().unwrap().clone();
        let data = ElementData::from_selector(&p);

        assert_eq!(data.tag, "p");
        assert_eq!(data.attributes["class"], "intro");
        assert_eq!(data.text, Some("Hello".to_owned()));
        assert_eq!(data.parent_name, Some("div".to_owned()));
        assert!(data.path.contains(&"p".to_owned()));
        assert!(data.path.contains(&"body".to_owned()));
    }
}
