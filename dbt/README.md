# dbt package (deferred)

Local CLI event volume does not currently justify a modeling runtime. Secchi
uses purpose-built SQL in `core/src/store/query.rs` directly against DuckDB,
and the future local dashboard should call that same Rust query layer.

Reconsider `dbt_secchi_analytics` only when a shared collector or warehouse
needs reusable staging and marts across DuckDB, Postgres, Snowflake, or
BigQuery. Until then, keeping queries beside the application code makes them
easy to test, version, and inspect without another dependency.
