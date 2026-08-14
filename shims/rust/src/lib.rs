//! Instrumentation shim for clap-based CLIs. One call at startup and one at
//! exit record privacy-safe usage events (command path, flag names,
//! duration, exit code) into the local Secchi analytics spool.
