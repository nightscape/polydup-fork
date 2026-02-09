//! Hash cache for incremental duplicate detection
//!
//! This module implements a persistent hash cache that stores rolling hashes
//! for all functions in the codebase. This enables:
//! - Fast git-diff mode: scan only changed files, lookup against cached hashes
//! - Incremental scanning: only rescan files that changed
//! - 10-100x speedup for large codebases

use crate::hashing::Token;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Location of a code block in the codebase
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CodeLocation {
    /// Absolute path to the file
    pub file_path: String,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// Token offset of this window within the function
    #[serde(default)]
    pub token_offset: Option<usize>,
    /// Length in tokens
    pub token_length: usize,
    /// The normalized token sequence (for similarity calculation)
    pub tokens: Vec<Token>,
    /// Raw source code (for Type-1 detection)
    pub raw_source: String,
}

/// Metadata about a cached file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheMetadata {
    /// Absolute path to the file
    pub path: String,
    /// Last modification time in nanoseconds since Unix epoch
    pub mtime: u64,
    /// File size in bytes
    pub size: u64,
}

/// The complete hash cache for a codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashCache {
    /// Version of the cache format (for future compatibility)
    pub version: String,
    /// Minimum block size (tokens) used to build this cache
    pub min_block_size: usize,
    /// Git commit hash when cache was built (if available)
    pub git_commit: Option<String>,
    /// Timestamp when cache was created
    pub created_at: u64,
    /// Map from rolling hash to all locations with that hash
    pub hash_index: HashMap<u64, Vec<CodeLocation>>,
    /// Metadata for all cached files (for invalidation)
    pub file_metadata: HashMap<String, FileCacheMetadata>,
}

impl HashCache {
    /// Create a new empty cache
    pub fn new(min_block_size: usize) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            min_block_size,
            git_commit: get_current_git_commit(),
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hash_index: HashMap::new(),
            file_metadata: HashMap::new(),
        }
    }

    /// Add a hash entry to the cache
    pub fn add_hash(&mut self, hash: u64, location: CodeLocation) {
        // Also store file metadata for cache invalidation
        if !self.file_metadata.contains_key(&location.file_path) {
            if let Ok(metadata) = get_file_metadata(&location.file_path) {
                self.file_metadata
                    .insert(location.file_path.clone(), metadata);
            }
        }

        self.hash_index.entry(hash).or_default().push(location);
    }

    /// Look up all locations with a given hash
    pub fn lookup(&self, hash: u64) -> Option<&Vec<CodeLocation>> {
        self.hash_index.get(&hash)
    }

    /// Check if a file needs to be rescanned (has changed since cache was built)
    pub fn file_needs_rescan(&self, file_path: &str) -> bool {
        match self.file_metadata.get(file_path) {
            Some(cached_meta) => {
                // Check if file still exists and hasn't changed
                match get_file_metadata(file_path) {
                    Ok(current_meta) => {
                        cached_meta.mtime != current_meta.mtime
                            || cached_meta.size != current_meta.size
                    }
                    Err(_) => true, // File deleted or inaccessible
                }
            }
            None => true, // File not in cache
        }
    }

    /// Remove all cache entries for a specific file
    pub fn invalidate_file(&mut self, file_path: &str) {
        // Remove from metadata
        self.file_metadata.remove(file_path);

        // Remove all hash entries for this file
        for locations in self.hash_index.values_mut() {
            locations.retain(|loc| loc.file_path != file_path);
        }

        // Clean up empty hash entries
        self.hash_index.retain(|_, locations| !locations.is_empty());
    }

    /// Drop cache entries for files whose metadata no longer matches disk.
    ///
    /// Returns the set of file paths that were removed so callers can refresh
    /// the cache entries when needed.
    pub fn invalidate_stale_files(&mut self) -> HashSet<String> {
        let mut stale_files: HashSet<String> = self
            .file_metadata
            .keys()
            .filter(|path| self.file_needs_rescan(path))
            .cloned()
            .collect();

        // Defensive: if a cache entry exists without metadata, treat it as stale
        for locations in self.hash_index.values() {
            for loc in locations {
                if !self.file_metadata.contains_key(&loc.file_path) {
                    stale_files.insert(loc.file_path.clone());
                }
            }
        }

        if stale_files.is_empty() {
            return stale_files;
        }

        self.file_metadata
            .retain(|path, _| !stale_files.contains(path));

        self.hash_index.retain(|_, locations| {
            locations.retain(|loc| !stale_files.contains(&loc.file_path));
            !locations.is_empty()
        });

        stale_files
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_hashes = self.hash_index.len();
        let total_locations: usize = self.hash_index.values().map(|v| v.len()).sum();
        let files_cached = self.file_metadata.len();

        CacheStats {
            total_hashes,
            total_locations,
            files_cached,
            created_at: self.created_at,
            git_commit: self.git_commit.clone(),
        }
    }

    /// Save cache to disk
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize cache to JSON")?;
        fs::write(path.as_ref(), json)
            .with_context(|| format!("Failed to write cache to {}", path.as_ref().display()))?;
        Ok(())
    }

    /// Load cache from disk
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read cache from {}", path.as_ref().display()))?;
        let cache: HashCache =
            serde_json::from_str(&json).context("Failed to deserialize cache JSON")?;

        // Version check
        if cache.version != env!("CARGO_PKG_VERSION") {
            anyhow::bail!(
                "Cache version mismatch: cache is v{}, but this is v{}. Please rebuild cache.",
                cache.version,
                env!("CARGO_PKG_VERSION")
            );
        }

        Ok(cache)
    }

    /// Check if cache exists and is valid
    pub fn is_valid<P: AsRef<Path>>(path: P) -> bool {
        Self::load(path).is_ok()
    }
}

impl Default for HashCache {
    fn default() -> Self {
        Self::new(50) // Use default threshold
    }
}

/// Cache statistics for reporting
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_hashes: usize,
    pub total_locations: usize,
    pub files_cached: usize,
    pub created_at: u64,
    pub git_commit: Option<String>,
}

/// Get file metadata for cache invalidation
fn get_file_metadata(file_path: &str) -> Result<FileCacheMetadata> {
    let metadata = fs::metadata(file_path)
        .with_context(|| format!("Failed to get metadata for {}", file_path))?;

    let duration = metadata
        .modified()
        .context("Failed to get file modification time")?
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("File mtime is before Unix epoch")?;
    let mtime = duration
        .as_secs()
        .checked_mul(1_000_000_000)
        .and_then(|secs| secs.checked_add(u64::from(duration.subsec_nanos())))
        .context("File mtime overflowed when converting to nanoseconds")?;

    Ok(FileCacheMetadata {
        path: file_path.to_string(),
        mtime,
        size: metadata.len(),
    })
}

/// Get current git commit hash (if in a git repository)
fn get_current_git_commit() -> Option<String> {
    use std::process::Command;

    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::Token;
    use tempfile::TempDir;

    #[test]
    fn test_cache_creation() {
        let cache = HashCache::new(10);
        assert_eq!(cache.version, env!("CARGO_PKG_VERSION"));
        assert!(cache.hash_index.is_empty());
        assert!(cache.file_metadata.is_empty());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut cache = HashCache::new(10);
        let location = CodeLocation {
            file_path: "/test/file.js".to_string(),
            start_line: 1,
            end_line: 10,
            token_offset: Some(0),
            token_length: 50,
            tokens: vec![Token::Keyword("function".to_string())],
            raw_source: "function test() {}".to_string(),
        };

        cache.add_hash(12345, location.clone());

        let results = cache.lookup(12345);
        assert!(results.is_some());
        assert_eq!(results.unwrap().len(), 1);
        assert_eq!(results.unwrap()[0].file_path, "/test/file.js");
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join(".polydup-cache.json");

        let mut cache = HashCache::new(10);
        let location = CodeLocation {
            file_path: "/test/file.js".to_string(),
            start_line: 1,
            end_line: 10,
            token_offset: Some(0),
            token_length: 50,
            tokens: vec![Token::Keyword("function".to_string())],
            raw_source: "function test() {}".to_string(),
        };
        cache.add_hash(12345, location);

        // Save
        cache.save(&cache_path).unwrap();
        assert!(cache_path.exists());

        // Load
        let loaded = HashCache::load(&cache_path).unwrap();
        assert_eq!(loaded.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(loaded.hash_index.len(), 1);
        assert!(loaded.lookup(12345).is_some());
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = HashCache::new(10);

        for i in 0..5 {
            let location = CodeLocation {
                file_path: format!("/test/file{}.js", i),
                start_line: 1,
                end_line: 10,
                token_offset: Some(0),
                token_length: 50,
                tokens: vec![Token::Keyword("function".to_string())],
                raw_source: "function test() {}".to_string(),
            };
            cache.add_hash(i, location);
        }

        let stats = cache.stats();
        assert_eq!(stats.total_hashes, 5);
        assert_eq!(stats.total_locations, 5);
    }

    #[test]
    fn test_invalidate_file() {
        let mut cache = HashCache::new(10);

        let loc1 = CodeLocation {
            file_path: "/test/file1.js".to_string(),
            start_line: 1,
            end_line: 10,
            token_offset: Some(0),
            token_length: 50,
            tokens: vec![Token::Keyword("function".to_string())],
            raw_source: "function test1() {}".to_string(),
        };
        let loc2 = CodeLocation {
            file_path: "/test/file2.js".to_string(),
            start_line: 1,
            end_line: 10,
            token_offset: Some(0),
            token_length: 50,
            tokens: vec![Token::Keyword("function".to_string())],
            raw_source: "function test2() {}".to_string(),
        };

        cache.add_hash(12345, loc1);
        cache.add_hash(67890, loc2);

        assert_eq!(cache.hash_index.len(), 2);

        // Invalidate file1
        cache.invalidate_file("/test/file1.js");

        assert_eq!(cache.hash_index.len(), 1);
        assert!(cache.lookup(12345).is_none());
        assert!(cache.lookup(67890).is_some());
    }

    #[test]
    fn test_invalidate_stale_files_removes_changed_entries() {
        use std::{thread, time::Duration};

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.js");

        std::fs::write(&file_path, "function a() { return 1; }\n").unwrap();

        let mut cache = HashCache::new(3);
        let location = CodeLocation {
            file_path: file_path.to_string_lossy().to_string(),
            start_line: 1,
            end_line: 1,
            token_offset: Some(0),
            token_length: 3,
            tokens: vec![Token::Keyword("function".to_string())],
            raw_source: "function a() { return 1; }".to_string(),
        };
        cache.add_hash(123, location);

        thread::sleep(Duration::from_secs(1));
        std::fs::write(&file_path, "function a() { return 2; }\n").unwrap();

        let removed = cache.invalidate_stale_files();

        assert_eq!(removed.len(), 1);
        assert!(removed.contains(&file_path.to_string_lossy().to_string()));
        assert!(cache.hash_index.is_empty());
        assert!(cache.file_metadata.is_empty());
    }
}
