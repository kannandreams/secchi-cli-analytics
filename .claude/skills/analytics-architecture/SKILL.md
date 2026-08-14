---
name: analytics-architecture
description: Load before changing any crate in this repo — crate layering, capture-path invariants, schema-freeze rules, and where new code goes.
---

# Architecture

## Crates and the direction of dependencies

```text
secchi-cli-analytics (bin "secchi-analytics")
    │                └──> secchi-analytics-core  [features = ["store"]]
    └──> secchi-analytics-clap
                     └──> secchi-analytics-core  [default features only]
```

- `core/` owns everything privacy-sensitive: the event schema
  (`event.rs`), redaction (`redact.rs`), the spool writer (`spool.rs`),
  sinks (`sink.rs`), config/identity/context, and — behind the `store`
  feature — the DuckDB store (`store/`).
- `shims/rust/` is thin by design: it reads what clap already knows and
  hands it to core. Adapter logic never grows redaction, storage, or
  delivery behavior.
- `daemon/` is the CLI. It is also the reference shim integration — its
  `main.rs` is quoted in the README, so keep it exemplary.

The `store` feature exists so the shim path never compiles or links
DuckDB. Do not add DuckDB (or any heavyweight dependency) to core's
default features.

## Capture-path invariants

1. **Structure, not content.** Raw argv and error messages have no field.
   A value is recorded only for an allowlisted flag, and only after
   passing `redact::redact` — which happens inside `EventBuilder`, the
   only construction path.
2. **Drop, never crash.** `EventSink::record` is infallible; `SpoolSink`
   swallows every error. Shim entry points wrap in `catch_unwind`. The
   only permitted output is the one-line first-run notice.
3. **Synchronous bounded write.** One open, one sub-4KiB `write_all`, no
   locks, no retries, no threads, no network. Do not "improve" this with
   a background flusher — CLIs exit immediately after their last command
   and a flusher loses exactly those events.
4. **Capture observes, never precedes.** Identity is created lazily at
   record time (see `Identity::Lazy` in the shim), after the host command
   ran.

## Schema freeze

`SCHEMA_VERSION = 1` is a public contract. `RAW_EVENT_COLUMNS`
(event.rs), the golden file (`core/tests/golden/event_v1.json`), and
`COLUMN_TYPES` (store/mod.rs) are bound together by tests. An additive
optional field updates all three in one reviewed change and needs no
version bump; renaming or repurposing a field needs a `SCHEMA_VERSION`
decision and a deprecation window.

## Store rules

- `raw_events` is the only table written; everything else is derived.
- Queries always union `raw_events` with the live spool via anti-join on
  `event_id` — that is what keeps interrupted compactions harmless.
- Complex types (lists, maps) never cross the duckdb-rs bindings; all
  ingestion is JSON/Parquet → SQL inside DuckDB.
- Compaction order is fixed: parquet copy, anti-join insert, spool
  delete. Each step is idempotent under re-run.

## Where new code goes

- New query or report → `core/src/store/query.rs` plus a command module
  in `daemon/src/commands/`; render in the command, compute in the store.
- New redaction pattern → `core/src/redact.rs`, tests first, including
  near-miss negatives.
- New framework adapter → see the add-shim-adapter skill.
- New env marker (CI system, agent) → `core/src/context.rs` const lists.
