# Go shim (planned)

The Cobra adapter for Secchi CLI Analytics lands here in Phase 2.

It will walk the command tree once at construction time, chain (not
replace) any existing `PersistentPreRunE`/`PersistentPostRunE` hooks, and
record command path and flag names via `cmd.CommandPath()` and
`cmd.Flags().Visit(...)`. Same event envelope, same privacy rules as every
other shim.
