//! Snapshot testing for dupe-core output formats
//!
//! Uses the `insta` crate to capture and compare output snapshots.
//! If output format changes, tests will fail until snapshots are reviewed and updated.

#[cfg(test)]
mod tests {
    use crate::{CloneType, DuplicateMatch, Scanner};
    use insta::{assert_debug_snapshot, assert_json_snapshot};
    use std::path::PathBuf;

    /// Creates a DuplicateMatch for testing with configurable core fields
    #[allow(clippy::too_many_arguments)]
    fn make_test_duplicate(
        file1: &str,
        file2: &str,
        start_line1: usize,
        start_line2: usize,
        length: usize,
        similarity: f64,
        hash: u64,
        clone_type: CloneType,
    ) -> DuplicateMatch {
        DuplicateMatch {
            file1: file1.to_string(),
            file2: file2.to_string(),
            start_line1,
            start_line2,
            end_line1: None,
            end_line2: None,
            length,
            similarity,
            hash,
            clone_type,
            edit_distance: None,
            suppressed_by_directive: None,
            token_offset1: Some(start_line1),
            token_offset2: Some(start_line2),
            target_length: Some(length),
            duplicate_id: None,
        }
    }

    #[test]
    fn snapshot_duplicate_match_json() {
        let dup = make_test_duplicate(
            "src/main.rs",
            "src/lib.rs",
            10,
            25,
            50,
            0.95,
            0x123456789ABCDEF,
            CloneType::Type2,
        );

        assert_json_snapshot!(dup, @r#"
        {
          "file1": "src/main.rs",
          "file2": "src/lib.rs",
          "start_line1": 10,
          "start_line2": 25,
          "length": 50,
          "similarity": 0.95,
          "hash": 81985529216486895,
          "clone_type": "type-2"
        }
        "#);
    }

    #[test]
    fn snapshot_empty_scan_result() {
        let scanner = Scanner::new();
        let paths = vec![PathBuf::from("nonexistent")];

        let result = scanner.scan(paths);

        // Should succeed with 0 files scanned
        assert!(result.is_ok());
        let mut report = result.unwrap();

        // Duration can vary (0-10ms), normalize for snapshot
        report.stats.duration_ms = 0;

        assert_debug_snapshot!(report, @"
        Report {
            version: None,
            scan_time: None,
            config: None,
            files_scanned: 0,
            functions_analyzed: 0,
            duplicates: [],
            skipped_files: [],
            stats: ScanStats {
                total_lines: 0,
                total_tokens: 0,
                unique_hashes: 0,
                duration_ms: 0,
                suppressed_by_ignore_file: 0,
                suppressed_by_directive: 0,
            },
        }
        ");
    }

    #[test]
    fn snapshot_test_duplicates_report() {
        use std::path::Path;

        // Only run if test_duplicates exists
        if !Path::new("test_duplicates").exists() {
            return;
        }

        let scanner = Scanner::with_config(3, 0.70).unwrap();
        let paths = vec![PathBuf::from("test_duplicates")];

        let result = scanner.scan(paths);
        assert!(result.is_ok());

        let report = result.unwrap();

        // Snapshot the structure (not exact values which may vary)
        assert!(report.files_scanned >= 1, "Should scan at least 1 file");
        assert!(
            report.functions_analyzed >= 1,
            "Should analyze at least 1 function"
        );

        // Snapshot a sample duplicate if found
        if let Some(dup) = report.duplicates.first() {
            assert_json_snapshot!("sample_duplicate", dup, {
                ".hash" => "[hash]",  // Hash values are dynamic
            });
        }
    }

    #[test]
    fn snapshot_scanner_configuration() {
        let scanner = Scanner::with_config(100, 0.95);

        // Scanner doesn't implement Debug, so just verify it was created
        assert!(scanner.is_ok());
    }

    #[test]
    fn snapshot_multiple_duplicates() {
        let duplicates = vec![
            make_test_duplicate("a.rs", "b.rs", 1, 1, 10, 1.0, 0xAAA, CloneType::Type1),
            make_test_duplicate("a.rs", "c.rs", 5, 10, 15, 0.92, 0xBBB, CloneType::Type2),
        ];

        assert_json_snapshot!(duplicates, {
            "[].hash" => "[hash]",  // Redact dynamic hashes
        });
    }
}
