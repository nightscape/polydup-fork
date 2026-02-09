//! Inline directive detection for suppressing false positives
//!
//! This module provides support for inline comments that suppress duplicate
//! detection warnings directly in source code, similar to linter directives.
//!
//! # Supported Directive Formats
//!
//! ## JavaScript/TypeScript/Rust
//! ```javascript
//! // polydup-ignore: intentional code reuse
//! function duplicateCode() { ... }
//! ```
//!
//! ## Python
//! ```python
//! # polydup-ignore: framework boilerplate
//! def duplicate_function():
//!     pass
//! ```
//!
//! # Detection Strategy
//!
//! Directives are detected by scanning comment lines immediately before
//! a function or code block. The directive suppresses duplicate detection
//! for the entire function/block that follows it.

use std::collections::HashMap;
use std::path::Path;

/// Represents a directive found in source code
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// Line number where the directive appears (1-indexed)
    pub line: usize,
    /// Optional reason provided in the directive
    pub reason: Option<String>,
}

/// Directive detection result for a single file
#[derive(Debug, Clone)]
pub struct FileDirectives {
    /// Map of line numbers to directives
    /// Key: Line number where suppression applies (function start line)
    /// Value: The directive that applies
    directives: HashMap<usize, Directive>,
}

impl FileDirectives {
    /// Creates an empty directive set
    pub fn new() -> Self {
        Self {
            directives: HashMap::new(),
        }
    }

    /// Checks if a line range is suppressed by a directive
    ///
    /// A directive suppresses a range if it appears within 3 lines before
    /// the start of the range (allowing for blank lines).
    ///
    /// # Arguments
    /// * `start_line` - Starting line of the code block (1-indexed)
    /// * `end_line` - Ending line of the code block (1-indexed)
    ///
    /// # Returns
    /// * `Some(Directive)` - If the range is suppressed
    /// * `None` - If no directive applies
    pub fn is_suppressed(&self, start_line: usize, _end_line: usize) -> Option<&Directive> {
        // Check the function start line and up to 3 lines before it
        // This allows for blank lines and multi-line comments between directive and function
        for offset in 0..=3 {
            if start_line > offset {
                let check_line = start_line - offset;
                if let Some(directive) = self.directives.get(&check_line) {
                    return Some(directive);
                }
            }
        }

        None
    }

    /// Adds a directive for a specific line
    fn add_directive(&mut self, line: usize, directive: Directive) {
        self.directives.insert(line, directive);
    }

    /// Returns the number of directives in this file
    pub fn len(&self) -> usize {
        self.directives.len()
    }

    /// Checks if there are any directives
    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }
}

impl Default for FileDirectives {
    fn default() -> Self {
        Self::new()
    }
}

/// Detects polydup-ignore directives in source code
///
/// Scans for comment lines containing "polydup-ignore" and extracts
/// optional reasons.
///
/// # Supported Formats
/// - `// polydup-ignore` (no reason)
/// - `// polydup-ignore: reason here`
/// - `# polydup-ignore: reason here` (Python)
///
/// # Arguments
/// * `source` - The source code to scan
///
/// # Returns
/// * `FileDirectives` - Detected directives with line numbers
pub fn detect_directives(source: &str) -> FileDirectives {
    let mut directives = FileDirectives::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1; // 1-indexed
        let trimmed = line.trim();

        // Check for polydup-ignore directive in comments
        if let Some(directive) = parse_directive_line(trimmed) {
            // Store the directive at the line where it appears
            // The is_suppressed() method will handle checking nearby lines
            directives.add_directive(line_num, directive);
        }
    }

    directives
}

/// Parses a single line to detect a polydup-ignore directive
///
/// # Arguments
/// * `line` - A trimmed line of source code
///
/// # Returns
/// * `Some(Directive)` - If the line contains a valid directive
/// * `None` - If no directive is found
fn parse_directive_line(line: &str) -> Option<Directive> {
    // Check for JavaScript/TypeScript/Rust style comments
    if let Some(rest) = line.strip_prefix("//") {
        return parse_comment_content(rest, line.len());
    }

    // Check for Python style comments
    if let Some(rest) = line.strip_prefix('#') {
        return parse_comment_content(rest, line.len());
    }

    None
}

/// Extracts directive information from comment content
fn parse_comment_content(content: &str, _line_len: usize) -> Option<Directive> {
    let content = content.trim();

    // Check for exact match or with colon
    if let Some(rest) = content.strip_prefix("polydup-ignore") {
        let rest = rest.trim();

        // Extract reason if provided after colon
        let reason = if let Some(after_colon) = rest.strip_prefix(':') {
            let r = after_colon.trim();
            if r.is_empty() {
                None
            } else {
                Some(r.to_string())
            }
        } else if rest.is_empty() {
            None
        } else {
            // If there's content but no colon, treat it as reason
            Some(rest.to_string())
        };

        return Some(Directive {
            line: 0, // Will be set by caller
            reason,
        });
    }

    None
}

/// Detects directives in a file
///
/// # Arguments
/// * `path` - Path to the source file
///
/// # Returns
/// * `Result<FileDirectives>` - Detected directives or error
pub fn detect_directives_in_file(path: &Path) -> crate::Result<FileDirectives> {
    let source = std::fs::read_to_string(path).map_err(crate::PolyDupError::Io)?;
    Ok(detect_directives(&source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_javascript_directive_with_reason() {
        let source = r#"
// polydup-ignore: intentional code reuse
function duplicate() {
    console.log("test");
}
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);

        // Directive is at line 2
        assert!(directives.is_suppressed(2, 5).is_some());
        assert!(directives.is_suppressed(3, 5).is_some()); // Function start should also match
    }

    #[test]
    fn test_detect_python_directive() {
        let source = r#"
# polydup-ignore: framework requirement
def duplicate_function():
    pass
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);
        assert!(directives.is_suppressed(2, 4).is_some());
    }

    #[test]
    fn test_directive_without_reason() {
        let source = "// polydup-ignore\nfunction test() {}";
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);

        let directive = directives.is_suppressed(1, 2).unwrap();
        assert!(directive.reason.is_none());
    }

    #[test]
    fn test_directive_with_colon_but_no_reason() {
        let source = "// polydup-ignore:\nfunction test() {}";
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);

        let directive = directives.is_suppressed(1, 2).unwrap();
        assert!(directive.reason.is_none());
    }

    #[test]
    fn test_no_directive() {
        let source = r#"
// This is just a regular comment
function not_ignored() {
    return 42;
}
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 0);
        assert!(directives.is_suppressed(2, 5).is_none());
    }

    #[test]
    fn test_multiple_directives() {
        let source = r#"
// polydup-ignore: reason 1
function fn1() {}

// polydup-ignore: reason 2
function fn2() {}
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 2);

        assert!(directives.is_suppressed(2, 3).is_some());
        assert!(directives.is_suppressed(5, 6).is_some());
    }

    #[test]
    fn test_rust_directive() {
        let source = r#"
// polydup-ignore: generated code
fn duplicate() -> i32 {
    42
}
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);
        assert!(directives.is_suppressed(2, 5).is_some());
    }

    // ========== Edge Case Tests (Issue #130) ==========

    #[test]
    fn directive_with_extra_whitespace_normalizes() {
        // Extra whitespace between comment marker and directive should be handled
        let source = "//   polydup-ignore: reason here\nfunction test() {}";
        let directives = detect_directives(source);
        assert_eq!(
            directives.len(),
            1,
            "Should detect directive with extra whitespace"
        );

        // Extra whitespace after directive keyword
        let source2 = "// polydup-ignore   : spaced reason\nfunction test() {}";
        let directives2 = detect_directives(source2);
        // Current behavior: treats content after "polydup-ignore" without colon as reason
        assert_eq!(
            directives2.len(),
            1,
            "Should detect directive with trailing whitespace"
        );
    }

    #[test]
    fn directive_no_space_after_comment_marker() {
        // No space between // and polydup-ignore
        let source = "//polydup-ignore: no space\nfunction test() {}";
        let directives = detect_directives(source);
        // Current behavior: requires at least comment prefix stripping, trim handles the rest
        assert_eq!(
            directives.len(),
            1,
            "Should detect directive without space after //"
        );
    }

    #[test]
    fn directive_case_sensitive() {
        // Uppercase directive - current behavior is case-sensitive
        let source = "// POLYDUP-IGNORE: uppercase\nfunction test() {}";
        let directives = detect_directives(source);
        assert_eq!(
            directives.len(),
            0,
            "Directive is case-sensitive - uppercase should NOT be detected"
        );

        // Mixed case
        let source2 = "// PolyDup-Ignore: mixed case\nfunction test() {}";
        let directives2 = detect_directives(source2);
        assert_eq!(
            directives2.len(),
            0,
            "Directive is case-sensitive - mixed case should NOT be detected"
        );
    }

    #[test]
    fn multiline_comment_directive_not_supported() {
        // Multi-line comment directives (/* */) are NOT currently supported
        let source = "/* polydup-ignore: legacy code */\nfunction oldImpl() {}";
        let directives = detect_directives(source);
        assert_eq!(
            directives.len(),
            0,
            "Multi-line comment directives (/* */) are not currently supported"
        );
    }

    #[test]
    fn inline_directive_after_code_not_detected() {
        // Directive at end of code line (inline comment) should NOT be detected
        // because we only match lines that START with comment markers
        let source = r#"function foo() { // polydup-ignore: inline
    return 42;
}"#;
        let directives = detect_directives(source);
        assert_eq!(
            directives.len(),
            0,
            "Inline directives after code should not be detected"
        );
    }

    #[test]
    fn multiple_directives_both_stored() {
        // Multiple consecutive directives - both should be stored
        let source = r#"// polydup-ignore: reason 1
// polydup-ignore: reason 2
function bar() { }
"#;
        let directives = detect_directives(source);
        assert_eq!(
            directives.len(),
            2,
            "Both consecutive directives should be stored"
        );

        // When checking suppression, both lines 1 and 2 have directives
        // The function at line 3 should be suppressed by the directive at line 2 (closest)
        assert!(
            directives.is_suppressed(3, 4).is_some(),
            "Function should be suppressed"
        );
    }

    #[test]
    fn directive_in_rust_doc_comment_triple_slash() {
        // Triple-slash doc comments start with ///
        // This is NOT a valid directive format (requires separate handling)
        let source = r#"/// polydup-ignore: doc comment
fn documented() -> i32 { 42 }
"#;
        let directives = detect_directives(source);
        // Current behavior: /// is parsed as // followed by / polydup-ignore...
        // So it won't match because the content starts with "/"
        assert_eq!(
            directives.len(),
            0,
            "Doc comments (///) should not be treated as directives"
        );
    }

    #[test]
    fn directive_with_different_languages_consistent() {
        // Python hash comment
        let python_source = "# polydup-ignore: python reason\ndef test(): pass";
        let python_directives = detect_directives(python_source);
        assert_eq!(
            python_directives.len(),
            1,
            "Python hash comment should work"
        );

        // JavaScript/Rust double-slash
        let js_source = "// polydup-ignore: js reason\nfunction test() {}";
        let js_directives = detect_directives(js_source);
        assert_eq!(
            js_directives.len(),
            1,
            "JS double-slash comment should work"
        );

        // Both should have the reason extracted
        let py_directive = python_directives.is_suppressed(1, 2).unwrap();
        assert_eq!(py_directive.reason, Some("python reason".to_string()));

        let js_directive = js_directives.is_suppressed(1, 2).unwrap();
        assert_eq!(js_directive.reason, Some("js reason".to_string()));
    }

    #[test]
    fn directive_with_blank_lines_before_function() {
        // Directive with blank lines between it and the function
        // The is_suppressed() method checks up to 3 lines before
        let source = r#"// polydup-ignore: with gap

function spaced() { return 1; }
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);
        // Function at line 3, directive at line 1, gap of 2 lines (within 3 line tolerance)
        assert!(
            directives.is_suppressed(3, 4).is_some(),
            "Should suppress function with 1 blank line gap"
        );
    }

    #[test]
    fn directive_too_far_from_function() {
        // Directive more than 3 lines before function should not apply
        let source = r#"// polydup-ignore: too far



function tooFar() { return 1; }
"#;
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);
        // Function at line 5, directive at line 1, gap of 4 lines (beyond 3 line tolerance)
        assert!(
            directives.is_suppressed(5, 6).is_none(),
            "Directive more than 3 lines away should not apply"
        );
    }

    #[test]
    fn directive_reason_with_special_characters() {
        // Reason containing special characters
        let source =
            "// polydup-ignore: reason with 'quotes' and \"double\" (parentheses)\nfn test() {}";
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);

        let directive = directives.is_suppressed(1, 2).unwrap();
        assert_eq!(
            directive.reason,
            Some("reason with 'quotes' and \"double\" (parentheses)".to_string())
        );
    }

    #[test]
    fn directive_with_trailing_whitespace_in_reason() {
        // Trailing whitespace in reason should be preserved (or trimmed - document behavior)
        let source = "// polydup-ignore: reason with trailing   \nfn test() {}";
        let directives = detect_directives(source);
        assert_eq!(directives.len(), 1);

        let directive = directives.is_suppressed(1, 2).unwrap();
        // Current behavior: reason is trimmed
        assert_eq!(
            directive.reason,
            Some("reason with trailing".to_string()),
            "Trailing whitespace in reason should be trimmed"
        );
    }
}
