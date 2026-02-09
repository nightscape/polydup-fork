# PolyDup CLI Documentation

Command-line interface documentation for PolyDup.

## Overview

The `polydup-cli` crate provides a standalone command-line tool for duplicate code detection. It wraps the `polydup-core` library with user-friendly CLI parsing, colored output, and JSON export.

## Installation

### From crates.io

```bash
cargo install polydup-cli
polydup --version
```

### From source

```bash
git clone https://github.com/wiesnerbernard/polydup.git
cd polydup
cargo build --release -p polydup-cli
./target/release/polydup --version
```

### Pre-built binaries

Download platform-specific binaries from [GitHub Releases](https://github.com/wiesnerbernard/polydup/releases).

## Usage

### Basic Commands

```bash
# Scan a directory
polydup scan ./src

# Scan multiple directories
polydup scan ./src ./tests

# Custom threshold
polydup scan ./src --threshold 0.85

# JSON output
polydup scan ./src --format json > results.json
```

### Command Reference

#### `polydup scan <PATHS...>`

Scan directories for duplicate code.

**Arguments:**
- `<PATHS...>` - One or more directories to scan

**Options:**
- `-t, --threshold <THRESHOLD>` - Similarity threshold (0.0-1.0, default: 0.9)
- `-b, --min-block-size <SIZE>` - Minimum lines per block (default: 10)
- `-f, --format <FORMAT>` - Output format: text or json (default: text)

**Examples:**

```bash
# Strict duplicate detection (95% similarity)
polydup scan ./src --threshold 0.95

# Lenient detection (70% similarity)
polydup scan ./src --threshold 0.70

# Larger code blocks only
polydup scan ./src --min-block-size 50

# Machine-readable output
polydup scan ./src --format json | jq '.duplicates | length'
```

## Output Formats

### Text (Default)

Human-readable colored output with file paths, line numbers, and similarity scores:

```
Duplicates

src/handler.rs:42-68 <-> src/utils.rs:156-182 (95.2% similar)
  Length: 27 lines
  Hash: 0x7f8a9b3c2d1e4f56

src/parser.rs:89-105 <-> tests/fixtures.rs:23-39 (88.7% similar)
  Length: 17 lines
  Hash: 0x1a2b3c4d5e6f7a8b
```

### JSON

Machine-readable format for scripting and integration:

```json
{
  "files_scanned": 42,
  "functions_analyzed": 187,
  "duplicates": [
    {
      "file1": "src/handler.rs",
      "file2": "src/utils.rs",
      "start_line1": 42,
      "start_line2": 156,
      "length": 27,
      "similarity": 0.952,
      "hash": 9182736455847392086
    }
  ],
  "stats": {
    "total_lines": 0,
    "total_tokens": 15432,
    "unique_hashes": 8721,
    "duration_ms": 156
  }
}
```

## Integration

### CI/CD

Exit with error if duplicates found:

```bash
polydup scan ./src --threshold 0.90 || exit 1
```

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
polydup scan ./src --threshold 0.95 --format json > /tmp/duplicates.json
DUPS=$(jq '.duplicates | length' /tmp/duplicates.json)
if [ "$DUPS" -gt 0 ]; then
    echo "Error: $DUPS duplicates detected"
    exit 1
fi
```

### GitHub Actions

```yaml
- name: Check for duplicates
  run: |
    cargo install polydup-cli
    polydup scan ./src --threshold 0.90
```

## Performance

The CLI is optimized for developer workflows:

- **Parallel processing**: Uses all available CPU cores
- **Incremental scanning**: Only processes changed files (future)
- **Fast startup**: <50ms cold start time
- **Memory efficient**: Streaming token processing

Typical performance:

- Small project (100 files): <1 second
- Medium project (1000 files): ~5 seconds
- Large project (10000 files): ~30 seconds

## Troubleshooting

### No duplicates found but expected some

Try lowering the threshold:

```bash
polydup scan ./src --threshold 0.70
```

### Too many false positives

Increase threshold or block size:

```bash
polydup scan ./src --threshold 0.95 --min-block-size 20
```

### Unsupported file type

Check supported languages: JavaScript, TypeScript, Python, Rust, Vue, Svelte

### Performance issues

For very large codebases, consider:

1. Scanning subdirectories separately
2. Increasing `--min-block-size` to reduce candidates
3. Using `--threshold 0.95` for stricter matching

## Contributing

See the main [README](../../../README.md) for contribution guidelines.
