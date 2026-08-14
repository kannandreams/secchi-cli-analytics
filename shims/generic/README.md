# Generic wrapper (planned)

`secchi-analytics wrap -- <binary> <args>` lands here in Phase 2, for CLIs
built on frameworks with no adapter yet.

A process-boundary wrapper only sees argv shape and exit codes, so its
data is coarser than a framework shim's: no resolved command path, no
flag-name/value distinction. It is the fallback, not the recommended
integration.
