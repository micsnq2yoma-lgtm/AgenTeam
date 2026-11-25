# Zero-Downtime Database Schema Migration Playbook

## Expand and Contract Pattern

Never execute destructive column renames or type narrowing in a single deployment. Follow four staged releases: (1) Expand by adding the nullable new column; (2) Dual-write from application code to both old and new columns while running a throttled backfill job; (3) Switch reads to the new column after verifying row parity; (4) Contract by removing old writes and dropping the legacy column in a subsequent release.

## Online Index Creation Guardrails

On PostgreSQL, all new indexes on tables exceeding 1 million rows must use `CREATE INDEX CONCURRENTLY` with `lock_timeout = '3s'` and `statement_timeout = '15min'` so migration scripts never block application traffic waiting behind an AccessExclusive lock.
