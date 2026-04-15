//! Python bindings for the scrapling-rs core parsing engine.
//!
//! This crate builds a native Python extension module (`.so` / `.pyd`) using
//! PyO3. It exposes the core `Selector` and `Selectors` types to Python with
//! a familiar API that mirrors the original Python scrapling library.
//!
//! # Exposed API
//!
//! - `scrapling_rs.Selector(html, url=None)` — Parse HTML and query with CSS selectors
//! - `scrapling_rs.Selectors` — A list-like collection of matched elements
//! - `scrapling_rs.parse(html, url=None)` — Convenience function (same as `Selector(html)`)
//! - `scrapling_rs.to_markdown(html)` — Convert HTML to Markdown
//! - `scrapling_rs.to_text(html)` — Convert HTML to plain text
//!
//! # Building
//!
//! ```bash
//! cd crates/scrapling-python
//! maturin develop  # or maturin build --release
//! ```
//!
//! # Usage from Python
//!
//! ```python
//! import scrapling_rs
//!
//! page = scrapling_rs.parse("<html><body><h1>Hello</h1></body></html>")
//! titles = page.css("h1")
//! print(titles[0].text())  # "Hello"
//! ```

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use scrapling::TextHandler;
use scrapling::selector::Selector as RustSelector;

/// A parsed HTML document with CSS selection, text extraction, and adaptive relocation.
#[pyclass(name = "Selector")]
struct PySelector {
    inner: RustSelector,
}

#[pymethods]
impl PySelector {
    /// Parse an HTML string.
    #[new]
    #[pyo3(signature = (html, url=None))]
    fn new(html: &str, url: Option<&str>) -> Self {
        let inner = match url {
            Some(u) => RustSelector::from_html_with_url(html, u),
            None => RustSelector::from_html(html),
        };
        Self { inner }
    }

    /// Select elements matching a CSS selector.
    fn css(&self, selector: &str) -> PySelectors {
        let results = self.inner.css(selector);
        PySelectors {
            items: results
                .iter()
                .map(|s| PySelector { inner: s.clone() })
                .collect(),
        }
    }

    /// Get the element's tag name.
    fn tag(&self) -> String {
        self.inner.tag().to_owned()
    }

    /// Get the element's direct text content.
    fn text(&self) -> String {
        self.inner.text().into_inner()
    }

    /// Get all text content recursively.
    #[pyo3(signature = (separator=" ", strip=true))]
    fn get_all_text(&self, separator: &str, strip: bool) -> String {
        self.inner
            .get_all_text(separator, strip, &[], true)
            .into_inner()
    }

    /// Get the element's attributes as a dict.
    fn attrib(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in self.inner.attrib().iter() {
            dict.set_item(k, v.as_ref())?;
        }
        Ok(dict.into())
    }

    /// Get the inner HTML.
    fn html_content(&self) -> String {
        self.inner.html_content().into_inner()
    }

    /// Get the outer HTML.
    fn outer_html(&self) -> String {
        self.inner.outer_html().into_inner()
    }

    /// Join a relative URL with this element's base URL.
    fn urljoin(&self, relative: &str) -> String {
        self.inner.urljoin(relative)
    }

    /// Check if the element has a CSS class.
    fn has_class(&self, name: &str) -> bool {
        self.inner.has_class(name)
    }

    /// Get the parent element.
    fn parent(&self) -> Option<PySelector> {
        self.inner.parent().map(|p| PySelector { inner: p })
    }

    /// Get child elements.
    fn children(&self) -> PySelectors {
        let children = self.inner.children();
        PySelectors {
            items: children
                .iter()
                .map(|s| PySelector { inner: s.clone() })
                .collect(),
        }
    }

    /// Find elements by text content.
    #[pyo3(signature = (text, partial=false, case_sensitive=false, clean_match=false))]
    fn find_by_text(
        &self,
        text: &str,
        partial: bool,
        case_sensitive: bool,
        clean_match: bool,
    ) -> PySelectors {
        let results = self
            .inner
            .find_by_text(text, partial, case_sensitive, clean_match);
        PySelectors {
            items: results
                .iter()
                .map(|s| PySelector { inner: s.clone() })
                .collect(),
        }
    }

    /// Generate a unique CSS selector for this element.
    fn generate_css_selector(&self) -> String {
        self.inner.generate_css_selector()
    }

    /// Parse the element's text as JSON.
    fn json(&self) -> PyResult<PyObject> {
        let val: serde_json::Value = self
            .inner
            .text()
            .json()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Python::with_gil(|py| json_to_py(py, &val))
    }

    fn __str__(&self) -> String {
        format!("{}", self.inner)
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }

    fn __len__(&self) -> usize {
        self.inner.children().len()
    }
}

/// A collection of Selector results.
#[pyclass(name = "Selectors")]
struct PySelectors {
    items: Vec<PySelector>,
}

#[pymethods]
impl PySelectors {
    fn __len__(&self) -> usize {
        self.items.len()
    }

    fn __getitem__(&self, idx: isize) -> PyResult<PySelector> {
        let len = self.items.len() as isize;
        let actual = if idx < 0 { len + idx } else { idx };
        if actual < 0 || actual >= len {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "index out of range",
            ));
        }
        Ok(PySelector {
            inner: self.items[actual as usize].inner.clone(),
        })
    }

    /// Get the first element, or None.
    fn first(&self) -> Option<PySelector> {
        self.items.first().map(|s| PySelector {
            inner: s.inner.clone(),
        })
    }

    /// Get all elements as a list.
    fn getall(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|s| s.inner.get().into_inner())
            .collect()
    }

    fn __str__(&self) -> String {
        format!("Selectors({})", self.items.len())
    }

    fn __repr__(&self) -> String {
        format!("Selectors({})", self.items.len())
    }
}

/// A convenience function to parse HTML and return a Selector.
#[pyfunction]
#[pyo3(signature = (html, url=None))]
fn parse(html: &str, url: Option<&str>) -> PySelector {
    PySelector::new(html, url)
}

/// Convert HTML to Markdown.
#[pyfunction]
fn to_markdown(html: &str) -> String {
    scrapling::shell::Convertor::to_markdown(html)
}

/// Convert HTML to plain text.
#[pyfunction]
fn to_text(html: &str) -> String {
    scrapling::shell::Convertor::to_text(html)
}

fn json_to_py(py: Python<'_>, val: &serde_json::Value) -> PyResult<PyObject> {
    match val {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_pyobject(py)?.into_any().unbind()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

/// scrapling_rs - Rust-powered web scraping toolkit with Python bindings
#[pymodule]
fn scrapling_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySelector>()?;
    m.add_class::<PySelectors>()?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(to_markdown, m)?)?;
    m.add_function(wrap_pyfunction!(to_text, m)?)?;
    Ok(())
}
