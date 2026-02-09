//! Tree-sitter parsing logic for extracting functions from source code
//!
//! This module provides the core parsing functionality to extract function
//! definitions from Rust, Python, and JavaScript codebases using Tree-sitter.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tree_sitter::{Language, Parser, Query, QueryCursor};

use crate::error::{PolyDupError, Result};
use crate::queries::{JAVASCRIPT_QUERY, PYTHON_QUERY, RUST_QUERY};

/// Represents a parsed function node from source code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionNode {
    /// Starting byte offset in the source file
    pub start_byte: usize,
    /// Ending byte offset in the source file
    pub end_byte: usize,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// The function body as a string
    pub body: String,
    /// Optional function name (if captured by query)
    pub name: Option<String>,
}

impl FunctionNode {
    /// Creates a new FunctionNode
    pub fn new(
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        end_line: usize,
        body: String,
    ) -> Self {
        Self {
            start_byte,
            end_byte,
            start_line,
            end_line,
            body,
            name: None,
        }
    }

    /// Creates a new FunctionNode with a name
    pub fn with_name(
        start_byte: usize,
        end_byte: usize,
        start_line: usize,
        end_line: usize,
        body: String,
        name: String,
    ) -> Self {
        Self {
            start_byte,
            end_byte,
            start_line,
            end_line,
            body,
            name: Some(name),
        }
    }

    /// Returns the length of the function in bytes
    pub fn len(&self) -> usize {
        self.end_byte - self.start_byte
    }

    /// Returns true if the function is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Extracts all function definitions from the given source code
///
/// # Arguments
/// * `code` - The source code to parse
/// * `lang` - The Tree-sitter Language grammar to use
///
/// # Returns
/// * `Result<Vec<FunctionNode>>` - A vector of extracted function nodes
///
/// # Errors
/// Returns an error if:
/// - The parser fails to parse the code
/// - The query compilation fails
/// - Invalid UTF-8 is encountered
pub fn extract_functions(code: &str, lang: Language) -> Result<Vec<FunctionNode>> {
    extract_functions_with_path(code, lang, None)
}

/// Internal function that accepts optional path for better error messages
fn extract_functions_with_path(
    code: &str,
    lang: Language,
    path: Option<&Path>,
) -> Result<Vec<FunctionNode>> {
    // Create a new parser
    let mut parser = Parser::new();
    parser
        .set_language(lang)
        .context("Failed to set language for parser")?;

    // Parse the source code
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| PolyDupError::Parsing("Failed to parse source code".to_string()))?;

    // Get the appropriate query for the language
    let query_source = get_query_for_language(lang)?;

    // Compile the query
    let query = Query::new(lang, query_source).map_err(|e| PolyDupError::Parsing(e.to_string()))?;

    // Execute the query
    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, tree.root_node(), code.as_bytes());

    // Extract function nodes from query matches
    let mut functions = Vec::new();

    for match_ in matches {
        let mut func_start = None;
        let mut func_end = None;
        let mut func_start_line = None;
        let mut func_end_line = None;
        let mut func_name = None;
        let mut func_body = None;

        for capture in match_.captures {
            let node = capture.node;
            let capture_name = &query.capture_names()[capture.index as usize];

            match capture_name.as_str() {
                "func" => {
                    func_start = Some(node.start_byte());
                    func_end = Some(node.end_byte());
                    // Tree-sitter rows are 0-indexed, convert to 1-indexed for humans
                    func_start_line = Some(node.start_position().row + 1);
                    func_end_line = Some(node.end_position().row + 1);
                }
                "function.name" => {
                    func_name = Some(
                        node.utf8_text(code.as_bytes())
                            .with_context(|| {
                                if let Some(p) = path {
                                    format!(
                                        "Invalid UTF-8 in function name at {}:{}",
                                        p.display(),
                                        node.start_position().row + 1
                                    )
                                } else {
                                    format!(
                                        "Invalid UTF-8 in function name at line {}",
                                        node.start_position().row + 1
                                    )
                                }
                            })?
                            .to_string(),
                    );
                }
                "function.body" => {
                    func_body = Some(
                        node.utf8_text(code.as_bytes())
                            .with_context(|| {
                                if let Some(p) = path {
                                    format!(
                                        "Invalid UTF-8 in function body at {}:{}",
                                        p.display(),
                                        node.start_position().row + 1
                                    )
                                } else {
                                    format!(
                                        "Invalid UTF-8 in function body at line {}",
                                        node.start_position().row + 1
                                    )
                                }
                            })?
                            .to_string(),
                    );
                }
                _ => {}
            }
        }

        // Create FunctionNode if we have the required information
        if let (Some(start), Some(end), Some(start_line), Some(end_line)) =
            (func_start, func_end, func_start_line, func_end_line)
        {
            let body = func_body.unwrap_or_else(|| code[start..end].to_string());

            let function = if let Some(name) = func_name {
                FunctionNode::with_name(start, end, start_line, end_line, body, name)
            } else {
                FunctionNode::new(start, end, start_line, end_line, body)
            };

            functions.push(function);
        }
    }

    Ok(functions)
}

/// Returns the appropriate query string for a given Tree-sitter Language
fn get_query_for_language(lang: Language) -> Result<&'static str> {
    // Compare language pointers to identify which language we're dealing with
    // This is necessary because Language doesn't implement PartialEq

    let rust_lang = tree_sitter_rust::language();
    let python_lang = tree_sitter_python::language();
    let javascript_lang = tree_sitter_javascript::language();

    if is_same_language(lang, rust_lang) {
        Ok(&RUST_QUERY)
    } else if is_same_language(lang, python_lang) {
        Ok(&PYTHON_QUERY)
    } else if is_same_language(lang, javascript_lang) {
        Ok(&JAVASCRIPT_QUERY)
    } else {
        Err(PolyDupError::Parsing("Unsupported language".to_string()))
    }
}

/// Compares two Tree-sitter Language instances
///
/// Since Language doesn't implement PartialEq, we compare their internal
/// pointers as a proxy for equality.
fn is_same_language(lang1: Language, lang2: Language) -> bool {
    // Languages are considered equal if they have the same version and node kind count
    // This is a heuristic but works well in practice
    lang1.version() == lang2.version() && lang1.node_kind_count() == lang2.node_kind_count()
}

/// Convenience function to extract functions from Rust code
pub fn extract_rust_functions(code: &str) -> Result<Vec<FunctionNode>> {
    extract_functions(code, tree_sitter_rust::language())
}

/// Convenience function to extract functions from Python code
pub fn extract_python_functions(code: &str) -> Result<Vec<FunctionNode>> {
    extract_functions(code, tree_sitter_python::language())
}

/// Convenience function to extract functions from JavaScript code
pub fn extract_javascript_functions(code: &str) -> Result<Vec<FunctionNode>> {
    extract_functions(code, tree_sitter_javascript::language())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_function() {
        let code = r#"
fn hello_world() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

        let functions = extract_rust_functions(code).unwrap();
        assert_eq!(functions.len(), 2);

        // Check first function
        assert!(functions[0].name.as_deref() == Some("hello_world"));
        assert!(functions[0].body.contains("println!"));

        // Check second function
        assert!(functions[1].name.as_deref() == Some("add"));
        assert!(functions[1].body.contains("a + b"));
    }

    #[test]
    fn test_extract_python_function() {
        let code = r#"
def greet(name):
    return f"Hello, {name}!"

def multiply(x, y):
    return x * y
"#;

        let functions = extract_python_functions(code).unwrap();
        assert_eq!(functions.len(), 2);

        assert!(functions[0].name.as_deref() == Some("greet"));
        assert!(functions[1].name.as_deref() == Some("multiply"));
    }

    #[test]
    fn test_extract_javascript_function() {
        let code = r#"
function sayHello() {
    console.log("Hello!");
}

const add = (a, b) => {
    return a + b;
};
"#;

        let functions = extract_javascript_functions(code).unwrap();
        assert_eq!(functions.len(), 2);

        assert!(functions[0].name.as_deref() == Some("sayHello"));
        assert!(functions[0].body.contains("console.log"));
    }

    #[test]
    fn test_function_node_length() {
        let node = FunctionNode::new(10, 50, 1, 5, "test body".to_string());
        assert_eq!(node.len(), 40);
        assert!(!node.is_empty());
    }

    #[test]
    fn test_empty_code() {
        let functions = extract_rust_functions("").unwrap();
        assert_eq!(functions.len(), 0);
    }

    #[test]
    fn test_invalid_syntax() {
        let code = "fn broken {{{";
        let result = extract_rust_functions(code);
        // Should parse but find no complete functions
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }
}
