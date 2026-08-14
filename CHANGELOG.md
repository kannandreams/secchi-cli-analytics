# Changelog

All notable changes to this project are documented in this file. Versions
follow semantic versioning; releases are cut from `v*` tags.

Publishing note: the crates have path-and-version dependencies, so
`cargo publish` must run in dependency order — `secchi-analytics-core`,
then `secchi-analytics-clap`, then `secchi-cli-analytics`.

## Unreleased

- Initial version: event schema v1, redaction engine, JSONL spool capture,
  clap shim, and the `secchi-analytics` binary with `init`, `stats`, `tail`,
  `status`, `compact`, and `purge`.
