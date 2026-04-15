//! Low-level text cleaning and collection helpers.
//!
//! These utilities mirror the Python functions in `scrapling/core/utils/_utils.py`
//! and `scrapling/core/custom_types.py`. They are used internally by
//! [`TextHandler`](crate::TextHandler) and the selector engine, and are also
//! public for downstream crates that need the same behaviour.
//!
//! # Two cleaning modes
//!
//! Scrapling's Python codebase has **two** whitespace-cleaning tables that
//! differ in how they treat `\n` and `\r`:
//!
//! | Function | `\t` | `\n` | `\r` | Then |
//! |---|---|---|---|---|
//! | [`clean_spaces`] | → space | **delete** | **delete** | collapse consecutive spaces |
//! | [`clean_whitespace`] | → space | → space | → space | collapse consecutive spaces |
//!
//! [`clean_spaces`] is the general-purpose utility (used by the storage
//! engine and selector text comparison). [`clean_whitespace`] is used by
//! [`TextHandler::clean()`](crate::TextHandler::clean) where newlines are
//! normalised to spaces rather than stripped.

use std::sync::LazyLock;

use regex::Regex;

/// Pre-compiled regex matching one or more consecutive ASCII spaces.
static CONSECUTIVE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" +").unwrap());

/// Controls how `\n` and `\r` are handled during whitespace cleaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NewlineMode {
    /// Delete `\n` and `\r` entirely (used by storage/selector text comparison).
    Delete,
    /// Replace `\n` and `\r` with spaces (used by [`TextHandler::clean()`](crate::TextHandler::clean)).
    ReplaceWithSpace,
}

/// Normalize whitespace: replace `\t` with space, handle `\n`/`\r` per `mode`,
/// then collapse consecutive spaces into one.
fn normalize_whitespace(s: &str, mode: NewlineMode) -> String {
    let mut buf = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\t' => buf.push(' '),
            '\n' | '\r' => match mode {
                NewlineMode::Delete => {}
                NewlineMode::ReplaceWithSpace => buf.push(' '),
            },
            other => buf.push(other),
        }
    }
    CONSECUTIVE_SPACES.replace_all(&buf, " ").into_owned()
}

/// Replace tabs with spaces, **delete** newlines and carriage returns,
/// then collapse runs of consecutive spaces into a single space.
///
/// This mirrors the Python `clean_spaces()` function from
/// `core/utils/_utils.py`, which uses:
///
/// ```python
/// str.maketrans({"\t": " ", "\n": None, "\r": None})
/// ```
///
/// # Examples
///
/// ```
/// use scrapling::utils::clean_spaces;
///
/// assert_eq!(clean_spaces("hello  world"), "hello world");
/// assert_eq!(clean_spaces("a\tb\nc\rd"), "a bcd");   // \n and \r deleted
/// ```
pub fn clean_spaces(s: &str) -> String {
    normalize_whitespace(s, NewlineMode::Delete)
}

/// Replace tabs, newlines, and carriage returns **all with spaces**,
/// then collapse runs of consecutive spaces into a single space.
///
/// This mirrors the cleaning table used by `TextHandler.clean()` in
/// `core/custom_types.py`, which uses:
///
/// ```python
/// str.maketrans("\t\r\n", "   ")
/// ```
///
/// # Difference from [`clean_spaces`]
///
/// [`clean_spaces`] *deletes* `\n` and `\r`; this function *replaces* them
/// with a space. Both collapse consecutive spaces afterward.
///
/// # Examples
///
/// ```
/// use scrapling::utils::clean_whitespace;
///
/// assert_eq!(clean_whitespace("a\tb\nc\rd"), "a b c d");
/// assert_eq!(clean_whitespace("hello\t\tworld\n\nfoo"), "hello world foo");
/// ```
pub fn clean_whitespace(s: &str) -> String {
    normalize_whitespace(s, NewlineMode::ReplaceWithSpace)
}

/// Flatten one level of nesting, collecting all inner items into a single `Vec`.
///
/// This mirrors Python's `flatten()` utility — `list(chain.from_iterable(lst))`.
///
/// # Examples
///
/// ```
/// use scrapling::utils::flatten;
///
/// let nested = vec![vec![1, 2], vec![3], vec![4, 5, 6]];
/// assert_eq!(flatten(nested), vec![1, 2, 3, 4, 5, 6]);
/// ```
pub fn flatten<T>(nested: impl IntoIterator<Item = impl IntoIterator<Item = T>>) -> Vec<T> {
    nested.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_spaces_basic() {
        assert_eq!(clean_spaces("hello  world"), "hello world");
        // \t → space, \n → delete, \r → delete
        assert_eq!(clean_spaces("a\tb\nc\rd"), "a bcd");
        assert_eq!(clean_spaces("  lots   of   space  "), " lots of space ");
    }

    #[test]
    fn clean_spaces_empty() {
        assert_eq!(clean_spaces(""), "");
        // \n and \r are deleted, \t becomes space
        assert_eq!(clean_spaces("\n\r\t"), " ");
    }

    #[test]
    fn clean_whitespace_basic() {
        // \t, \n, \r all become spaces, then collapsed
        assert_eq!(clean_whitespace("a\tb\nc\rd"), "a b c d");
        assert_eq!(clean_whitespace("hello\t\tworld\n\nfoo"), "hello world foo");
    }

    #[test]
    fn clean_spaces_idempotent() {
        let input = "hello\t\tworld\n\nfoo";
        let once = clean_spaces(input);
        let twice = clean_spaces(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn flatten_vecs() {
        let nested = vec![vec![1, 2], vec![3], vec![4, 5, 6]];
        assert_eq!(flatten(nested), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn flatten_empty() {
        let nested: Vec<Vec<i32>> = vec![vec![], vec![]];
        assert_eq!(flatten(nested), Vec::<i32>::new());
    }
}
