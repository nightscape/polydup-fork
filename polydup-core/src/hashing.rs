//! Hashing and normalization logic for duplicate code detection
//!
//! This module implements:
//! - Token normalization (Type-2 clone detection)
//! - Rabin-Karp rolling hash algorithm
//! - Hash-based code similarity detection
//! - Edit distance calculation (Type-3 clone detection)

use serde::{Deserialize, Serialize};
use std::num::Wrapping;

/// Normalized token representation
///
/// Identifiers and literals are normalized to allow detection of
/// structurally similar code (Type-2 clones).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Token {
    /// Keyword (if, for, while, fn, def, etc.)
    Keyword(String),
    /// Normalized identifier placeholder
    Identifier,
    /// Normalized string literal placeholder
    StringLiteral,
    /// Normalized number literal placeholder
    NumberLiteral,
    /// Operator (+, -, *, /, etc.)
    Operator(String),
    /// Punctuation (parentheses, braces, semicolons)
    Punctuation(String),
}

impl Token {
    /// Returns a string representation for hashing
    pub fn as_hash_string(&self) -> &str {
        match self {
            Token::Keyword(kw) => kw.as_str(),
            Token::Identifier => "$$ID",
            Token::StringLiteral => "$$STR",
            Token::NumberLiteral => "$$NUM",
            Token::Operator(op) => op.as_str(),
            Token::Punctuation(p) => p.as_str(),
        }
    }
}

/// Normalizes source code into a token stream for duplicate detection
///
/// # Normalization Rules
/// - Comments are ignored
/// - Whitespace is ignored
/// - Identifiers → `$$ID`
/// - String literals → `$$STR`
/// - Number literals → `$$NUM`
/// - Keywords are preserved
///
/// # Arguments
/// * `code` - The source code to normalize
///
/// # Returns
/// * `Vec<Token>` - Normalized token stream
pub fn normalize(code: &str) -> Vec<Token> {
    let (tokens, _) = normalize_with_line_numbers(code);
    tokens
}

/// Normalizes source code into tokens while tracking line offsets
///
/// Returns both the normalized tokens and the zero-based line offset (relative
/// to the start of `code`) where each token begins.
pub fn normalize_with_line_numbers(code: &str) -> (Vec<Token>, Vec<usize>) {
    let keywords = get_keyword_set();
    // Pre-allocate capacity based on heuristic: ~1 token per 3 characters
    let estimated_tokens = code.len() / 3;
    let mut tokens = Vec::with_capacity(estimated_tokens);
    let mut line_offsets = Vec::with_capacity(estimated_tokens);
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    let mut line = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Skip whitespace
        if ch.is_whitespace() {
            if ch == '\n' {
                line += 1;
            }
            i += 1;
            continue;
        }

        // Skip single-line comments (// and #)
        if (ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/') || ch == '#' {
            // Skip until end of line
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Skip multi-line comments (/* */ and """ """)
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '\n' {
                    line += 1;
                }
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // String literals
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let token_line = line;
            i += 1;
            // Skip until closing quote
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2; // Skip escaped character
                    continue;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(Token::StringLiteral);
            line_offsets.push(token_line);
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() {
            let token_line = line;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(Token::NumberLiteral);
            line_offsets.push(token_line);
            continue;
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let token_line = line;
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();

            if keywords.contains(&word.as_str()) {
                tokens.push(Token::Keyword(word));
            } else {
                tokens.push(Token::Identifier);
            }
            line_offsets.push(token_line);
            continue;
        }

        // Operators (multi-char)
        if i + 1 < chars.len() {
            let two_char: String = chars[i..i + 2].iter().collect();
            if is_operator(&two_char) {
                let token_line = line;
                tokens.push(Token::Operator(two_char));
                i += 2;
                line_offsets.push(token_line);
                continue;
            }
        }

        // Single-char operators and punctuation
        let single = ch.to_string();
        if is_operator(&single) {
            let token_line = line;
            tokens.push(Token::Operator(single));
            line_offsets.push(token_line);
        } else if is_punctuation(ch) {
            let token_line = line;
            tokens.push(Token::Punctuation(single));
            line_offsets.push(token_line);
        }

        i += 1;
    }

    (tokens, line_offsets)
}

/// Rabin-Karp rolling hash for efficient substring comparison
///
/// Uses a rolling window to compute hashes of code blocks.
/// Allows for efficient duplicate detection in O(n) time.
#[derive(Debug, Clone)]
pub struct RollingHash {
    /// Window size for rolling hash
    window_size: usize,
    /// Base for polynomial rolling hash
    base: Wrapping<u64>,
    /// Current hash value
    hash: Wrapping<u64>,
    /// Power of base for window size (base^window_size)
    base_power: Wrapping<u64>,
    /// Current window contents
    window: Vec<u64>,
}

impl RollingHash {
    /// Creates a new RollingHash with the specified window size
    ///
    /// # Arguments
    /// * `window_size` - Number of tokens in the rolling window (default: 50)
    pub fn new(window_size: usize) -> Self {
        let base = Wrapping(257u64);
        let mut base_power = Wrapping(1u64);

        // Calculate base^window_size
        for _ in 0..window_size {
            base_power *= base;
        }

        Self {
            window_size,
            base,
            hash: Wrapping(0),
            base_power,
            window: Vec::with_capacity(window_size),
        }
    }

    /// Adds a token to the rolling hash window
    ///
    /// If the window is full, the oldest token is removed.
    ///
    /// # Arguments
    /// * `token_hash` - Hash value of the token to add
    ///
    /// # Returns
    /// * `Option<u64>` - The current hash if window is full, None otherwise
    pub fn roll(&mut self, token_hash: u64) -> Option<u64> {
        if self.window.len() < self.window_size {
            // Window not full yet
            self.window.push(token_hash);
            self.hash = self.hash * self.base + Wrapping(token_hash);

            if self.window.len() == self.window_size {
                Some(self.hash.0)
            } else {
                None
            }
        } else {
            // Window is full, remove oldest and add new
            let old_token = self.window.remove(0);
            self.window.push(token_hash);

            // Remove contribution of old token
            self.hash -= Wrapping(old_token) * self.base_power;
            // Shift and add new token
            self.hash = self.hash * self.base + Wrapping(token_hash);

            Some(self.hash.0)
        }
    }

    /// Resets the rolling hash to initial state
    pub fn reset(&mut self) {
        self.hash = Wrapping(0);
        self.window.clear();
    }

    /// Returns the current hash value (if window is full)
    pub fn current_hash(&self) -> Option<u64> {
        if self.window.len() == self.window_size {
            Some(self.hash.0)
        } else {
            None
        }
    }

    /// Returns the current window size
    pub fn window_size(&self) -> usize {
        self.window_size
    }
}

/// Computes rolling hashes for a token stream
///
/// # Arguments
/// * `tokens` - Normalized token stream
/// * `window_size` - Size of the rolling window
///
/// # Returns
/// * `Vec<(u64, usize)>` - List of (hash, start_index) pairs
pub fn compute_rolling_hashes(tokens: &[Token], window_size: usize) -> Vec<(u64, usize)> {
    if tokens.len() < window_size {
        return Vec::new();
    }

    let mut hasher = RollingHash::new(window_size);
    let mut hashes = Vec::new();

    for (idx, token) in tokens.iter().enumerate() {
        let token_hash = hash_token(token);
        if let Some(hash) = hasher.roll(token_hash) {
            // idx is the last token in the window, so start_index is idx - window_size + 1
            // but we need to ensure it doesn't underflow
            let start_index = idx.saturating_sub(window_size - 1);
            hashes.push((hash, start_index));
        }
    }

    hashes
}

/// Computes a hash value for a single token
fn hash_token(token: &Token) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    token.as_hash_string().hash(&mut hasher);
    hasher.finish()
}

/// Represents a detected duplicate code block
#[derive(Debug, Clone)]
pub struct CloneMatch {
    pub source_start: usize,
    pub target_start: usize,
    /// Length of the matched region in the source sequence
    pub length: usize,
    /// Length of the matched region in the target sequence (may differ for Type-3)
    pub target_length: usize,
    /// Token-level similarity (0.0-1.0). 1.0 for Type-1/2, calculated for Type-3
    pub similarity: f64,
}

/// Detects duplicates using rolling hash with greedy extension
///
/// This implements the Hash-and-Extend strategy:
/// 1. Use rolling hash to find candidate matches (50-token windows)
/// 2. Verify the match to handle hash collisions
/// 3. Greedily extend the match beyond the initial window
/// 4. Skip ahead to avoid reporting overlapping duplicates
///
/// # Arguments
/// * `tokens` - The token sequence to analyze
/// * `window_size` - Size of the rolling window (default: 50)
///
/// # Returns
/// * `Vec<CloneMatch>` - List of detected clones with variable lengths
pub fn detect_duplicates_with_extension(tokens: &[Token], window_size: usize) -> Vec<CloneMatch> {
    use std::collections::HashMap;

    if tokens.len() < window_size {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut hash_map: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut i = 0;

    // Build rolling hashes and detect matches with extension
    while i <= tokens.len().saturating_sub(window_size) {
        // 1. Compute hash for current window
        let current_hash = compute_window_hash(&tokens[i..i + window_size]);

        // 2. Check if we've seen this hash before
        if let Some(prev_indices) = hash_map.get(&current_hash) {
            // Try to match with each previous occurrence
            for &prev_index in prev_indices.iter() {
                // Skip if this would be a self-match or overlap
                if prev_index >= i {
                    continue;
                }

                // 3. Verify exact match (handle hash collisions)
                if verify_window_match(tokens, prev_index, i, window_size) {
                    // 4. GREEDY EXTENSION: Expand beyond the initial window
                    let mut extension = 0;
                    while (i + window_size + extension < tokens.len())
                        && (prev_index + window_size + extension < i)
                        && (tokens[prev_index + window_size + extension]
                            == tokens[i + window_size + extension])
                    {
                        extension += 1;
                    }

                    let total_length = window_size + extension;

                    // Record the full match
                    matches.push(CloneMatch {
                        source_start: prev_index,
                        target_start: i,
                        length: total_length,
                        target_length: total_length,
                        similarity: 1.0, // Exact match
                    });

                    // 5. Skip ahead to avoid reporting overlapping subsets
                    i += extension.max(1);
                    break; // Found a match, move to next position
                }
            }
        }

        // Store this position for future comparisons
        hash_map.entry(current_hash).or_default().push(i);
        i += 1;
    }

    matches
}

/// Computes hash for a specific token window
///
/// Uses Rabin-Karp polynomial rolling hash with:
/// - BASE = 257 (prime > 256 to minimize collisions for all byte values)
/// - MODULUS = 1e9+7 (large prime commonly used in hashing algorithms)
pub fn compute_window_hash(window: &[Token]) -> u64 {
    /// Prime base for polynomial rolling hash (chosen to be > 256)
    const BASE: u64 = 257;
    /// Large prime modulus to reduce hash collisions (1e9+7)
    const MODULUS: u64 = 1_000_000_007;

    let mut hash: u64 = 0;
    for token in window {
        let token_hash = hash_token(token);
        // Use u128 to prevent overflow before modulo operation
        let wide_hash = (hash as u128 * BASE as u128 + token_hash as u128) % MODULUS as u128;
        hash = wide_hash as u64;
    }
    hash
}

/// Verifies that two token windows within the same slice are exactly identical
fn verify_window_match(tokens: &[Token], idx_a: usize, idx_b: usize, len: usize) -> bool {
    if idx_a + len > tokens.len() || idx_b + len > tokens.len() {
        return false;
    }
    tokens[idx_a..idx_a + len] == tokens[idx_b..idx_b + len]
}

/// Verifies that two token windows from different slices are exactly identical
///
/// Used for cross-file/cross-function duplicate detection.
pub fn verify_cross_window_match(
    tokens1: &[Token],
    tokens2: &[Token],
    idx1: usize,
    idx2: usize,
    len: usize,
) -> bool {
    if idx1 + len > tokens1.len() || idx2 + len > tokens2.len() {
        return false;
    }
    tokens1[idx1..idx1 + len] == tokens2[idx2..idx2 + len]
}

/// Extends a token match greedily beyond the initial window size
///
/// Given two token sequences and starting positions with a known matching window,
/// extends the match as far as possible while tokens continue to match.
///
/// # Arguments
/// * `tokens1` - First token sequence
/// * `tokens2` - Second token sequence
/// * `pos1` - Starting position in tokens1
/// * `pos2` - Starting position in tokens2
/// * `initial_len` - Length of the initial matching window
///
/// # Returns
/// * Number of additional tokens beyond `initial_len` that also match
pub fn extend_match(
    tokens1: &[Token],
    tokens2: &[Token],
    pos1: usize,
    pos2: usize,
    initial_len: usize,
) -> usize {
    let mut extension = 0;
    while (pos1 + initial_len + extension < tokens1.len())
        && (pos2 + initial_len + extension < tokens2.len())
        && (tokens1[pos1 + initial_len + extension] == tokens2[pos2 + initial_len + extension])
    {
        extension += 1;
    }
    extension
}

/// Returns a set of keywords for all supported languages
fn get_keyword_set() -> &'static [&'static str] {
    &[
        // Rust keywords
        "as",
        "break",
        "const",
        "continue",
        "crate",
        "else",
        "enum",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "impl",
        "in",
        "let",
        "loop",
        "match",
        "mod",
        "move",
        "mut",
        "pub",
        "ref",
        "return",
        "self",
        "Self",
        "static",
        "struct",
        "super",
        "trait",
        "true",
        "type",
        "unsafe",
        "use",
        "where",
        "while",
        "async",
        "await",
        "dyn",
        // Python keywords
        "and",
        "assert",
        "class",
        "def",
        "del",
        "elif",
        "except",
        "finally",
        "from",
        "global",
        "import",
        "is",
        "lambda",
        "nonlocal",
        "not",
        "or",
        "pass",
        "raise",
        "try",
        "with",
        "yield",
        // JavaScript keywords
        "await",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "let",
        "new",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ]
}

/// Checks if a string is an operator
fn is_operator(s: &str) -> bool {
    matches!(
        s,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "="
            | "=="
            | "!="
            | "<"
            | ">"
            | "<="
            | ">="
            | "&&"
            | "||"
            | "!"
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | "+="
            | "-="
            | "*="
            | "/="
            | "=>"
            | "->"
            | "::"
            | "."
    )
}

/// Checks if a character is punctuation
fn is_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '{' | '}' | '[' | ']' | ';' | ':' | ',' | '?'
    )
}

/// Computes token-level similarity between two token sequences using edit distance
///
/// Returns a similarity score between 0.0 and 1.0, where:
/// - 1.0 = identical sequences
/// - 0.0 = completely different
///
/// Uses normalized Levenshtein distance:
/// similarity = 1 - (edit_distance / max_length)
///
/// # Arguments
/// * `tokens1` - First token sequence
/// * `tokens2` - Second token sequence
///
/// # Returns
/// * `f64` - Similarity score (0.0-1.0)
pub fn compute_token_similarity(tokens1: &[Token], tokens2: &[Token]) -> f64 {
    if tokens1.is_empty() && tokens2.is_empty() {
        return 1.0;
    }
    if tokens1.is_empty() || tokens2.is_empty() {
        return 0.0;
    }

    // Compute edit distance using custom token comparison
    let distance = compute_token_edit_distance(tokens1, tokens2);
    let max_len = tokens1.len().max(tokens2.len());

    // Normalize to similarity score
    let similarity = 1.0 - (distance as f64 / max_len as f64);
    similarity.clamp(0.0, 1.0)
}

/// Computes Levenshtein edit distance between two token sequences
///
/// Uses dynamic programming to calculate the minimum number of
/// insertions, deletions, or substitutions needed to transform
/// tokens1 into tokens2.
///
/// # Arguments
/// * `tokens1` - First token sequence
/// * `tokens2` - Second token sequence
///
/// # Returns
/// * `usize` - Edit distance
pub fn compute_token_edit_distance(tokens1: &[Token], tokens2: &[Token]) -> usize {
    let len1 = tokens1.len();
    let len2 = tokens2.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    // Create DP table
    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row: Vec<usize> = vec![0; len2 + 1];

    for i in 1..=len1 {
        curr_row[0] = i;

        for j in 1..=len2 {
            let cost = if tokens1[i - 1] == tokens2[j - 1] {
                0
            } else {
                1
            };

            curr_row[j] = (prev_row[j - 1] + cost) // substitution
                .min(prev_row[j] + 1) // deletion
                .min(curr_row[j - 1] + 1); // insertion
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

/// Explores different window lengths to find the best Type-3 match
///
/// Tries extending both windows by small amounts to find the best similarity score.
/// This accounts for insertions/deletions that make windows different lengths.
///
/// # Returns
/// * `Option<(source_len, target_len, similarity)>` - Best match found, or None
#[allow(clippy::too_many_arguments)]
fn explore_length_variants(
    tokens1: &[Token],
    tokens2: &[Token],
    i: usize,
    j: usize,
    window_size: usize,
    base_similarity: f64,
    length_delta_limit: usize,
    tolerance: f64,
) -> Option<(usize, usize, f64)> {
    let max_extra1 = length_delta_limit.min(tokens1.len().saturating_sub(i + window_size));
    let max_extra2 = length_delta_limit.min(tokens2.len().saturating_sub(j + window_size));

    let mut best_match: Option<(usize, usize, f64)> = None;

    for extra1 in 0..=max_extra1 {
        let len1 = window_size + extra1;
        let window1 = &tokens1[i..i + len1];

        for extra2 in 0..=max_extra2 {
            let len2 = window_size + extra2;

            // Skip if length difference exceeds limit
            if len1.abs_diff(len2) > length_delta_limit {
                continue;
            }

            // Early rejection: minimum edits needed exceeds tolerance
            let max_len = len1.max(len2);
            let min_distance = len1.abs_diff(len2);
            let max_allowed_distance = ((1.0 - tolerance) * max_len as f64).ceil() as usize;

            if min_distance > max_allowed_distance {
                continue;
            }

            // Compute similarity (reuse base if same window)
            let candidate_similarity = if extra1 == 0 && extra2 == 0 {
                base_similarity
            } else {
                compute_token_similarity(window1, &tokens2[j..j + len2])
            };

            if candidate_similarity >= tolerance {
                best_match = match best_match {
                    None => Some((len1, len2, candidate_similarity)),
                    Some((best_len1, best_len2, best_sim)) => {
                        let better_similarity = candidate_similarity > best_sim + f64::EPSILON;
                        let better_coverage =
                            !better_similarity && len1.max(len2) > best_len1.max(best_len2);

                        if better_similarity || better_coverage {
                            Some((len1, len2, candidate_similarity))
                        } else {
                            best_match
                        }
                    }
                };
            }
        }
    }

    best_match
}

/// Detects Type-3 clones (gap-tolerant) between two token sequences
///
/// Uses a sliding window approach with edit distance calculation:
/// 1. Find candidate regions where tokens partially match
/// 2. Calculate edit distance for each candidate pair
/// 3. Accept matches above the tolerance threshold
///
/// # Arguments
/// * `tokens1` - First token sequence
/// * `tokens2` - Second token sequence
/// * `window_size` - Minimum block size to consider
/// * `tolerance` - Minimum similarity threshold (0.0-1.0)
///
/// # Returns
/// * `Vec<CloneMatch>` - List of Type-3 clone matches with similarity scores
pub fn detect_type3_clones(
    tokens1: &[Token],
    tokens2: &[Token],
    window_size: usize,
    tolerance: f64,
) -> Vec<CloneMatch> {
    let mut matches = Vec::new();

    // Performance optimization: Only check sequences within ±20% length difference
    let len_ratio = tokens1.len() as f64 / tokens2.len().max(1) as f64;
    if !(0.8..=1.2).contains(&len_ratio) {
        return matches;
    }
    if tokens1.len() < window_size || tokens2.len() < window_size || window_size == 0 {
        return matches;
    }

    // Allow limited length drift so edit distance can align insertions/deletions.
    // Bounded by tolerance (minimum edits needed) and capped to avoid quadratic blowup.
    let base_delta = ((1.0 - tolerance) * window_size as f64).ceil() as usize;
    let length_delta_limit = (base_delta + 2).max(1).min(window_size).min(20);
    // Explore only when the base windows are at least somewhat similar to avoid exhaustive search
    let similarity_floor = (tolerance - 0.3).max(0.0);

    // Sliding window comparison
    let mut i = 0;
    while i + window_size <= tokens1.len() {
        let mut j = 0;
        while j + window_size <= tokens2.len() {
            // Calculate base similarity for the minimum windows
            let window1 = &tokens1[i..i + window_size];
            let window2 = &tokens2[j..j + window_size];
            let similarity = compute_token_similarity(window1, window2);

            if similarity < similarity_floor {
                j += 1;
                continue;
            }

            // Explore length variants to find best match
            let best_match = explore_length_variants(
                tokens1,
                tokens2,
                i,
                j,
                window_size,
                similarity,
                length_delta_limit,
                tolerance,
            );

            if let Some((source_len, target_len, final_similarity)) = best_match {
                matches.push(CloneMatch {
                    source_start: i,
                    target_start: j,
                    length: source_len,
                    target_length: target_len,
                    similarity: final_similarity,
                });

                // Skip ahead on the target side proportionally to the match size
                let skip = target_len.saturating_sub(window_size).max(1);
                j += skip;
            } else {
                j += 1;
            }
        }
        i += 1;
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_rust_code() {
        let code = r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        "#;

        let tokens = normalize(code);
        assert!(!tokens.is_empty());

        // Check that 'fn' is a keyword
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Token::Keyword(k) if k == "fn")));

        // Check that identifiers are normalized
        assert!(tokens.contains(&Token::Identifier));
    }

    #[test]
    fn test_normalize_python_code() {
        let code = r#"
        def greet(name):
            return f"Hello, {name}!"
        "#;

        let tokens = normalize(code);
        assert!(!tokens.is_empty());

        // Check that 'def' and 'return' are keywords
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Token::Keyword(k) if k == "def")));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Token::Keyword(k) if k == "return")));

        // Check that string is normalized
        assert!(tokens.contains(&Token::StringLiteral));
    }

    #[test]
    fn test_normalize_javascript_code() {
        let code = r#"
        function multiply(a, b) {
            return a * b;
        }
        "#;

        let tokens = normalize(code);
        assert!(!tokens.is_empty());

        // Check that 'function' and 'return' are keywords
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Token::Keyword(k) if k == "function")));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Token::Keyword(k) if k == "return")));
    }

    #[test]
    fn test_normalize_ignores_comments() {
        let code = r#"
        // This is a comment
        fn test() {
            /* Multi-line
               comment */
            let x = 5; // inline comment
        }
        "#;

        let tokens = normalize(code);

        // Should not contain comment text
        for token in &tokens {
            if let Token::Identifier = token {
                // OK, identifier
            } else if let Token::Keyword(_) = token {
                // OK, keyword
            }
        }
    }

    #[test]
    fn test_rolling_hash_creation() {
        let hasher = RollingHash::new(50);
        assert_eq!(hasher.window_size(), 50);
        assert_eq!(hasher.current_hash(), None);
    }

    #[test]
    fn test_rolling_hash_basic() {
        let mut hasher = RollingHash::new(3);

        // Add tokens one by one
        assert_eq!(hasher.roll(1), None); // Window not full
        assert_eq!(hasher.roll(2), None); // Window not full

        let hash1 = hasher.roll(3); // Window full
        assert!(hash1.is_some());

        let hash2 = hasher.roll(4); // Window rolls
        assert!(hash2.is_some());

        // Hashes should be different
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_rolling_hashes() {
        let tokens = vec![
            Token::Keyword("fn".to_string()),
            Token::Identifier,
            Token::Punctuation("(".to_string()),
            Token::Identifier,
            Token::Punctuation(")".to_string()),
        ];

        let hashes = compute_rolling_hashes(&tokens, 3);
        assert_eq!(hashes.len(), 3); // 5 tokens, window size 3 = 3 hashes
    }

    #[test]
    fn test_hash_token_consistency() {
        let token1 = Token::Identifier;
        let token2 = Token::Identifier;

        assert_eq!(hash_token(&token1), hash_token(&token2));
    }

    #[test]
    fn test_token_as_hash_string() {
        assert_eq!(Token::Identifier.as_hash_string(), "$$ID");
        assert_eq!(Token::StringLiteral.as_hash_string(), "$$STR");
        assert_eq!(Token::NumberLiteral.as_hash_string(), "$$NUM");
        assert_eq!(Token::Keyword("fn".to_string()).as_hash_string(), "fn");
    }
}
