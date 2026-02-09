//! Centralized default values for CLI configuration
//!
//! This module provides a single source of truth for all default values
//! used across the CLI. This ensures consistency between command-line
//! arguments, config files, and runtime behavior.

/// Default minimum block size for duplicate detection (in tokens)
pub const MIN_BLOCK_SIZE: usize = 50;

/// Default similarity threshold for duplicate detection (0.0-1.0)
pub const SIMILARITY: f64 = 0.85;

/// Default Type-3 tolerance for gap-tolerant clone detection (0.0-1.0)
pub const TYPE3_TOLERANCE: f64 = 0.85;

/// Check if a min_block_size value is the default
pub fn is_default_min_block_size(value: usize) -> bool {
    value == MIN_BLOCK_SIZE
}

/// Check if a similarity value is the default (with floating-point tolerance)
pub fn is_default_similarity(value: f64) -> bool {
    (value - SIMILARITY).abs() < 0.001
}
