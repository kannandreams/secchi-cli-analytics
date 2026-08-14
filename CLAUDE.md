# CLAUDE.md

secchi-cli-analytics is Secchi's CLI-usage-analytics pillar: framework
shims emit privacy-safe events into a JSONL spool, and the
`secchi-analytics` binary compacts them into DuckDB and answers
stats/tail/status queries. Capture is structure-only — command paths and
flag names, never argv or free text — and instrumentation must never add
latency or crash modes to a host CLI.

## Before you code

Skills are the project's design contract; load the relevant one first.

- **analytics-architecture** — crate layering, capture-path invariants,
  where new code goes.
- **add-shim-adapter** — checklist for a new framework adapter.
- **analytics-testing** — house test style; offline and deterministic.
- **self-review** — run before every push or PR; replays the CI gates.

## Commands (CI-identical)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
uvx pre-commit run --all-files
```

Fast inner loop (never compiles DuckDB):
`cargo test -p secchi-analytics-core -p secchi-analytics-clap`

## Non-negotiables

- Event schema v1 is frozen. The golden-file test is never "fixed" by
  regenerating the golden: additive fields update `RAW_EVENT_COLUMNS`,
  the golden, and the store column table together in one reviewed change;
  anything else needs a `SCHEMA_VERSION` decision.
- Raw argv and free-text error messages never get a field, anywhere.
- The shim path (core default features) never links DuckDB and never
  panics or errors across its public API; every failure is a dropped
  event, and only a first run may print.
- Flag values enter events only through the redacting builder.
- Tests are offline and deterministic: tempdirs, injected ids and
  timestamps, MemorySink. No `now()` in an assertion path.
- Branch names: `<type>/<slug>` — enforced by pre-commit.
