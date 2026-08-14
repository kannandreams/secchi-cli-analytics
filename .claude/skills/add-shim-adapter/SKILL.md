---
name: add-shim-adapter
description: Load when adding or changing a framework adapter (clap, Click, Typer, Cobra, the generic wrapper) — the contract every shim must satisfy and the checklist for a new one.
---

# Adding a framework adapter

Adapters exist so a CLI author writes one line and never thinks about
telemetry again. All privacy and delivery behavior lives in the core; an
adapter only reads what the framework already knows. If you find yourself
writing redaction, storage, or retry logic in an adapter, stop — it goes
in `core/`.

## The contract

Every adapter, in every language, must:

1. Emit the identical envelope (`core/src/event.rs`; schema v1). No
   per-framework fields, no second event shape.
2. Record the *resolved* command path from the framework's dispatch
   state — never parsed argv.
3. Record flag names only for flags the user actually passed on the
   command line (clap: `ValueSource::CommandLine`; Click: parameter
   source; Cobra: `Flags().Visit`). Defaults and env-sourced values are
   not something the user typed.
4. Extract values only for allowlisted flag names, and route them through
   the core builder so redaction is structural, not optional.
5. Be infallible at the public API. Internal panics are caught and
   discarded. The only permitted output is the core first-run notice.
6. Never block: capture is one bounded local append (via the core), no
   network, no locks, no threads.
7. Ship a MemorySink-based integration test against a toy CLI (see
   `shims/rust/tests/toy_cli.rs` for the shape): path, explicit-flags-only,
   allowlist, redaction of a planted secret, failed-phase, and kill-switch
   coverage.

## Checklist for a new adapter

- [ ] Find the framework's construction-time hook that sees every current
      and future subcommand (clap: post-parse `ArgMatches` walk; Click:
      group invoke wrap; Cobra: persistent pre/post run — chain existing
      hooks, never replace them).
- [ ] API shape mirrors the clap shim: builder with `cli_version`,
      `allow_values`, test-sink injection; `start` + one record call.
- [ ] Identity is resolved at record time, not start time (capture
      observes the host command, never precedes it).
- [ ] Honor `SECCHI_ANALYTICS=0` and `SECCHI_ANALYTICS_DIR` through the
      core config (or reimplement exactly its precedence in that
      language, spool-format-compatible).
- [ ] Toy-CLI integration tests per the contract above.
- [ ] README: move the framework from planned to shipped in the diagram
      and repository layout.
- [ ] secchi-bible `docs/cli-analytics/sdk-integration.md`: update the
      framework's section with the real API.
