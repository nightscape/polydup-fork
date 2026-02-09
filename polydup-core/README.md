# PolyDup

[![Crates.io](https://img.shields.io/crates/v/polydup.svg)](https://crates.io/crates/polydup)
[![npm](https://img.shields.io/npm/v/polydup.svg)](https://www.npmjs.com/package/polydup)
[![PyPI](https://img.shields.io/pypi/v/polydup.svg)](https://pypi.org/project/polydup/)
[![GitHub Action](https://img.shields.io/badge/Action-v0.2.1-blue.svg?logo=github)](https://github.com/wiesnerbernard/polydup-action)
[![CI](https://github.com/wiesnerbernard/polydup/actions/workflows/ci.yml/badge.svg)](https://github.com/wiesnerbernard/polydup/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-66.3%25-yellow)](./coverage/tarpaulin-report.html)
[![Tests](https://img.shields.io/badge/tests-82%20passing-success)](https://github.com/wiesnerbernard/polydup/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

Cross-language duplicate code detector powered by Tree-sitter and Rust.

## Features

- **Blazing Fast**: Parallel processing with Rabin-Karp rolling hash algorithm (up to 10x faster than regex-based detectors)
- **Cross-Language**: JavaScript, TypeScript, Python, Rust, Vue, Svelte (more coming)
- **Accurate**: Tree-sitter AST parsing for semantic-aware detection (eliminates false positives from comments/strings)
- **Multi-Platform**: CLI, Node.js npm package, Python pip package, Rust library
- **CI/CD Ready**: Official GitHub Action with automatic PR comments and git-diff mode
- **Configurable**: Adjust thresholds and block sizes for your needs
- **Efficient**: Zero-copy FFI bindings for minimal overhead (passes file paths, not contents)

## Performance

PolyDup includes a **hash cache system** that dramatically accelerates duplicate detection in CI/CD workflows:

| Mode | Small (1K LOC) | Medium (10K LOC) | Large (100K LOC) |
|------|----------------|------------------|------------------|
| Full scan | ~50ms | ~500ms | ~5s |
| Git-diff (no cache) | ~30ms | ~300ms | ~3s |
| Git-diff (with cache) | ~15ms | ~30ms | ~50ms |

**Key benefits:**
- **10-100x faster** CI runs for large codebases
- Cache persists across CI runs (GitHub Actions cache supported)
- Only changed files are scanned, hashes looked up in cache

**Quick start:**
```bash
# Build cache (one-time, ~0.5s for typical codebases)
polydup cache build

# Fast incremental scans using cache
polydup scan . --git-diff origin/main..HEAD
```

See [docs/caching.md](docs/caching.md) for detailed performance characteristics and CI integration patterns.

## Architecture

**Shared Core Architecture**: All duplicate detection logic lives in Rust, exposed via FFI bindings.

```
┌─────────────────────────────────────────────┐
│           polydup-core (Rust)               │
│  • Tree-sitter parsing                      │
│  • Rabin-Karp hashing                       │
│  • Parallel file scanning                   │
│  • Duplicate detection                      │
└─────────────────────────────────────────────┘
          ▲          ▲          ▲
          │          │          │
    ┌─────┴───┐  ┌───┴────┐  ┌─┴─────┐
    │ CLI     │  │ Node.js│  │ Python│
    │ (Rust)  │  │(napi-rs)│  │(PyO3) │
    └─────────┘  └────────┘  └───────┘
```

**Crates:**
- **polydup-core**: Pure Rust library with Tree-sitter parsing, hashing, and reporting
- **polydup** (CLI): Standalone CLI tool (`cargo install polydup`)
- **polydup-node**: Node.js library bindings via napi-rs (`npm install polydup`)
- **polydup-py**: Python library bindings via PyO3 (`pip install polydup`)

## Installation

> **Important**: PolyDup is available in multiple forms for different use cases:
> - **CLI Tool**: `cargo install polydup` - Command-line scanning
> - **Python Library**: `pip install polydup` - Python API bindings (NOT a CLI)
> - **Node.js Library**: `npm install polydup` - Node.js API bindings (NOT a CLI)
>
> If you want to run `polydup` from the command line, use `cargo install polydup`.

### GitHub Action (Easiest for CI/CD) 🚀

The fastest way to add duplicate detection to your workflow:

```yaml
name: Code Quality

on:
  pull_request:
    branches: [ main ]

permissions:
  contents: read
  pull-requests: write  # Required for PR comments

jobs:
  duplicate-detection:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for git-diff mode

      - uses: wiesnerbernard/polydup-action@v0.3.0
        with:
          threshold: 50
          similarity: '0.85'
          fail-on-duplicates: true
```

**Benefits:**
- 🚀 10-100x faster (only scans changed files in PR)
- 💬 Automatic PR comments with duplicate reports
- ✅ Zero configuration needed
- 🔒 Secure (no data leaves your repository)
- ⚡ Fast startup with binary caching (~5s after first run)

#### Action Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `threshold` | `50` | Minimum code block size in tokens |
| `similarity` | `0.85` | Similarity threshold (0.0-1.0) |
| `fail-on-duplicates` | `true` | Fail the check if duplicates found |
| `format` | `text` | Output format: `text` or `json` |
| `base-ref` | auto | Base git reference (auto-detects from PR) |
| `github-token` | - | Token for PR comments |
| `comment-on-pr` | `true` | Post results as PR comment |

#### Action Outputs

| Output | Description |
|--------|-------------|
| `duplicates-found` | Number of duplicate code blocks found |
| `files-scanned` | Number of files scanned |
| `exit-code` | Exit code (0 = no duplicates, 1 = duplicates) |

#### Using Outputs in Workflows

```yaml
- uses: wiesnerbernard/polydup-action@v0.2.1
  id: polydup
  with:
    fail-on-duplicates: false
    github-token: ${{ secrets.GITHUB_TOKEN }}

- name: Check results
  run: |
    echo "Files scanned: ${{ steps.polydup.outputs.files-scanned }}"
    echo "Duplicates found: ${{ steps.polydup.outputs.duplicates-found }}"
    if [ "${{ steps.polydup.outputs.duplicates-found }}" -gt 10 ]; then
      echo "Too many duplicates!"
      exit 1
    fi
```

#### Example PR Comment

When duplicates are found, the action posts a comment like:

```
## PolyDup Duplicate Code Report

**Found 3 duplicate code block(s)**

- Files scanned: 12
- Threshold: 50 tokens
- Similarity: 0.85

<details>
<summary>View Details</summary>
[Detailed scan output...]
</details>

**Tip**: Consider refactoring duplicated code to improve maintainability.
```

See [polydup-action](https://github.com/wiesnerbernard/polydup-action) for full documentation.

---

### Manual CI Installation (Recommended Until v0.3.0) ⚡

For production CI/CD, install the CLI directly in your workflow:

```yaml
name: Code Quality

on:
  pull_request:
    branches: [ main ]

jobs:
  duplicate-detection:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # For git-diff mode

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache polydup binary
        uses: actions/cache@v4
        with:
          path: ~/.cargo/bin/polydup
          key: ${{ runner.os }}-polydup-v0.8.1

      - name: Install polydup
        run: cargo install polydup --locked

      - name: Scan for duplicates
        run: |
          polydup scan . \
            --git-diff origin/${{ github.base_ref }}..HEAD \
            --threshold 50 \
            --similarity 0.85 \
            --format text
```

**Benefits:**
- ✅ Git-diff mode works correctly (10-100x faster)
- ✅ Reliable, production-tested
- ⚡ Fast with caching (~30s first run, ~5s cached)

---

### Rust CLI (For Local Development)

The fastest way to use PolyDup locally is via the CLI tool:

```bash
# Install from crates.io
cargo install polydup

# Verify installation
polydup --version

# Scan for duplicates
polydup scan ./src
```

**System Requirements:**
- Rust 1.70+ (if building from source)
- macOS, Linux, or Windows

> **Note**: Homebrew tap coming soon! (`brew install polydup`)

**Pre-built Binaries:**

Download pre-compiled binaries from [GitHub Releases](https://github.com/wiesnerbernard/polydup/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/wiesnerbernard/polydup/releases/latest/download/polydup-macos-aarch64 -o polydup
chmod +x polydup
sudo mv polydup /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/wiesnerbernard/polydup/releases/latest/download/polydup-macos-x86_64 -o polydup
chmod +x polydup
sudo mv polydup /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/wiesnerbernard/polydup/releases/latest/download/polydup-linux-x86_64 -o polydup
chmod +x polydup
sudo mv polydup /usr/local/bin/

# Linux (x86_64 static - musl)
curl -L https://github.com/wiesnerbernard/polydup/releases/latest/download/polydup-linux-x86_64-musl -o polydup
chmod +x polydup
sudo mv polydup /usr/local/bin/

# Windows (x86_64)
# Download polydup-windows-x86_64.exe from releases page and add to PATH
```

### Node.js/npm (Library Only)

> **Note**: This is a **library package** for integrating duplicate detection into Node.js applications.
> It does NOT provide a CLI. For command-line usage, use `cargo install polydup`.

Install as a project dependency:

```bash
npm install polydup
```

**Requirements:** Node.js 16+ on macOS (Intel/ARM), Windows (x64), or Linux (x64)

**Usage:**

```javascript
const { findDuplicates } = require('polydup');

const duplicates = findDuplicates(
  ['src/', 'tests/'],  // Paths to scan
  10,                  // Minimum block size (lines)
  0.85                 // Similarity threshold (0.0-1.0)
);

console.log(`Found ${duplicates.length} duplicates`);
duplicates.forEach(dup => {
  console.log(`${dup.file1}:${dup.start_line1} ↔ ${dup.file2}:${dup.start_line2}`);
  console.log(`Similarity: ${(dup.similarity * 100).toFixed(1)}%`);
});
```

### Python/pip (Library Only)

> **Note**: This is a **library package** for integrating duplicate detection into Python applications.
> It does NOT provide a CLI. For command-line usage, use `cargo install polydup`.
>
> Running `python -m polydup` will display installation guidance.

Install from PyPI:

```bash
# Using pip
pip install polydup

# Using uv (recommended for faster installs)
uv pip install polydup
```

**Requirements:** Python 3.8-3.12 on macOS (Intel/ARM), Windows (x64), or Linux (x64)

**Usage:**

```python
import polydup

# Scan for duplicates
duplicates = polydup.find_duplicates(
    paths=['src/', 'tests/'],
    min_block_size=10,
    similarity_threshold=0.85
)

print(f"Found {len(duplicates)} duplicates")
for dup in duplicates:
    print(f"{dup['file1']}:{dup['start_line1']} ↔ {dup['file2']}:{dup['start_line2']}")
    print(f"Similarity: {dup['similarity']*100:.1f}%")
```

### Rust Library

Use the core library in your Rust project:

```toml
[dependencies]
polydup-core = "0.1"
```

```rust
use polydup_core::{Scanner, find_duplicates};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let scanner = Scanner::with_config(10, 0.85)?;
    let report = scanner.scan(vec![PathBuf::from("src")])?;

    println!("Found {} duplicates", report.duplicates.len());
    Ok(())
}
```

## Building from Source

### CLI
```bash
cargo build --release -p polydup
./target/release/polydup scan ./src
```

### Node.js
```bash
cd crates/polydup-node
npm install
npm run build
```

### Python
```bash
cd crates/polydup-py
maturin develop
python -c "import polydup; print(polydup.version())"
```

## CLI Usage

### Quick Start with `polydup init`

The fastest way to get started is with the interactive initialization wizard:

```bash
# Run the initialization wizard
polydup init

# Non-interactive mode (use defaults)
polydup init --yes

# Force overwrite existing configuration
polydup init --force

# Only generate CI/CD configuration (skip .polyduprc.toml)
polydup init --ci-only
```

The wizard will:
- **Auto-detect your project environment** (Node.js, Rust, Python, etc.)
- **Generate `.polyduprc.toml`** with environment-specific defaults
- **Create GitHub Actions workflow** (optional)
- **Show install instructions** tailored to your environment
- **Provide next steps** for local usage

**Example workflow:**

```bash
$ polydup init

PolyDup Initialization Wizard
=============================

Detected environments:
  - Node.js
  - Python

✔ Select similarity threshold: Standard (0.85)
✔ Select minimum block size: Medium (50 lines)
✔ Add custom exclude patterns? · No
✔ Would you like to create a GitHub Actions workflow? · Yes

Configuration saved to: .polyduprc.toml
GitHub Actions workflow created: .github/workflows/polydup.yml

Next Steps:
  1. Install: npm install -g polydup
  2. Scan: polydup scan ./src
```

### Configuration File (`.polyduprc.toml`)

After running `polydup init`, you'll have a `.polyduprc.toml` file:

```toml
[scan]
min_block_size = 50
similarity_threshold = 0.85

[scan.exclude]
patterns = [
    "**/node_modules/**",
    "**/__pycache__/**",
    "**/*.test.js",
    "**/*.test.py",
]

[output]
format = "text"
verbose = false

[ci]
enabled = false
fail_on_duplicates = true
```

**Configuration Discovery:**
- PolyDup searches for `.polyduprc.toml` in current directory and parent directories
- CLI arguments override config file settings
- Perfect for monorepos with shared configuration at root

### Basic Commands

```bash
# Scan a directory
polydup scan ./src

# Scan multiple directories
polydup scan ./src ./tests ./lib

# Custom threshold (0.0-1.0, higher = stricter)
polydup scan ./src --threshold 0.85

# Adjust minimum block size (lines)
polydup scan ./src --min-block-size 50

# JSON output for scripting
polydup scan ./src --format json > duplicates.json
```

### Examples

**Quick scan for severe duplicates:**
```bash
polydup scan ./src --threshold 0.95 --min-block-size 20
```

**Deep scan for similar code:**
```bash
polydup scan ./src --threshold 0.70 --min-block-size 5
```

**Scan specific file types:**
```bash
# PolyDup auto-detects: .rs, .js, .ts, .jsx, .tsx, .py, .vue, .svelte
polydup scan ./src  # Scans all supported languages
```

**CI/CD integration:**
```bash
# Exit with error if duplicates found
polydup scan ./src --threshold 0.90 || exit 1
```

### Output Formats

**Text (default):** Human-readable colored output with file paths, line numbers, and similarity scores

**JSON:** Machine-readable format for scripting and tooling integration
```bash
polydup scan ./src --format json | jq '.duplicates | length'
```

### Commands

PolyDup supports the following subcommands:

| Command | Description | Example |
|---------|-------------|---------|
| `scan` | Scan for duplicate code (default command) | `polydup scan ./src` |
| `init` | Interactive setup wizard | `polydup init` |
| `config` | Manage configuration file | `polydup config validate` |
| `cache` | Manage hash cache for fast git-diff scans | `polydup cache build` |
| `ignore` | Manage ignored duplicates | `polydup ignore list` |

See [Ignore System Guide](docs/guides/ignore-system.md) for comprehensive documentation on managing false positives.

**Scan Command Options:**

The `scan` command accepts all options listed below. When no subcommand is specified, `scan` is assumed for backward compatibility.

```bash
# These are equivalent:
polydup scan ./src --threshold 0.95
polydup ./src --threshold 0.95
```

**Init Command Options:**

| Option | Description |
|--------|-------------|
| `--yes`, `-y` | Skip interactive prompts, use defaults |
| `--force` | Overwrite existing `.polyduprc.toml` |
| `--ci-only` | Only generate CI/CD configuration (skip `.polyduprc.toml`) |

**CI-Only Mode:**

Use `--ci-only` to add or update CI/CD workflows without modifying your existing configuration:

```bash
# Interactive: Choose your CI platform
polydup init --ci-only

# Non-interactive: Generate GitHub Actions workflow
polydup init --ci-only --yes
```

Supported CI platforms:
- **GitHub Actions** (`.github/workflows/polydup.yml`)
- **GitLab CI** (`.gitlab-ci.yml`)
- **Azure Pipelines** (`azure-pipelines.yml`)
- **Jenkins** (`Jenkinsfile`)

**Config Command:**

Manage and validate your `.polyduprc.toml` configuration:

```bash
# Validate configuration
polydup config validate

# Show configuration summary
polydup config show

# Show configuration file path
polydup config path
```

**Cache Command:**

Manage the hash cache for fast git-diff duplicate detection:

```bash
# Build cache for entire codebase (run once, takes ~0.5-2s)
polydup cache build

# Build with custom threshold and verbose output
polydup cache build --min-tokens 100 -v

# View cache statistics
polydup cache info

# Clear the cache
polydup cache clear
```

**How Caching Works:**
1. **Build**: `polydup cache build` scans all files and creates `.polydup-cache.json` with a hash index
2. **Scan**: `polydup scan --git-diff <range>` automatically uses the cache if it exists
3. **Result**: 10-1000x faster scans (milliseconds instead of seconds) for incremental changes

**When to Use:**
- ✅ Running git-diff mode in CI/CD (automatic with GitHub Action)
- ✅ Frequent local scans during development
- ✅ Large codebases (>10K LOC) where full scans are slow

The cache is automatically invalidated when files are modified (based on mtime/size). See [Caching Guide](docs/caching.md) for details.

### CLI Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--threshold` | float | 0.9 | Similarity threshold (0.0-1.0) |
| `--min-block-size` | int | 10 | Minimum lines per code block |
| `--format` | text\|json | text | Output format |
| `--output` | path | - | Write report to file |
| `--only-type` | types | - | Filter by clone type (type-1, type-2, type-3) |
| `--exclude-type` | types | - | Exclude clone types |
| `--group-by` | criterion | - | Group results (file, similarity, type, size) |
| `--verbose` | flag | false | Show performance statistics |
| `--no-color` | flag | false | Disable colored output |
| `--debug` | flag | false | Enable debug mode with detailed traces |
| `--enable-type3` | flag | false | Enable Type-3 gap-tolerant detection |
| `--save-baseline` | path | - | Save scan results as baseline for future comparisons |
| `--compare-to` | path | - | Compare against baseline (show only new duplicates) |
| `--git-diff` | range | - | Only scan files changed in git diff range (e.g., `origin/main..HEAD`) ⚡ **Recommended for CI** |

**Performance Tip**: For large codebases (>50K LOC), increase `--min-block-size` to 20-50 for faster scans with less noise.

### Baseline/Snapshot Mode

**The most powerful feature for CI/CD**: Block new duplicates without failing on legacy code.

#### Use Case: "We have existing duplicates, but block any NEW ones"

Many codebases have legacy duplication that's not worth fixing immediately. Baseline mode lets you:
- ✅ Accept existing duplicates as-is
- ✅ Fail CI/CD only when new duplicates are introduced
- ✅ Gradually reduce technical debt without blocking development

#### Quick Start

**Step 1: Create baseline from your main branch**

```bash
# On main/master branch: capture current state
polydup scan ./src --save-baseline .polydup-baseline.json
git add .polydup-baseline.json
git commit -m "chore: add duplication baseline"
```

**Step 2: Use in CI/CD to block new duplicates**

```yaml
# .github/workflows/polydup.yml
- name: Check for new duplicates
  run: |
    polydup scan ./src --compare-to .polydup-baseline.json
    # Exits with code 1 if NEW duplicates found
    # Exits with code 0 if no new duplicates (CI passes)
```

**Step 3: See it in action on a PR**

```bash
# Developer adds duplicate code in feature branch
polydup scan ./src --compare-to .polydup-baseline.json
```

Output:
```
ℹ Comparing against baseline: .polydup-baseline.json
  11 total duplicates, 3 new since baseline

Duplicates
═══════════════════════════════════════════

1. Type-2 (renamed) | Similarity: 100.0% | Length: 59 tokens
   ├─ src/new-feature.ts:12
   └─ src/utils.ts:45

❌ 3 new duplicates found since baseline
```

Exit code: `1` (CI fails, PR blocked)

#### Advanced Baseline Workflows

**Incremental improvement: Update baseline after cleanup**

```bash
# Team cleans up 10 duplicates
polydup scan ./src --save-baseline .polydup-baseline.json
git add .polydup-baseline.json
git commit -m "chore: update baseline after duplication cleanup"
```

**Combining with filters**

```bash
# Save baseline excluding Type-3 (noisy matches)
polydup scan ./src --exclude-type type-3 --save-baseline baseline.json

# Only block new Type-1 and Type-2 duplicates
polydup scan ./src --only-type type-1,type-2 --compare-to baseline.json
```

**Manual review mode**

```bash
# See what duplicates are NEW (no CI failure, just info)
polydup scan ./src --compare-to baseline.json --format json \
  | jq '.duplicates | length'
```

#### Real-world Example: PR Comments

Use with GitHub Actions to comment on PRs:

```yaml
- name: Check duplicates
  id: polydup
  run: |
    OUTPUT=$(polydup scan ./src --compare-to .polydup-baseline.json --format json || true)
    NEW_COUNT=$(echo "$OUTPUT" | jq '.duplicates | length')
    echo "new_duplicates=$NEW_COUNT" >> $GITHUB_OUTPUT

- name: Comment on PR
  if: steps.polydup.outputs.new_duplicates > 0
  uses: actions/github-script@v7
  with:
    script: |
      github.rest.issues.createComment({
        issue_number: context.issue.number,
        owner: context.repo.owner,
        repo: context.repo.repo,
        body: '⚠️ This PR introduces ${{ steps.polydup.outputs.new_duplicates }} new code duplicates. Please refactor before merging.'
      })
```

### Git-Diff Mode 🚀 **RECOMMENDED FOR CI/CD**

**The fastest, simplest way to check for duplicates in Pull Requests**.

#### Why Git-Diff Mode?

Advantages over Baseline Mode:
- ✅ **Fast and accurate** - Scans all files but filters results to changed files
- ✅ **No file management** - No baseline file to commit/sync
- ✅ **Universal** - Works on all CI platforms (GitHub, GitLab, Jenkins, etc.)
- ✅ **Simpler** - Just specify a git range, no baseline setup needed
- ✅ **Accurate** - Works in shallow clones (common in CI environments)

#### Quick Start

**Single command to check duplicates in a PR:**

```bash
# Scan only files changed between main and current branch
polydup scan . --git-diff origin/main..HEAD
```

**CI/CD Integration:**

**GitHub Actions (Recommended):**

Use the official [PolyDup GitHub Action](https://github.com/wiesnerbernard/polydup-action) for the best experience:

```yaml
# .github/workflows/polydup.yml
name: PolyDup Duplicate Detection

on:
  pull_request:
    branches: [ main, master ]

jobs:
  duplicate-check:
    runs-on: ubuntu-latest
    name: Detect Duplicate Code

    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - uses: wiesnerbernard/polydup-action@v1
        with:
          fail-on-duplicates: true
          comment-on-pr: true
```

**Features:**
- ⚡ Automatically uses git-diff mode
- 💬 Posts results as PR comments
- ✅ Proper fail/pass based on duplicates found
- 🔧 No manual installation required

**Manual Installation:**

```yaml
jobs:
  duplicate-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install PolyDup
        run: cargo install polydup

      - name: Check for duplicates in PR
        run: |
          polydup scan . --git-diff origin/main..HEAD
          # Exits with code 1 if duplicates found
```

#### Common Usage Patterns

**1. Check uncommitted changes:**

```bash
polydup scan . --git-diff HEAD
```

**2. Compare branches:**

```bash
polydup scan . --git-diff main..feature-branch
```

**3. Check last N commits:**

```bash
polydup scan . --git-diff HEAD~3..HEAD
```

**4. JSON output for tooling:**

```bash
polydup scan . --git-diff origin/main..HEAD --format json
```

#### How It Works

1. Runs `git diff --name-only --diff-filter=ACMR <range>`
2. Gets list of Added, Copied, Modified, Renamed files
3. Filters out deleted files (can't scan what doesn't exist)
4. Scans all files in the codebase to build complete hash cache
5. Filters results to show only duplicates involving changed files
6. Exits with code 1 if duplicates found, 0 otherwise

#### Real-World CI Example

```bash
# Before: Scanning entire codebase (50K LOC, 500 files)
polydup scan ./src  # 🐢 Takes 15-20 seconds

# After: Git-diff mode (PR with 5 changed files)
polydup scan . --git-diff origin/main..HEAD  # ⚡ Takes 0.5-1 second
```

**10-100x speedup** on large codebases with focused PRs!

#### Edge Cases Handled

- ✅ **Deleted files** - Automatically filtered out (can't scan deleted code)
- ✅ **Renamed files** - Detected via `--diff-filter=R`, scanned correctly
- ✅ **Shallow clones** - Works in CI environments with `fetch-depth: 0`
- ✅ **Invalid ranges** - Clear error message with suggestions

#### When to Use Git-Diff vs Baseline

**Use Git-Diff Mode (recommended):**
- ✅ Pull Request checks in CI/CD
- ✅ Fast feedback on code changes
- ✅ Git-based workflows

**Use Baseline Mode when:**
- ✅ Non-git workflows (Perforce, SVN, etc.)
- ✅ Tracking historical debt reduction
- ✅ Explicit acceptance of legacy duplicates

### Advanced Features

#### Filtering by Clone Type

Focus on specific types of duplicates for targeted refactoring:

```bash
# Show only exact duplicates (highest priority)
polydup scan ./src --only-type type-1

# Show only renamed duplicates
polydup scan ./src --only-type type-2

# Show both Type-1 and Type-2
polydup scan ./src --only-type type-1,type-2

# Exclude noisy Type-3 matches
polydup scan ./src --exclude-type type-3
```

**Use cases:**
- `--only-type type-1`: Quick wins for immediate refactoring
- `--only-type type-2`: Identify abstraction opportunities
- `--exclude-type type-3`: Reduce false positives in large codebases

#### Grouping Results

Organize duplicates for different workflows:

```bash
# Group by file (refactoring prioritization)
polydup scan ./src --group-by file

# Group by similarity (quality triage)
polydup scan ./src --group-by similarity

# Group by clone type (targeted cleanup)
polydup scan ./src --group-by type

# Group by size (impact analysis)
polydup scan ./src --group-by size
```

**Grouping strategies:**
- **file**: See which files need refactoring most
- **similarity**: Prioritize high-confidence matches
- **type**: Handle Type-1 separately from Type-2
- **size**: Focus on large duplicates for maximum impact

#### Output Options

```bash
# Save report to file
polydup scan ./src --output duplicates.txt

# JSON for CI/CD pipelines
polydup scan ./src --format json --output report.json

# Disable colors for logs
polydup scan ./src --no-color

# Or use NO_COLOR environment variable
NO_COLOR=1 polydup scan ./src

# Verbose mode with performance stats
polydup scan ./src --verbose
```

#### Debug Mode

Enhanced error messages with actionable suggestions:

```bash
# Enable debug mode for troubleshooting
polydup scan ./src --debug

# Debug mode shows:
# - Current working directory
# - File access permissions
# - Parser errors with context
# - Configuration validation details
```

**Example error output:**
```
Error: Path does not exist: /nonexistent/path

Suggestion: Check the path spelling and ensure it exists
  Example: polydup scan ./src
           polydup scan /absolute/path/to/project

Debug Info: Current directory: /Users/you/project
```

#### Combining Features

Mix and match for powerful workflows:

```bash
# High-priority refactoring targets
polydup scan ./src \
  --only-type type-1 \
  --group-by file \
  --min-block-size 50 \
  --output refactor-priorities.txt

# CI/CD duplicate gate
polydup scan ./src \
  --threshold 0.95 \
  --exclude-type type-3 \
  --format json \
  --output duplicates.json

# Deep analysis with verbose stats
polydup scan ./src \
  --enable-type3 \
  --group-by similarity \
  --verbose

# Quick triage without noise
polydup scan ./src \
  --only-type type-1,type-2 \
  --group-by type \
  --no-color
```

### Dashboard Output

PolyDup provides a professional dashboard with actionable insights:

```
╔═══════════════════════════════════════════════════════════╗
║                      Scan Results                         ║
╠═══════════════════════════════════════════════════════════╣
║ Files scanned:       142                                  ║
║ Functions analyzed:  287                                  ║
║ Duplicates found:    15                                   ║
║ Estimated savings:   ~450 lines                           ║
╠═══════════════════════════════════════════════════════════╣
║ Clone Type Breakdown:                                     ║
║   Type-1 (exact):    5 groups  │ Critical priority       ║
║   Type-2 (renamed):  8 groups  │ High priority           ║
║   Type-3 (modified): 2 groups  │ Medium priority         ║
╠═══════════════════════════════════════════════════════════╣
║ Top Offenders:                                            ║
║   1. src/handlers.ts      8 duplicates                    ║
║   2. lib/utils.ts         5 duplicates                    ║
║   3. components/Form.tsx  3 duplicates                    ║
╚═══════════════════════════════════════════════════════════╝

Duplicate #1 (Type-2: Renamed identifiers)
  Location: src/auth.ts:45-68 ↔ src/admin.ts:120-143
  Similarity: 94.2% | Length: 24 lines
  ...
```

**Dashboard features:**
- **Lines saved estimation**: Potential code reduction
- **Top offenders**: Files needing most attention
- **Similarity range**: Quality distribution (min-max)
- **Priority labels**: Critical (Type-1), High (Type-2), Medium (Type-3)

### Exit Codes

PolyDup uses semantic exit codes for CI/CD integration:

| Exit Code | Meaning | Use Case |
|-----------|---------|----------|
| `0` | No duplicates found | Clean codebase ✓ |
| `1` | Duplicates detected | Quality gate (expected) |
| `2` | Error occurred | Configuration/runtime issue |

**CI/CD examples:**

```bash
# Fail build if duplicates found
polydup scan ./src || exit 1

# Warning only (report but don't fail)
polydup scan ./src || true

# Strict quality gate (fail on any duplicates)
if polydup scan ./src --threshold 0.95; then
  echo "No duplicates found"
else
  echo "⚠️ Duplicates detected - please refactor"
  exit 1
fi
```

## Supported Languages

- **JavaScript/TypeScript**: `.js`, `.jsx`, `.ts`, `.tsx`
- **Python**: `.py`
- **Rust**: `.rs`
- **Vue**: `.vue`
- **Svelte**: `.svelte`

More languages coming soon (Java, Go, C/C++, Ruby, PHP)

## Clone Types

PolyDup classifies duplicates into different types based on the International Workshop on Software Clones (IWSC) taxonomy:

### Type-1: Exact Clones
Identical code fragments except for whitespace, comments, and formatting.

**Example:**
```javascript
// File 1
function calculateTotal(items) {
    let sum = 0;
    for (let i = 0; i < items.length; i++) {
        sum += items[i].price;
    }
    return sum;
}

// File 2 (Type-1 clone - only formatting differs)
function calculateTotal(items) {
  let sum = 0;
  for (let i = 0; i < items.length; i++) { sum += items[i].price; }
  return sum;
}
```

**Why they exist:** Direct copy-paste without any modifications.

### Type-2: Renamed/Parameterized Clones
Structurally identical code with renamed identifiers, changed literals, or different types.

**Example:**
```javascript
// File 1
function calculateTotal(items) {
    let sum = 0;
    for (let i = 0; i < items.length; i++) {
        sum += items[i].price;
    }
    return sum;
}

// File 2 (Type-2 clone - renamed variables, same logic)
function computeSum(products) {
    let total = 0;
    for (let j = 0; j < products.length; j++) {
        total += products[j].cost;
    }
    return total;
}
```

**Why they exist:** Copy-paste-modify pattern where developers adapt code for different contexts.

**Detection:** PolyDup normalizes identifiers and literals (e.g., `sum` → `@@ID`, `0` → `@@NUM`) to detect structural similarity.

### Type-3: Near-Miss (Gap-Tolerant) Clones
Similar code with minor modifications like inserted/deleted statements or changed expressions. Type-3 detection finds code that has evolved differently but still shares significant structure.

**Enable Type-3 detection:**
```bash
polydup scan ./src --enable-type3 --type3-tolerance 0.85
```

**Example:**
```javascript
// File 1
function processOrder(order) {
    validateOrder(order);
    let total = calculateTotal(order.items);
    applyDiscount(total, order.coupon);
    return total;
}

// File 2 (Type-3 clone - added logging, changed discount logic)
function processOrder(order) {
    validateOrder(order);
    console.log("Processing order:", order.id);  // ADDED
    let total = calculateTotal(order.items);
    let discount = order.coupon ? 0.1 : 0;      // MODIFIED
    total *= (1 - discount);                     // MODIFIED
    return total;
}
```

**Why they exist:** Code evolution, bug fixes, or feature additions that slightly modify duplicated logic.

**When to use Type-3:**
- Legacy codebases with evolved duplicates
- Finding code that was copy-paste-modified
- Identifying candidates for refactoring into parameterized functions

**Tolerance setting:** The `--type3-tolerance` flag (0.0-1.0) controls how similar code must be. Higher values = stricter matching.

### Type-4: Semantic Clones (Not Yet Implemented)
Functionally equivalent code with different implementations.

**Example:**
```javascript
// File 1 - Imperative loop
function sum(arr) {
    let total = 0;
    for (let i = 0; i < arr.length; i++) {
        total += arr[i];
    }
    return total;
}

// File 2 - Functional approach
function sum(arr) {
    return arr.reduce((acc, val) => acc + val, 0);
}

// File 3 - Recursive
function sum(arr, i = 0) {
    if (i >= arr.length) return 0;
    return arr[i] + sum(arr, i + 1);
}
```

**Why they exist:** Different programming paradigms or styles achieving the same result.

**Detection Challenge:** Requires semantic analysis, control-flow graphs, or ML-based approaches.

### Understanding Your Results

When PolyDup reports duplicates, the clone type indicates:

- **Type-1**: Exact copy-paste → Quick win for extraction into shared utilities
- **Type-2**: Adapted copy-paste → Candidate for parameterized functions or generics
- **Type-3**: Modified duplicates → May require refactoring with strategy patterns
- **Type-4**: Semantic equivalence → Consider standardizing on one implementation

**Typical Real-World Distribution:**
- Type-1: 5-10% (rare in mature codebases)
- Type-2: 60-70% (most common - copy-paste-modify)
- Type-3: 20-30% (evolved duplicates)
- Type-4: <5% (requires specialized detection)

**Performance Note**: PolyDup efficiently handles codebases up to 100K LOC. Tested on real-world projects with detection times under 5 seconds for most repos.

## Troubleshooting

### Common Issues

#### "No duplicates found" but you expect some

**Possible causes:**
- **Threshold too high**: Try lowering `--threshold` to 0.70-0.80
- **Block size too large**: Reduce `--min-block-size` to 5-10 lines
- **Type-3 not enabled**: Add `--enable-type3` for gap-tolerant matching

```bash
# More sensitive scan
polydup scan ./src --threshold 0.70 --min-block-size 5 --enable-type3
```

#### "Too many false positives"

**Solutions:**
- **Increase threshold**: Use `--threshold 0.95` for high-confidence matches
- **Exclude Type-3**: Add `--exclude-type type-3` to remove noisy matches
- **Increase block size**: Use `--min-block-size 50` for substantial duplicates only

```bash
# Strict, high-quality scan
polydup scan ./src --threshold 0.95 --exclude-type type-3 --min-block-size 50
```

#### "Permission denied" errors

**Fix:**
```bash
# Check file permissions
ls -la /path/to/scan

# Run with proper permissions
chmod +r /path/to/files

# Use --debug to see detailed error info
polydup scan ./src --debug
```

#### "Unsupported file type" warnings

**Explanation:** PolyDup currently supports JavaScript, TypeScript, Python, Rust, Vue, and Svelte. Other file types are skipped automatically.

**Workaround:**
- Wait for language support (check [GitHub issues](https://github.com/wiesnerbernard/polydup/issues))
- Contribute a parser (see [CONTRIBUTING.md](CONTRIBUTING.md))

#### Colors not working in CI/CD

**Solution:**
```bash
# Disable colors explicitly
polydup scan ./src --no-color

# Or use environment variable
NO_COLOR=1 polydup scan ./src
```

#### "Out of memory" on large codebases

**Solutions:**
```bash
# Increase minimum block size to reduce memory usage
polydup scan ./src --min-block-size 100

# Scan directories separately
polydup scan ./src
polydup scan ./tests
polydup scan ./lib

# Exclude generated/vendor code
# Create .polyduprc.toml with exclude patterns
```

### Performance Tips

**For large codebases (>50K LOC):**
- Use `--min-block-size 50-100` to focus on substantial duplicates
- Disable Type-3 detection (it's more computationally expensive)
- Use `--exclude-type type-3` to skip gap-tolerant matching
- Increase `--threshold` to 0.95 to reduce candidate matches

**For monorepos:**
- Create `.polyduprc.toml` at root with shared configuration
- Use `--group-by file` to organize results by module
- Exclude `node_modules`, `dist`, `target`, etc. in config

**For CI/CD:**
- Cache the `polydup` binary to speed up pipeline
- Use `--format json` for machine-readable output
- Set appropriate exit code handling (0=clean, 1=duplicates, 2=error)

### Getting Help

**Debug Mode:**
```bash
# Enable detailed error traces
polydup scan ./src --debug
```

**Verbose Output:**
```bash
# Show performance statistics
polydup scan ./src --verbose
```

**Report an Issue:**
1. Check [existing issues](https://github.com/wiesnerbernard/polydup/issues)
2. Include:
   - PolyDup version (`polydup --version`)
   - Operating system and architecture
   - Command that failed
   - Error message with `--debug` flag
   - Sample code if applicable (anonymized)

**Community:**
- GitHub Discussions: [Ask questions](https://github.com/wiesnerbernard/polydup/discussions)
- GitHub Issues: [Report bugs](https://github.com/wiesnerbernard/polydup/issues)

## Development

### Building from Source

**Prerequisites:**
- Rust 1.70+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 16+ (for Node.js bindings)
- Python 3.8-3.12 (for Python bindings)

**CLI:**
```bash
git clone https://github.com/wiesnerbernard/polydup.git
cd polydup
cargo build --release -p polydup
./target/release/polydup scan ./src
```

**Node.js bindings:**
```bash
cd crates/polydup-node
npm install
npm run build
npm test
```

**Python bindings:**
```bash
cd crates/polydup-py
pip install maturin
maturin develop
python -c "import polydup; print(polydup.version())"
```

**Run tests:**
```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p polydup-core

# With coverage
cargo install cargo-tarpaulin
cargo tarpaulin --workspace
```

### Creating a Release

**Recommended**: Create releases directly from GitHub UI - fully automated, no local tools required!

1. Go to [Releases → New Release](https://github.com/wiesnerbernard/polydup/releases/new)
2. Create a new tag (e.g., `v0.2.7`)
3. Click "Publish release"
4. **Everything happens automatically** (~5-7 minutes):
   - Syncs version files (Cargo.toml, package.json, pyproject.toml)
   - Updates CHANGELOG.md with release entry
   - Moves tag to version-synced commit (if needed)
   - Builds binaries for all 5 platforms (macOS/Linux/Windows)
   - Publishes to crates.io, npm, and PyPI
   - Creates release with binary assets
   - **Zero manual steps required - truly one-click releases!**

**Alternative**: Use the release script locally:
```bash
./scripts/release.sh 0.2.5
```

See [docs/RELEASE.md](docs/RELEASE.md) for detailed instructions.

### Pre-commit Hooks

Install pre-commit hooks to automatically run linting and tests:

```bash
# Install pre-commit (if not already installed)
pip install pre-commit

# Install the git hooks
pre-commit install
pre-commit install -t pre-push

# Run manually on all files
pre-commit run --all-files
```

The hooks will automatically run:
- **On commit**: `cargo fmt`, `cargo clippy`, file checks (trailing whitespace, YAML/TOML validation)
- **On push**: Full test suite with `cargo test`

To skip hooks temporarily:
```bash
git commit --no-verify
```

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Install pre-commit hooks (`pre-commit install`)
4. Make your changes and ensure tests pass (`cargo test --workspace`)
5. Run clippy (`cargo clippy --workspace --all-targets -- -D warnings`)
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## License

MIT OR Apache-2.0
