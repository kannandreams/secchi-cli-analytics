---
name: analytics-testing
description: Load before writing or changing any test in this repo — the house style, what is prohibited, and the fixtures that exist.
---

# Testing house style

Tests are offline and deterministic, always. CI has no network and no
stable clock; a test that would behave differently there is wrong here.

## Rules

- **No wall-clock in assertions.** `EventBuilder::build` takes explicit
  ids and timestamps — use it. `build_now` appears only in production
  code and in tests that merely count events. Store tests inject `today`
  into `compact` and fixed cutoffs into `stats`.
- **Tempdirs for all filesystem work** (`tempfile::tempdir()`), one per
  test, never a shared path. The binary's e2e tests isolate through
  `SECCHI_ANALYTICS_DIR` per invocation.
- **No process-global environment mutation in unit tests.** Env-dependent
  behavior (kill switch, dir override) is tested end-to-end in
  `daemon/tests/cli_e2e.rs`, where each spawned process gets its own
  environment; unit tests use injectable seams (`context::detect_from`,
  explicit paths).
- **MemorySink is the shim transport in tests** — no disk, no config.
- **Fixture events are hand-built** through the builder with sequential
  `Uuid::from_u128` ids and fixed RFC 3339 timestamps (see
  `core/tests/store_queries.rs`). No random data, no generated fixtures.
- **Redaction tests are table-driven** with near-miss negatives alongside
  positives. A new pattern lands with both, plus an entry in the
  fail-closed table if the entropy backstop catches its near-misses.
- **No test-only public API.** The seams that exist (`MemorySink`,
  builder injection, `detect_from`) are the seams; do not add
  `#[cfg(test)]` visibility escapes to production types.

## Layout

- Unit tests live in a `tests` module inside the file they test.
- Cross-module behavior: `core/tests/` (golden schema, store queries).
- Binary behavior: `daemon/tests/cli_e2e.rs` via `assert_cmd`.
- Shim behavior: `shims/rust/tests/toy_cli.rs` against a toy clap app.

## The golden test

`core/tests/golden_event_v1.rs` compares byte-for-byte against
`core/tests/golden/event_v1.json` and cross-checks `RAW_EVENT_COLUMNS`.
If it fails, the schema changed: that is a design decision, not a test
fix. See the analytics-architecture skill's schema-freeze section before
touching it.
