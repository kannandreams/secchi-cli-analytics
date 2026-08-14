<table>
  <tr>
    <td>
      <h1>secchi-cli-analytics</h1>
      <p>CLI usage analytics for the Secchi developer tool intelligence
      family. Know which commands people actually run, where they fail,
      and whether a human or an agent was driving — without ever
      collecting an argument value or an error message.</p>
    </td>
  </tr>
</table>

[![CI](https://github.com/kannandreams/secchi-cli-analytics/actions/workflows/ci.yml/badge.svg)](https://github.com/kannandreams/secchi-cli-analytics/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Package downloads and GitHub stars say a tool was installed. They say
nothing about which subcommands earn their keep, which flags nobody uses,
where new users hit their first error, or what a coding agent does when it
drives your CLI. Secchi CLI Analytics answers those questions from data
that never leaves the machine unless you decide it should.

## Try it

```bash
cargo install secchi-cli-analytics

secchi-analytics init          # create ~/.secchi/analytics, print what is captured
secchi-analytics status        # this invocation is itself recorded
secchi-analytics stats --since 7d
secchi-analytics tail
```

The binary instruments itself, so there is data to look at from the first
command. If you use the Python `secchi` CLI, the same commands are
available as `secchi analytics <command>`.

## Architecture

One Rust core owns everything privacy-sensitive — the event schema, the
redaction engine, storage, and (later) delivery. Framework shims stay
thin: they read what the CLI framework already knows and hand it to the
core. The same envelope lands in the same tables whether the event came
from clap, Click, or Cobra.

```text
   your CLI
   ┌─────────────────────────────────────────────────────────┐
   │  clap (shipped)   click / typer (planned)               │
   │  cobra (planned)  generic wrapper (planned)             │
   └────────────────────────┬────────────────────────────────┘
                            │ one call at startup, one at exit
                            ▼
   ┌─────────────────────────────────────────────────────────┐
   │  shim: command path, flag names, duration, exit code    │
   │        never raw argv, never error messages             │
   ├─────────────────────────────────────────────────────────┤
   │  redaction: pattern + entropy checks on the few         │
   │  allowlisted values → [REDACTED:...] on a match         │
   └────────────────────────┬────────────────────────────────┘
                            │ synchronous bounded append; on any
                            │ failure the event is dropped
                            ▼
   ~/.secchi/analytics/spool/2026-08-14.jsonl
                            │
                            │ secchi-analytics compact
                            ▼
   archive/*.parquet  +  events.duckdb (raw_events)
                            │
              ┌─────────────┼─────────────────┐
              ▼             ▼                 ▼
      stats / tail    any DuckDB client   dbt models
                                          (planned)
```

Two design rules do most of the work:

- **Capture structure, not content.** Raw argv and free-text error
  messages have no field in the schema, so "capture then redact" can
  never fail open. A flag value reaches storage only if a human
  allowlisted that flag — and even then it passes a secret scan first.
- **Drop, never crash.** Instrumentation adds no user-visible latency and
  no failure modes. The capture path is one bounded file append; if it
  cannot complete, the event is discarded and the host CLI never knows.

## Instrumenting your own clap CLI

This is `secchi-analytics`'s own `main`, which is the reference
integration:

```rust
use secchi_analytics_clap::Analytics;

fn main() {
    let analytics = Analytics::builder("secchi-analytics")
        .cli_version(env!("CARGO_PKG_VERSION"))
        .allow_values(["since"])
        .start();

    let matches = cli::build().get_matches();
    let outcome = commands::run(&matches);

    let (exit_code, error_class) = match &outcome {
        Ok(()) => (0, None),
        Err(error) => {
            eprintln!("error: {error:#}");
            (1, Some(commands::error_class(error)))
        }
    };

    analytics.record_exit(&matches, exit_code, error_class);
    std::process::exit(exit_code);
}
```

`start()` and `record_exit()` are infallible by contract. In tests, inject
the in-memory transport and assert on emitted events without touching
disk:

```rust
use secchi_analytics_core::sink::MemorySink;

let sink = MemorySink::shared();
let analytics = Analytics::builder("myctl").sink(sink.clone()).start();
// ... drive the CLI ...
assert_eq!(sink.events().len(), 1);
```

## What is captured, what is not

| Captured | Never captured |
| --- | --- |
| command path (`project add`) | raw argv |
| flag names (`--format`) | flag values, unless allowlisted |
| duration, exit code | error messages or stack traces |
| error type name (`ConfigError`) | file contents, stdin/stdout |
| OS and architecture | username, hostname, machine id |
| actor: human or agent | anything derived from identity |

The install id is a random UUID, generated locally, never derived from
who or where you are. The first run prints a one-line notice — there is
no silent first run. `SECCHI_ANALYTICS=0` disables capture entirely, and
`secchi-analytics purge` deletes everything, because local-first data is
only meaningful if you can fully inspect and fully delete it.

## Commands

| Command | What it does |
| --- | --- |
| `init` | Create the data directory and install identity |
| `status` | What is on, where data lives, how much there is |
| `stats [--since 30d] [--command X] [--json]` | Per-command aggregates |
| `tail [-n 20] [--json]` | Most recent events |
| `compact` | Rotate closed spool days into Parquet + DuckDB |
| `purge [--yes]` | Delete all local analytics data |

`stats` and `tail` read the live spool as well as the compacted store, so
they are correct even if `compact` has never run.

## Repository layout

```text
core/           event schema, redaction, spool, DuckDB store
daemon/         the secchi-analytics binary
shims/rust/     clap adapter (this repo dogfoods it)
shims/python/   Click/Typer adapter        (planned, phase 1)
shims/go/       Cobra adapter              (planned, phase 2)
shims/generic/  process-boundary wrapper   (planned, phase 2)
dbt/            dbt_secchi_analytics       (planned, phase 3)
```

Deferred by design, tracked in the roadmap: the Unix-socket fast path and
long-lived daemon mode, REST export and the enterprise collector, and
`pip install "secchi[cli-analytics]"` as a convenience install.

## Development

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The first build compiles bundled DuckDB and takes several minutes; it is
cached afterwards. The fast inner loop for core and shim work is
`cargo test -p secchi-analytics-core -p secchi-analytics-clap`, which
never touches DuckDB.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
