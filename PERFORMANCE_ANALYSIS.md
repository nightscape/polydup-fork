# PolyDup Performance Analysis

> **Status:** Updated 2026-05-09 after fixing issues #1, #2, and a latent
> rolling-hash math bug discovered during the work. Earlier revision (2026-05-08)
> verified the analysis against the code and corrected an LLM-first-pass that
> had the call-path attribution and impact ranking wrong.
>
> **Headline result:** tatuin scan went from 28.48 s → 1.00 s (~28× speedup);
> r3bl-open-core (697 files), previously killed after 15+ minutes, now
> completes in 53.48 s. Output is byte-identical apart from the (now-correct)
> `unique_hashes` count.

---

## TL;DR

The default scan path is:

```
scan()
 └─ analyze_files()                            # tokenize each file (parallel via Rayon)
 └─ find_duplicate_hashes()
     └─ find_type12_duplicates()              # O(n²) over functions
         └─ find_clones_between_functions()   # ⟵ THIS is the real hot path
     └─ find_type3_duplicates() (optional)
 └─ compute_stats()                           # runs once at end
```

**`find_clones_between_functions` (lib.rs:1424) was the dominant cost.** It
called `compute_window_hash` (a non-rolling, O(window_size) per step polynomial
hash) for every window position in every function pair. **Fixed (2026-05-09)**
by switching it to an incremental rolling hash via `compute_rolling_hashes`.

The `RollingHash` struct previously had two issues that didn't matter on the
default scan: a `Vec::remove(0)` shift on every roll, and a math bug where
the oldest token's coefficient was `b^(N+1)` instead of `b^N` so identical
windows reached via different prefixes hashed differently. Both were fixed
because pulling `RollingHash` into the hot path made them load-bearing
(performance and correctness, respectively).

---

## Issue Inventory (Verified)

### 1. `find_clones_between_functions` rebuilds window hashes from scratch — ✅ FIXED (2026-05-09)

**Location:** `polydup-core/src/lib.rs:1424-1494` (post-fix)

```rust
while i <= func1.tokens.len().saturating_sub(self.min_block_size) {
    let hash = hashing::compute_window_hash(&func1.tokens[i..i + self.min_block_size]);
    hash_map.entry(hash).or_default().push(i);
    i += 1;
}

// And the same for func2 in the search loop ...
let hash = hashing::compute_window_hash(&func2.tokens[j..j + self.min_block_size]);
```

`compute_window_hash` (hashing.rs:433) iterates the full 50-token window each
time and runs `hash_token` per token. With `min_block_size=50` this is **O(m × 50)**
work per function instead of the **O(m)** an incremental rolling hash would give.

This loop is hit once per `(i, j)` function pair — it's inside the O(n²) outer
loop in `find_type12_duplicates`. On any non-trivial codebase this is the
single largest cost.

**Fix applied:** the function now precomputes a single sweep of rolling
hashes per side via `compute_rolling_hashes`, then walks `func2`'s hash list
by index instead of recomputing each window. The match-and-skip semantics
are preserved (skip = `extension.max(1) + 1` token positions per match).

**Why the original LLM pass missed it:** the same algorithm shape exists in
`detect_duplicates_with_extension` (hashing.rs:365), which the first pass
flagged. But that function is **dead code** (imported at lib.rs:27, never
called outside tests). The live version is in lib.rs and used the same
anti-pattern.

**Measured impact:** ~28× on tatuin (28.48 s → 1.00 s). On r3bl-open-core
the scan now completes in 53.48 s where it previously failed to finish.

---

### 2. `Vec::remove(0)` in `RollingHash::roll()` — ✅ FIXED (2026-05-09)

**Location:** `polydup-core/src/hashing.rs:266` (pre-fix)

```rust
let old_token = self.window.remove(0);  // O(n) shift
self.window.push(token_hash);
```

The bug was real. **Fix applied:** swapped `Vec<u64>` for `VecDeque<u64>` and
use `pop_front()` / `push_back()` for O(1) per step.

After fix #1 brought `RollingHash` into the hot path, this swap was load-bearing
for the speedup — without it the rolling-hash refactor would still be O(n²) in
window size from the `remove(0)` shifts.

### 2b. Latent math bug in `RollingHash::roll()` — ✅ FIXED (2026-05-09, bonus)

While verifying issue #1, the rolling hash was producing **different hashes
for identical windows** depending on what tokens preceded them. Cause: the
order of operations was

```rust
self.hash -= old_token * base_power;     // base_power = b^window_size
self.hash = self.hash * base + new_token;
```

which expands to `old_hash * b - old_token * b^(N+1) + new_token`. The
oldest token's coefficient should be `b^N`, not `b^(N+1)`. Fixed by swapping
the two steps:

```rust
self.hash = self.hash * base + new_token;   // shift everyone by b first
self.hash -= old_token * base_power;        // now oldest has coeff b^N
```

**Why this had been latent:** every existing consumer (`compute_stats`,
`add_hashes_to_cache`, `scan_with_cache`) only compared rolling hashes within
a single token stream that started from token 0. A clone that appeared in
identical form at the same offset in two streams would still hash identically.
The bug would have started biting `scan --git-diff` against the cache as
soon as a clone appeared at different offsets — or in the changed file vs.
its cached neighbour. It was an unexploded mine.

A regression test (`test_rolling_hash_consistent_across_prefixes`) now guards
the property: same window contents → same hash regardless of prefix.

---

### 3. Redundant rolling hash in `compute_stats` — REAL BUT MISDIAGNOSED

**Location:** `polydup-core/src/lib.rs:724-734`

```rust
let unique_hashes: usize = {
    let mut hash_set = std::collections::HashSet::new();
    for fh in function_hashes {
        let hashes = compute_rolling_hashes(&fh.tokens, self.min_block_size);
        for (hash, _) in hashes {
            hash_set.insert(hash);
        }
    }
    hash_set.len()
};
```

Haiku claimed these "were ALREADY computed in `find_duplicate_hashes`" — that's
**false**. `find_clones_between_functions` uses `compute_window_hash`, a
*different* polynomial hash with a different formula. The two hash spaces are
not interchangeable; there's no cache to reuse.

The actual issue is that `compute_stats` does work that isn't necessary. If we
just want a uniqueness count, we could either (a) skip it, (b) count via a
cheaper sketch, or (c) populate the set as part of the main detection pass.

**Estimated impact:** runs once at end of scan, so capped at the share of
total time spent in stats. Likely 5-10% on small inputs, less on large ones
where the function-pair loop dominates.

---

### 4. Char-by-char tokenization with allocations — REAL, MODEST

**Location:** `polydup-core/src/hashing.rs:70-200` in `normalize_with_line_numbers`

```rust
let chars: Vec<char> = code.chars().collect();          // line 76
// ...
let word: String = chars[start..i].iter().collect();    // line 162
```

The `Vec<char>` upfront allocation is real. The per-identifier
`iter().collect::<String>()` is real. The keyword path has to materialize a
`String` to look up in the keyword set, but the identifier path does not need
the allocation.

**Fix sketch:** iterate `code.char_indices()`, slice `&code[start..end]` for
keyword lookup, skip the allocation entirely for identifiers (they're all
normalized to `Token::Identifier`).

**Estimated impact:** 5-10% of tokenization time, which itself is a fraction
of total scan time on real codebases (the function-pair loop dominates).

---

### 5. Dead code: `detect_duplicates_with_extension` ❌

**Location:** `polydup-core/src/hashing.rs:365-426`

`grep -rn detect_duplicates_with_extension` shows only the import and the
definition — no callers. It's dead code. Haiku's proposed fix here would
have **zero effect** because nothing runs this function.

The same algorithmic pattern is in the live function
`find_clones_between_functions` (issue #1) — that's where the fix needs to
land.

**Fix:** delete this function and the import, or repurpose it as the engine
for issue #1 by making it pluggable.

---

### 6. `to_string_lossy().to_string()` for file paths — TRIVIAL

**Location:** `polydup-core/src/lib.rs:842`

```rust
let file_path: Arc<str> = path.to_string_lossy().to_string().into();
```

When the `Cow` is already owned (non-UTF8 path), `.to_string()` is an extra
allocation. Better:

```rust
let file_path: Arc<str> = path.to_string_lossy().into();
```

`Arc::<str>::from(Cow<str>)` exists and avoids the extra step.

Note: Haiku's suggested `into_owned().into()` is functionally identical to
the current code — both always allocate. The above is the actual improvement.

**Estimated impact:** <1%. Once per file. Cleanup-grade.

---

## Status & Recommended Order of Operations

1. ~~Benchmark first.~~ ✅ Done.
2. ~~Fix #1 + #2 together.~~ ✅ Done. Plus a third bonus fix (#2b: rolling
   hash math bug) discovered during the work and required for #1 to be
   correct.
3. ~~Re-benchmark.~~ ✅ Done — see "Post-fix Measurements" below.
4. **#3 (stats):** the `compute_stats` rolling-hash sweep is now ~5 ms inside
   a 1 s scan. Not worth touching unless it shows up on bigger inputs.
5. **#4 (tokenization):** only if profiling shows it's still meaningful.
   The function-pair loop no longer dominates, so this may now be the
   biggest single cost — worth profiling before assuming.
6. **#5 (dead code):** delete during cleanup.
7. **#6 (path alloc):** drive-by fix.

A new candidate worth noting: scans are still single-threaded in the
function-pair loop (`real ≈ user` on r3bl-open-core's 53 s run). Parallelising
the outer loop in `find_type12_duplicates` with Rayon would scale linearly
with cores on big codebases. Not in the original analysis — flag it as #7
if pursued.

---

## Baseline Measurements (commit 357ed72, release build)

### Target: `/Users/martin/Workspaces/pkm/tatuin`
- 87 Rust files, 14,745 LOC, 513 functions, 87,723 tokens, 70 duplicates
- 5 consecutive runs (no warmup needed; very stable):

| run | real (s) |
|-----|----------|
| 1 | 23.90 |
| 2 | 24.07 |
| 3 | 23.95 |
| 4 | 23.89 |
| 5 | 23.90 |

Mean: **23.94 s ± 0.08 s**.

A separate single-run sample taken on 2026-05-09 (same commit, same
machine, different ambient load) measured `28.48 real / 21.19 user`. The
post-fix comparison below uses 28.48 s as the same-session baseline, but
the doc baseline of 23.94 s is the more reliable steady-state number.

CPU breakdown for one run: `24.75 real / 24.87 user / 0.09 sys`.
**user ≈ real → workload is essentially single-threaded.** Rayon parallelism
in `analyze_files` is a small fraction of total time. The dominant cost is
the single-threaded `find_duplicate_hashes` → `find_type12_duplicates` →
`find_clones_between_functions` chain — which directly confirms issue #1
above.

### Target: `polydup-core/src` (self)
- ~36 files, much smaller surface
- Single run: `1.44 real / 1.43 user / 0.02 sys` — same single-threaded shape.

### Target: r3bl-open-core (697 files, ~aborted)
- Killed after 15+ minutes at 99% on a single core. Confirms the O(n²)
  function-pair loop blows up on larger codebases. **This is the user's
  ~50% slow claim showing up in the wild.**

## Post-fix Measurements (2026-05-09, after #1 + #2 + #2b)

### Target: tatuin — 5 consecutive runs
| run | real (s) | user (s) |
|-----|----------|----------|
| 1 | 1.03 | 1.14 |
| 2 | 0.96 | 1.09 |
| 3 | 1.00 | 1.12 |
| 4 | 1.01 | 1.14 |
| 5 | 1.01 | 1.14 |

Mean: **1.00 s ± 0.03 s** — a **~28× wall-clock improvement** vs. the
28.48 s same-session baseline (~24× vs. the 23.94 s doc baseline).
Internal `duration_ms` from the JSON output went 25,844 → 1,009.

Output diff vs. baseline: byte-identical apart from `scan_time` (timestamp)
and `unique_hashes` (34,802 → 32,479). The lower count is the *correct*
one — the rolling-hash math bug had inflated it by treating identical
windows reached via different prefixes as distinct.

### Target: r3bl-open-core (697 files)
- `53.48 real / 54.11 user / 0.29 sys` — completed where the baseline ran
  >900 s before being killed. Still single-threaded (`user ≈ real`), so the
  outer loop is now the place to look if more headroom is needed (see #7
  in the recommendations above).

### How to re-run
```bash
cargo build --release
for i in 1 2 3 4 5; do
  /usr/bin/time -p ./target/release/polydup scan /path/to/target --format json \
    > /tmp/run-$i.json 2> /tmp/run-$i.time
  grep '^real' /tmp/run-$i.time
done
```

Use **tatuin** as the standard benchmark target — it's large enough to
exercise the hot path (now ~1 s) but small enough to iterate on. For
regression risk on the rolling hash itself, the
`test_rolling_hash_consistent_across_prefixes` unit test in
`polydup-core/src/hashing.rs` is the canary.
