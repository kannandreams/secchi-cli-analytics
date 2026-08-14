//! Core library for Secchi CLI Analytics: the frozen event schema, the
//! redaction engine, the spool writer, and (behind the `store` feature) the
//! DuckDB-backed local store.
//!
//! Everything privacy-sensitive lives in this crate so it can be audited in
//! one place. Framework shims depend on the default features only, which
//! keeps their dependency tree small and DuckDB-free.

pub mod error;
pub mod event;
pub mod redact;
