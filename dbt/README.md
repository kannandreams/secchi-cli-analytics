# dbt package (planned)

`dbt_secchi_analytics` lands here in Phase 3: staging, intermediate, and
mart models over the raw events table, runnable unmodified on DuckDB,
Postgres, Snowflake, and BigQuery. The local profile will point
`dbt-duckdb` at `~/.secchi/analytics/events.duckdb`.
