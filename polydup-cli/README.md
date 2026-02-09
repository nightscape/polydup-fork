# PolyDup CLI

Command-line interface for **PolyDup**, the cross-language duplicate code detector.

## Installation

### From Source

```bash
cd crates/polydup-cli
cargo build --release

# Binary will be at: target/release/polydup
```

### System-wide Installation

```bash
cargo install --path crates/polydup-cli

# Or from the workspace root:
cargo install --path .
```

## Usage

### Basic Scan

```bash
polydup ./src
```

### Scan Multiple Paths

```bash
polydup ./src ./lib ./tests
```

### Adjust Detection Parameters

```bash
# Set minimum block size (default: 50 tokens)
polydup ./src --threshold 30

# Set similarity threshold (default: 0.85 = 85%)
polydup ./src --similarity 0.9

# Combine both
polydup ./src --threshold 30 --similarity 0.9
```

### Exclude Files (e.g., Tests)

By default, PolyDup excludes common test file patterns:
- `**/*.test.{ts,js,tsx,jsx}`
- `**/*.spec.{ts,js,tsx,jsx}`
- `**/__tests__/**`
- `**/*.test.py`

To use **custom exclusions** (replaces defaults):

```bash
# Exclude specific patterns
polydup ./src --exclude "**/*.generated.ts" --exclude "**/*.mock.js"

# Exclude multiple patterns
polydup ./src -e "**/*.test.ts" -e "**/*.spec.js" -e "**/fixtures/**"

# No exclusions (scan everything including tests)
polydup ./src --exclude ""
```

### Output Formats

**Text output (default):**
```bash
polydup ./src
```

Output:
```
Scan Results
═══════════════════════════════════════════════════════════
Files scanned:      4
Functions analyzed: 45
Duplicates found:   0

No duplicates found!
```

**JSON output (for scripting):**
```bash
polydup ./src --format json
```

Output:
```json
{
  "files_scanned": 4,
  "functions_analyzed": 45,
  "duplicates": [],
  "stats": {
    "total_lines": 0,
    "total_tokens": 3665,
    "unique_hashes": 2666,
    "duration_ms": 8
  }
}
```

### Verbose Mode

Show additional performance metrics:

```bash
polydup ./src --verbose
```

Output includes:
- Total tokens processed
- Number of unique hashes
- Scan duration

## Command-Line Options

```
polydup [OPTIONS] <PATHS>...

Arguments:
  <PATHS>...  Paths to scan (files or directories)

Options:
  -f, --format <FORMAT>
          Output format [default: text] [possible values: text, json]

  -t, --threshold <MIN_BLOCK_SIZE>
          Minimum code block size in tokens [default: 50]

  -s, --similarity <SIMILARITY>
          Similarity threshold (0.0-1.0) [default: 0.85]

  -v, --verbose
          Show verbose output

  -h, --help
          Print help

  -V, --version
          Print version
```

## Managing False Positives

PolyDup provides an ignore system to suppress false positives while keeping them documented.

### Adding Ignore Entries

Add a duplicate to the ignore list:

```bash
# Add by ID (from scan output)
polydup ignore add abc123def --files "src/utils.rs:10-30,src/helpers.rs:45-65" --reason "Intentional code reuse"

# Interactive mode
polydup ignore add
# You'll be prompted for files and reason
```

### Listing Ignored Duplicates

```bash
# List all ignored duplicates
polydup ignore list

# Verbose output (shows file paths)
polydup ignore list --verbose

# JSON output for scripting
polydup ignore list --format json
```

Example output:
```
Ignored Duplicates (2)

1. abc123def456
   Reason: Boilerplate initialization code
   Added by: alice
   Added at: 2025-12-26 10:30:15 UTC
   Files: 2 file(s)

2. xyz789abc123
   Reason: Required by framework convention
   Added by: bob
   Added at: 2025-12-26 11:45:30 UTC
   Files: 3 file(s)
```

### Removing Ignore Entries

```bash
# Remove by ID
polydup ignore remove abc123def456
```

### Ignore File Format

Ignored duplicates are stored in `.polydup-ignore` (TOML format):

```toml
version = 1

[[ignores]]
id = "abc123def456"
reason = "Intentional code reuse"
added_by = "alice"
added_at = "2025-12-26T10:30:15Z"

[[ignores.files]]
file = "src/utils.rs"
start_line = 10
end_line = 30

[[ignores.files]]
file = "src/helpers.rs"
start_line = 45
end_line = 65
```

**Tip**: Commit `.polydup-ignore` to version control to share ignore decisions with your team!

## Git-Diff Mode (PR Review)

Scan only files changed in a git diff range - perfect for PR checks:

```bash
# Scan files changed in current branch vs main
polydup scan . --git-diff origin/main..HEAD

# Scan files changed in last commit
polydup scan . --git-diff HEAD~1..HEAD

# Scan with custom similarity threshold
polydup scan . --git-diff main..feature-branch --similarity 0.9
```

### How It Works

1. **Fast**: Only scans files in your diff (10-100x faster for large repos)
2. **Smart**: Scans entire codebase but reports only duplicates involving changed files
3. **Accurate**: Detects when changed code duplicates with unchanged code

**Example Output:**
```bash
ℹ Git-Diff Mode: Only scanning files changed in origin/main..HEAD
  Git diff filter: Added 3 file(s) -> Modified/Renamed 2 file(s)
  Changed files (2):
    • src/handler.rs
    • src/utils.rs

  Git-diff filter: 4 duplicate(s) involve changed files
```

### Combined with Ignore Rules and Directives

Git-diff mode works seamlessly with ignore management:

```bash
# PR check with directives
polydup scan . --git-diff origin/main..HEAD --enable-directives

# PR check with ignore rules loaded from .polydup-ignore
polydup scan . --git-diff HEAD~1..HEAD --verbose
```

**CI/CD Example:**
```yaml
# .github/workflows/pr-check.yml
- name: Check for duplicates in PR
  run: polydup scan . --git-diff origin/${{ github.base_ref }}..HEAD
  # Only fails if new duplicates introduced in this PR
```

**Benefits:**
- ✅ Focuses review on relevant changes
- ✅ Respects existing ignore rules
- ✅ Works with inline directives
- ✅ No baseline files to manage

## Exit Codes

- **0**: No duplicates found
- **1**: Duplicates found (or error occurred)

This allows usage in CI/CD pipelines:

```bash
#!/bin/bash
if ! polydup ./src --threshold 100; then
    echo "❌ Duplicates detected!"
    exit 1
fi
echo "No duplicates!"
```

## Examples

### CI/CD Integration

**GitHub Actions:**

```yaml
name: Check Duplicates

on: [push, pull_request]

jobs:
  check-dupes:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Install PolyDup
        run: cargo install --path crates/polydup-cli

      - name: Check for duplicates
        run: |
          polydup ./src --threshold 50 --similarity 0.85 --format json > duplicates.json

      - name: Upload results
        uses: actions/upload-artifact@v3
        if: failure()
        with:
          name: duplicate-report
          path: duplicates.json
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

echo "Checking for duplicate code..."
if ! polydup ./src --threshold 100 --similarity 0.9; then
    echo "❌ Large code duplicates detected!"
    echo "Review the duplicates above and consider refactoring."
    exit 1
fi
```

### Makefile Integration

```makefile
.PHONY: check-dupes
check-dupes:
	@echo "Scanning for duplicates..."
	@polydup ./src ./lib --threshold 50 --similarity 0.85

.PHONY: dupes-json
dupes-json:
	@polydup ./src --format json > duplicates.json
	@echo "Report saved to duplicates.json"
```

### Shell Script for Multiple Projects

```bash
#!/bin/bash
# scan-all-projects.sh

projects=(
    "project1/src"
    "project2/lib"
    "project3/backend"
)

for project in "${projects[@]}"; do
    echo "Scanning $project..."
    polydup "$project" --format json > "${project//\//-}-report.json"
done

echo "All scans complete!"
```

## Performance Tuning

### Fast Scan (Lower Accuracy)

```bash
# Large block size = fewer comparisons = faster
polydup ./src --threshold 100 --similarity 0.7
```

### Thorough Scan (Higher Accuracy)

```bash
# Small block size = more comparisons = slower but catches smaller duplicates
polydup ./src --threshold 20 --similarity 0.95
```

### Recommended Settings

| Use Case | Threshold | Similarity |
|----------|-----------|------------|
| **Quick check** | 100 | 0.85 |
| **Standard scan** | 50 | 0.85 |
| **Thorough analysis** | 30 | 0.90 |
| **Refactoring prep** | 20 | 0.95 |

## Troubleshooting

### No Duplicates Found (But You Expected Some)

- **Lower the threshold**: Try `--threshold 20` to catch smaller duplicates
- **Lower similarity**: Try `--similarity 0.7` for looser matching
- **Check file types**: Only Rust, Python, and JavaScript/TypeScript are supported

### Too Many False Positives

- **Raise the threshold**: Try `--threshold 100` to only catch large duplicates
- **Raise similarity**: Try `--similarity 0.95` for stricter matching

### Slow Performance

- **Increase threshold**: Larger blocks = fewer comparisons
- **Scan fewer files**: Be more specific with paths
- **Use release build**: `cargo build --release` (already done if installed)

## Supported Languages

- **Rust**: `.rs` files
- **Python**: `.py` files
- **JavaScript/TypeScript**: `.js`, `.jsx`, `.ts`, `.tsx` files

More languages coming soon!

## Algorithm

PolyDup uses:
1. **Tree-sitter** for AST-based parsing
2. **Token normalization** (identifiers → `$$ID`, strings → `$$STR`, numbers → `$$NUM`)
3. **Rabin-Karp rolling hash** with window size 50
4. **Parallel processing** via Rayon for multi-core performance

See [architecture-research.md](../../docs/architecture-research.md) for details.

## License

MIT OR Apache-2.0

## Links

- **Core Library**: [polydup-core](../polydup-core)
- **Node.js Bindings**: [polydup-node](../polydup-node)
- **Python Bindings**: [polydup-py](../polydup-py)
- **GitHub**: https://github.com/wiesnerbernard/polydup
