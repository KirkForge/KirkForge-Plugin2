# ADR-0013: The `stratum:` comment convention for deferred work

- **Status:** Accepted
- **Date:** 2026-06-24

## Context

A codebase accumulates shortcuts. The naive shortcuts — global
locks, O(n²) scans, naive heuristics, magic-number thresholds —
work at the scale they were written for and silently fail at the
next scale. The two failure modes are:

1. **The shortcut becomes load-bearing.** A contributor reads
   the shortcut, assumes it is the design, and builds on top of
   it. By the time the scale breaks the shortcut, the design has
   ossified around it.
2. **The shortcut becomes invisible.** A refactor erases the
   context that justified the shortcut. The next contributor
   "fixes" the obvious inefficiency without understanding why it
   existed.

The fix is a comment convention that is:

- Searchable (so a contributor can find all shortcuts).
- Declarative (so the comment names the ceiling and the upgrade
  path, not just the apology).
- Auditable (so a separate tool can harvest shortcuts into a
  ledger).

The convention is `stratum: <ceiling>, <upgrade path>`. It
appears inline at the site of the shortcut. A separate harvest
script (`stratum debt`) greps for the convention and produces a
markdown ledger sorted by file and line.

## Decision

### The convention

```rust
// stratum: <one-line ceiling>, <one-line upgrade path>
```

Examples:

```rust
// stratum: global Mutex on store, per-shard locks if throughput matters
let store = Arc::new(Mutex::new(store));

// stratum: O(n) scan over content, hashmap index if transforms slow on huge inputs
for transform in &self.transforms {
    if transform.applies_to().contains(&content_type) {
        return transform.apply(content);
    }
}

// stratum: fixed bloat threshold, per-domain override exists but is unused
const BLOAT_THRESHOLD: f32 = 0.5;

// stratum: in-memory store only, swap to Sqlite when persistence needed
let store: Box<dyn OffloadStore> = Box::new(InMemoryOffloadStore::new());

// stratum: hard-coded mode in dev, env var in CI
let mode = Mode::Full;
```

The convention is two parts separated by a comma:

1. **Ceiling** — what the shortcut cannot handle. Concrete and
   falsifiable: "global Mutex", "O(n) scan", "fixed threshold".
   Not vague: "this might be slow" is not a ceiling.
2. **Upgrade path** — what to do when the ceiling breaks. Also
   concrete: "per-shard locks", "hashmap index", "per-domain
   override". Not vague: "optimize" is not an upgrade path.

A comment that fails either test is removed at review.

### Where the convention applies

The convention applies to:

- Constants and magic numbers.
- Lock granularity choices.
- Algorithm choices (O(n) vs O(log n) vs amortised).
- Storage backend choices.
- Hard-coded values that should be configurable.

The convention does *not* apply to:

- TODO comments (`// TODO: refactor`).
- FIXME comments.
- Documentation comments (`///`).
- Comments explaining what code does (the code should explain
  itself).

`TODO` and `FIXME` are the wrong tools for shortcuts. They
record an intention to change; they do not record why the
shortcut exists or when it stops working.

### The harvest skill

`stratum debt` (binary subcommand) greps for the convention and
prints a ledger:

```
crates/kirkstratum-core/src/store/mod.rs:12
  global Mutex on store, per-shard locks if throughput matters

crates/kirkstratum-core/src/pipeline/orchestrator.rs:78
  O(n) scan over content, hashmap index if transforms slow on huge inputs

crates/kirkstratum-core/src/config.rs:5
  fixed bloat threshold, per-domain override exists but is unused
```

The harvest is a deterministic grep over `crates/` and `docs/`.
It does not parse Rust; it does not need to. The convention is
line-oriented and self-describing.

```rust
// crates/kirkstratum-cli/src/debt.rs

use std::path::Path;

pub fn harvest(root: &Path) -> Vec<DebtEntry> {
    let pattern = "// stratum: ";
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().map_or(false, |x| x == "rs"))
    {
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (i, line) in text.lines().enumerate() {
            if let Some(rest) = line.trim_start().strip_prefix(pattern) {
                out.push(DebtEntry {
                    file: entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    line: i + 1,
                    comment: rest.trim().to_string(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[derive(Debug, serde::Serialize)]
pub struct DebtEntry {
    pub file: std::path::PathBuf,
    pub line: usize,
    pub comment: String,
}
```

The `stratum debt` subcommand prints the ledger to stdout. The
output is markdown-friendly; piping to `tee DEBT.md` produces a
file that can be reviewed in a PR.

### The ledger file

`DEBT.md` at the repo root is the canonical ledger. It is
generated by `stratum debt > DEBT.md` and committed alongside
the code. A PR that adds a new shortcut adds a row to the
ledger; a PR that fixes a shortcut removes the row.

`DEBT.md` is gitignored or not, at the repo owner's discretion.
The convention treats it as a generated artefact: re-run
`stratum debt` to refresh it. A stale ledger is worse than no
ledger.

### Audit interval

`stratum debt` is run on every CI build, but its output is
informational only. CI does not fail on the presence of debt;
CI fails on the *absence* of a `DEBT.md` if any `stratum:`
comments exist in the tree:

```yaml
# .github/workflows/ci.yml (sketch)
- name: Audit stratum debt
  run: |
    cargo run --quiet --bin stratum -- debt > /tmp/debt.md
    if git grep -q '^// stratum: ' crates/; then
      test -s /tmp/debt.md || (echo "stratum: comments exist but DEBT.md is empty" && exit 1)
    fi
```

The check is structural: if shortcuts exist, the ledger must be
non-empty. The ledger's *contents* are a human concern.

### Examples of bad and good comments

Bad (no ceiling):

```rust
// TODO: this could be faster
```

Bad (no upgrade path):

```rust
// stratum: O(n²) scan
```

Bad (vague ceiling):

```rust
// stratum: might not scale, optimize later
```

Good:

```rust
// stratum: O(n²) scan over transform list, swap to type-indexed registry if N > 32
```

Good:

```rust
// stratum: hard-coded mode=Full in dev, --mode flag added in CLI but not wired here
```

Good:

```rust
// stratum: in-memory store only, see ADR-0004 § SqliteOffloadStore for the production backend
```

## Consequences

Negative first:

- A contributor who adds a shortcut without the comment is
  flagged at review. This is friction; the friction is the
  point.
- The ledger can grow large. A project with many shortcuts has
  a long `DEBT.md`. The harvest is sorted by file and line, so
  navigation is cheap, but the noise is real.
- The convention is grep-based, not parser-based. A
  multi-line `stratum:` block (e.g. a long comment with
  embedded code samples) is harvested as one line. A contributor
  who wants a structured harvest must keep each comment on one
  line.

Positive:

- A new contributor can read `DEBT.md` and see every shortcut
  in the codebase in 30 seconds.
- The convention is grep-friendly. `git grep 'stratum:'` is the
  fastest way to find every deferred decision.
- The harvest is mechanical. There is no judgment call about
  whether a comment qualifies; the prefix decides.

## Implementation notes

The harvest function lives at `crates/kirkstratum-cli/src/debt.rs`.
The `walkdir` crate is added as a dependency of the CLI only;
the core crate does not gain a filesystem-walking dependency.

The CLI subcommand:

```rust
#[derive(Parser)]
pub struct DebtArgs {
    /// Write to file instead of stdout.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,
}

pub fn run(args: &DebtArgs) -> anyhow::Result<()> {
    let root = std::path::Path::new(".");
    let entries = harvest(root);
    let md = render_markdown(&entries);
    match &args.out {
        Some(p) => std::fs::write(p, md)?,
        None => print!("{}", md),
    }
    Ok(())
}
```

The `render_markdown` function emits:

```markdown
# Stratum debt ledger

Generated by `stratum debt`. Do not edit by hand; re-run the
command to refresh.

## crates/kirkstratum-core/src/store/mod.rs

- L12: global Mutex on store, per-shard locks if throughput matters
- L34: hard-coded TTL of 60s, --ttl flag added in CLI but not wired

## crates/kirkstratum-core/src/pipeline/orchestrator.rs

- L78: O(n) scan over content, hashmap index if transforms slow on huge inputs
```

A test asserts that `harvest` finds a known fixture comment and
ignores a `TODO` in the same file:

```rust
#[test]
fn harvest_finds_stratum_comments_and_ignores_todo() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("lib.rs"), "
        // stratum: O(n) scan, hashmap if N > 100
        fn f() {}

        // TODO: refactor
        fn g() {}
    ").unwrap();
    let entries = harvest(dir.path());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].line, 2);
}
```
