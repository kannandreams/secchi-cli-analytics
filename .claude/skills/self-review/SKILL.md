---
name: self-review
description: Pre-push / pre-PR self review for secchi-cli-analytics. Run before pushing a branch or opening a pull request. It replays every CI and pre-commit gate locally, then performs a structured peer-style review of the branch diff against this repo's architecture and testing standards, and produces a PASS/FAIL report. Use when the user says "review my changes", "am I ready to push", "raise a PR", or after finishing any feature/fix.
---

# Self review (pre-PR)

Act as the peer reviewer this PR will eventually get. Two phases: first
the **mechanical gates** (exactly what CI runs), then a **review pass**
over the diff. Do not push or open a PR while any gate fails or any
blocking finding is open.

## Phase 1 — Mechanical gates (replay CI locally)

Run each gate and record PASS/FAIL with the failing output:

```bash
# 0. Branch name must match <type>/<slug>
scripts/check-branch-name.sh

# 1. Hooks (trailing whitespace, EOF, toml/yaml validity, fmt, branch name)
uvx pre-commit run --all-files

# 2. Formatting and lints — same commands as .github/workflows/ci.yml
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. Full offline test suite
cargo test --workspace --all-features

# 4. Release build must still succeed (CI runs this on every push)
cargo build --workspace --release
```

Notes:
- If a gate fails, fix it and re-run **all** gates, not just the failed
  one.
- Never "fix" a failure by loosening a test, deleting an assertion, or
  adding an `#[allow]` — that is a blocking finding in itself. A new
  `#[allow]` needs a comment stating why the lint is wrong here.
- CI also runs cargo-deny; if dependencies changed, run
  `cargo deny check` locally too.

## Phase 2 — Review pass over the diff

```bash
git fetch origin main
git diff --stat origin/main...HEAD
git diff origin/main...HEAD
```

Read the full diff, then check each item and collect findings:

### Scope and shape
- [ ] One problem/feature per PR; no unrelated refactoring mixed in.
- [ ] Commits explain what and why.
- [ ] No stray files: target output, `.DS_Store`, debugging leftovers,
      commented-out code.

### Architecture conformance (see the analytics-architecture skill)
- [ ] Capture-path invariants intact: structure-not-content, drop-never-
      crash, synchronous bounded write, capture observes never precedes.
- [ ] Schema untouched — or `RAW_EVENT_COLUMNS`, the golden file, and the
      store `COLUMN_TYPES` all changed together with an additive-only
      rationale (anything else needs a `SCHEMA_VERSION` decision).
- [ ] Core default features still DuckDB-free; no heavyweight deps added
      to the shim path.
- [ ] Adapter changes stay thin (no redaction/storage/delivery logic in a
      shim) and satisfy the add-shim-adapter contract.
- [ ] Library crates return typed errors; `anyhow` stays in the binary.

### Tests (see the analytics-testing skill)
- [ ] Behavior change ⇒ test change, in the right layer (unit / store
      fixtures / toy-CLI / e2e).
- [ ] House style: offline, deterministic, tempdirs, injected ids and
      times, MemorySink; no env mutation in unit tests.
- [ ] New redaction patterns come with positive and near-miss negative
      cases.

### Docs and user surface
- [ ] New commands or flags appear in `--help`, the README commands
      table, and — if the integration story changed — the README example
      and secchi-bible `docs/cli-analytics/`.
- [ ] `CHANGELOG.md` updated when release-noteworthy.

## Phase 3 — Report

Output a report in this exact shape, then act on it:

```
## Self-review: <branch>

### Gates
| Gate | Result |
| --- | --- |
| branch-name | PASS |
| pre-commit  | PASS |
| fmt / clippy | PASS |
| cargo test | PASS (N passed) |
| release build | PASS |

### Findings
1. [BLOCKING] <file:line> — <problem, and which standard it violates>
2. [ADVISORY] <file:line> — <suggestion>

### Verdict
READY TO PUSH  —  or  —  NOT READY: fix blocking findings above.
```

Severity rules: gate failures, capture-path or schema-freeze violations,
missing tests for changed behavior, or swallowed errors outside the sink
boundary are **BLOCKING**. Style preferences beyond rustfmt/clippy's
verdict are **ADVISORY** — do not block on taste.

Only after "READY TO PUSH": push the branch, and if asked to open the PR,
use a title in `<type>: <summary>` form matching the branch type and a
body that states the problem, the change, and the test evidence.
