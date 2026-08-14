# Contributing

Thanks for considering a contribution. This repo is small on ceremony and
strict on a few invariants.

## Ground rules

- One problem or feature per pull request.
- Branch names follow `<type>/<slug>`, e.g. `feat/cobra-shim`,
  `fix/spool-day-rollover`. Types: feat, fix, docs, style, refactor,
  perf, test, build, ci, chore, revert. A pre-commit hook enforces this.
- CI must be green: `cargo fmt`, clippy with `-D warnings`, the full test
  suite, a release build, and cargo-deny, on Linux and macOS.

## Setup

```bash
git clone https://github.com/kannandreams/secchi-cli-analytics
cd secchi-cli-analytics
uvx pre-commit install
cargo test --workspace --all-features   # first run compiles DuckDB; be patient
```

The fast inner loop for core and shim work is
`cargo test -p secchi-analytics-core -p secchi-analytics-clap`, which
never compiles DuckDB.

## The invariants that matter

The event schema is frozen (`core/tests/golden_event_v1.rs` will tell you
if you touched it), the capture path must never crash or block a host
CLI, and raw argv or free-text error messages never get a field. The
details live in `.claude/skills/` — see below.

## Working with coding agents

This repo ships its design contract as agent skills in `.claude/skills/`,
and they are just as useful to humans:

- `analytics-architecture` — crate layering and capture-path invariants.
- `add-shim-adapter` — the contract and checklist for a new framework
  adapter.
- `analytics-testing` — the house test style.
- `self-review` — the pre-PR gate; it replays CI locally and reviews the
  diff.

If you use Claude Code or a similar agent, it will pick these up
automatically. If you don't, read them anyway — they are the fastest
route to a PR that passes review.
