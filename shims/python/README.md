# Python shim (planned)

Click and Typer adapters for Secchi CLI Analytics land here in Phase 1.

The adapter will follow the same contract as the clap shim in
`../rust`: instrument once at CLI construction time, record command path
and flag names (never argv or values outside the allowlist), and degrade
to dropped events on any internal failure. The Python package will be pure
Python — it only appends JSON lines to the spool, so no compiled
dependency is required.

The first integration target is the `secchi` CLI itself.
