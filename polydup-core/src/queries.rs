//! Tree-sitter query definitions for extracting functions from different languages
//!
//! These queries use Tree-sitter's S-expression query syntax to capture
//! function definitions across Rust, Python, and JavaScript/TypeScript.

use once_cell::sync::Lazy;

/// Tree-sitter query for Rust function extraction
///
/// Captures:
/// - `function_item`: Standard function definitions
/// - Methods within impl blocks
/// - Associated functions
pub static RUST_QUERY: Lazy<&'static str> = Lazy::new(|| include_str!("../queries/rust.scm"));

/// Tree-sitter query for Python function extraction
///
/// Captures:
/// - Function definitions
/// - Method definitions within classes
/// - Async function definitions
pub static PYTHON_QUERY: Lazy<&'static str> = Lazy::new(|| include_str!("../queries/python.scm"));

/// Tree-sitter query for JavaScript/TypeScript function extraction
///
/// Captures:
/// - Function declarations
/// - Function expressions
/// - Arrow functions
/// - Method definitions in classes
pub static JAVASCRIPT_QUERY: Lazy<&'static str> =
    Lazy::new(|| include_str!("../queries/javascript.scm"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queries_not_empty() {
        assert!(!RUST_QUERY.is_empty());
        assert!(!PYTHON_QUERY.is_empty());
        assert!(!JAVASCRIPT_QUERY.is_empty());
    }

    #[test]
    fn test_queries_contain_captures() {
        assert!(RUST_QUERY.contains("@func"));
        assert!(PYTHON_QUERY.contains("@func"));
        assert!(JAVASCRIPT_QUERY.contains("@func"));
    }
}
