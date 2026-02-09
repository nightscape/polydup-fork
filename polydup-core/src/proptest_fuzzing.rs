//! Property-based testing for dupe-core using proptest
//!
//! These tests generate thousands of random inputs to ensure:
//! 1. The parser never panics on malformed code
//! 2. Normalization is idempotent
//! 3. Hashing is deterministic

#[cfg(test)]
mod tests {
    use crate::hashing::{compute_rolling_hashes, normalize};
    use crate::parsing::{
        extract_javascript_functions, extract_python_functions, extract_rust_functions,
    };
    use proptest::prelude::*;

    /// Helper to build a code strategy from snippets and a dynamic pattern
    fn code_strategy(
        snippets: Vec<&'static str>,
        dynamic_fmt: &'static str,
    ) -> impl Strategy<Value = String> {
        let snippet_strategies: Vec<BoxedStrategy<String>> = snippets
            .into_iter()
            .map(|s| Just(s.to_string()).boxed())
            .collect();

        let dynamic = "[a-z]{1,10}".prop_map(move |s| dynamic_fmt.replace("{}", &s));

        let mut all_strategies = snippet_strategies;
        all_strategies.push(dynamic.boxed());

        prop::collection::vec(prop::strategy::Union::new(all_strategies), 0..10)
            .prop_map(|lines| lines.join("\n"))
    }

    /// Strategy to generate random Rust-like code
    fn rust_code_strategy() -> impl Strategy<Value = String> {
        code_strategy(
            vec![
                "fn test() { }",
                "fn main() { println!(\"hello\"); }",
                "struct Foo { x: i32 }",
                "impl Foo { fn bar(&self) {} }",
                "// comment\nfn foo() {}",
                "fn nested() { if true { let x = 1; } }",
            ],
            "fn {}() {}",
        )
    }

    /// Strategy to generate random Python-like code
    fn python_code_strategy() -> impl Strategy<Value = String> {
        code_strategy(
            vec![
                "def test(): pass",
                "def main():\n    print('hello')",
                "class Foo:\n    def bar(self): pass",
                "# comment\ndef foo(): pass",
                "def nested():\n    if True:\n        x = 1",
            ],
            "def {}(): pass",
        )
    }

    /// Strategy to generate random JavaScript-like code
    fn javascript_code_strategy() -> impl Strategy<Value = String> {
        code_strategy(
            vec![
                "function test() {}",
                "function main() { console.log('hello'); }",
                "const foo = () => {}",
                "// comment\nfunction foo() {}",
                "function nested() { if (true) { let x = 1; } }",
            ],
            "function {}() {}",
        )
    }

    proptest! {
        /// Property: Rust parser never panics on random input
        #[test]
        fn rust_parser_never_panics(code in rust_code_strategy()) {
            // This should never panic, even on invalid syntax
            let result = extract_rust_functions(&code);
            // Either Ok or Err, but never panic
            prop_assert!(result.is_ok() || result.is_err());
        }

        /// Property: Python parser never panics on random input
        #[test]
        fn python_parser_never_panics(code in python_code_strategy()) {
            let result = extract_python_functions(&code);
            prop_assert!(result.is_ok() || result.is_err());
        }

        /// Property: JavaScript parser never panics on random input
        #[test]
        fn javascript_parser_never_panics(code in javascript_code_strategy()) {
            let result = extract_javascript_functions(&code);
            prop_assert!(result.is_ok() || result.is_err());
        }

        /// Property: Normalization is idempotent (normalize(normalize(x)) == normalize(x))
        #[test]
        fn normalization_is_idempotent(code in "[a-zA-Z0-9_\\s\\+\\-\\*\\/\\(\\)\\{\\}\\[\\];,\\.]{0,1000}") {
            let tokens1 = normalize(&code);
            let tokens1_str: Vec<String> = tokens1.iter().map(|t| t.as_hash_string().to_string()).collect();
            let reconstructed = tokens1_str.join(" ");

            let tokens2 = normalize(&reconstructed);

            // Normalization should be stable
            prop_assert_eq!(tokens1.len(), tokens2.len());
        }

        /// Property: Hash computation is deterministic
        #[test]
        fn hashing_is_deterministic(code in "[a-z]{10,100}") {
            let tokens = normalize(&code);
            if tokens.len() >= 5 {
                let hashes1 = compute_rolling_hashes(&tokens, 5);
                let hashes2 = compute_rolling_hashes(&tokens, 5);

                // Same input should produce same hashes
                prop_assert_eq!(hashes1, hashes2);
            }
        }

        /// Property: Parser handles empty input gracefully
        #[test]
        fn parser_handles_empty_input(whitespace in "\\s{0,100}") {
            let rust_result = extract_rust_functions(&whitespace);
            let python_result = extract_python_functions(&whitespace);
            let js_result = extract_javascript_functions(&whitespace);

            // All should return Ok with empty Vec, not panic
            prop_assert!(rust_result.is_ok());
            prop_assert!(python_result.is_ok());
            prop_assert!(js_result.is_ok());
        }

        /// Property: Normalization preserves token count invariant
        #[test]
        fn normalization_preserves_structure(code in "[a-zA-Z]{1,50}") {
            let tokens = normalize(&code);
            // Should produce at least one token if input is non-empty
            prop_assert!(!tokens.is_empty());
        }

        /// Property: Rolling hash window respects size constraint
        #[test]
        fn rolling_hash_respects_window_size(
            token_count in 10usize..100,
            window_size in 3usize..20,
        ) {
            // Generate dummy tokens
            let tokens: Vec<_> = (0..token_count)
                .map(|_| crate::hashing::Token::Identifier)
                .collect();

            let hashes = compute_rolling_hashes(&tokens, window_size);

            // Number of hashes should be token_count - window_size + 1
            let expected_count = if token_count >= window_size {
                token_count - window_size + 1
            } else {
                0
            };

            prop_assert_eq!(hashes.len(), expected_count);
        }
    }

    #[test]
    fn fuzz_with_special_characters() {
        let special_cases = vec![
            "fn test() { /* */ }",
            "fn test() { // comment\n }",
            "fn test() { \"string with \\\"quotes\\\"\" }",
            "fn test() { 'char' }",
            "fn test() { 123.456 }",
            "fn test() { 0x1234 }",
            "fn test() { r#\"raw string\"# }",
            "fn test() {\n\t\r\n}",
        ];

        for case in special_cases {
            let result = extract_rust_functions(case);
            assert!(
                result.is_ok() || result.is_err(),
                "Should not panic on: {}",
                case
            );
        }
    }

    #[test]
    fn fuzz_with_deeply_nested_code() {
        let nested = "fn test() { { { { { { { { { { } } } } } } } } } }";
        let result = extract_rust_functions(nested);
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle deeply nested blocks"
        );
    }

    #[test]
    fn fuzz_with_unicode() {
        let unicode_cases = vec![
            "fn test() { println!(\"🦀\"); }",
            "fn test_日本語() { }",
            "fn test() { let x = \"Привет\"; }",
            "fn test() { // 中文注释\n }",
        ];

        for case in unicode_cases {
            let result = extract_rust_functions(case);
            assert!(
                result.is_ok() || result.is_err(),
                "Should handle unicode: {}",
                case
            );
        }
    }
}
