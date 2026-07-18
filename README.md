<h1 align="center">
    tower-sessions-toasty-store
</h1>

<p align="center">
    <a href="https://github.com/tokio-rs/toasty">Toasty</a> ORM session store for <code>tower-sessions</code>. One store, many backends: SQLite, PostgreSQL, MySQL, Turso.
</p>

[![tests](https://github.com/patte/tower-sessions-toasty-store/actions/workflows/rust.yml/badge.svg)](https://github.com/patte/tower-sessions-toasty-store/actions/workflows/rust.yml) [![crates.io](https://img.shields.io/crates/v/tower-sessions-toasty-store)](https://crates.io/crates/tower-sessions-toasty-store)

## Overview

This is a `SessionStore` for the [`tower-sessions`](https://github.com/maxcountryman/tower-sessions) middleware backed by the [Toasty](https://github.com/tokio-rs/toasty) ORM. The store uses only portable Toasty query-builder calls — no raw SQL — so a single code path serves every SQL backend Toasty supports. The integration test suite runs against SQLite, Turso, PostgreSQL, and MySQL.

It follows the structure of [`tower-sessions-stores`](https://github.com/maxcountryman/tower-sessions-stores) and [`tower-sessions-rusqlite-store`](https://github.com/patte/tower-sessions-rusqlite-store) for easy maintenance.

All contributions are welcome!

## 🤸 Usage

Check out the [counter example](./toasty-store/examples/counter.rs). Run it with `cargo run --example counter`.

The backend is selected by the connection URL scheme; enable the matching `toasty` cargo feature in your application:

```toml
tower-sessions-toasty-store = "0.1"
toasty = { version = "0.8", features = ["sqlite"] } # or postgresql, mysql, turso
```

### Store-owned database

The simplest setup — the store builds its own `toasty::Db` (and connection pool):

```rust
let session_store = ToastyStore::connect("sqlite::memory:").await?;
session_store.migrate().await?;
```

### Application-owned database

If your application already uses Toasty, share its `Db` (and pool) with the store. ⚠️ You **must** register the store's `TowerSession` model when building the `Db` — Toasty panics on queries for unregistered models:

```rust
use tower_sessions_toasty_store::{ToastyStore, TowerSession};

let db = toasty::Db::builder()
    .models(toasty::models!(TowerSession, crate::*))
    .connect("postgresql://user:pass@localhost:5432/mydb")
    .await?;
// db.push_schema().await?; or your toasty-cli migrations
let session_store = ToastyStore::new(db);
```

To namespace the session table, use `.table_name_prefix("myapp_")` on the `Db` builder — it applies to the store's `tower_sessions` table too.

## 🥐 Toasty limitations and deviations

Toasty is young. This table records every place this store deviates from the reference implementations ([`sqlx-store`](https://github.com/maxcountryman/tower-sessions-stores/tree/main/sqlx-store), [`rusqlite-store`](https://github.com/patte/tower-sessions-rusqlite-store)) *because of* a current Toasty limitation — so that as Toasty gains features, the workarounds can be spotted and undone. It doubles as the upgrade checklist when bumping the `toasty` dependency.

| Toasty limitation (as of 0.8) | What this store does instead | Revisit when Toasty… |
|---|---|---|
| No native upsert (`ON CONFLICT DO UPDATE`) | `save()`: transaction + select-then-branch, retry-as-update on insert race | gains an upsert/`create_or_update` builder |
| No affected-row count from update/delete | can't "try update, detect miss" — forces the select above | returns row counts from `exec()` |
| No structured unique-violation error | `create()`: transaction + exists-loop (upstream PostgresStore pattern) instead of insert-or-abort + catch | adds an `is_constraint_violation()`-style API |
| No `time` crate support (only `jiff`) | `expiry_date` stored as unix-seconds `i64` | adds `time::OffsetDateTime` field support |
| `push_schema()` not idempotent, no if-not-exists | `migrate()` probes the table first, only pushes on error | makes `push_schema` idempotent or exposes if-not-exists |
| Table name fixed at compile time (`#[table]`) | no `with_table_name()`; use `Db::builder().table_name_prefix(..)` | supports runtime table naming per model |
| Interactive transactions are SQL-only | DynamoDB unsupported | exposes conditional writes / DynamoDB transactions |
| Unregistered model panics (`invalid model ID`) | usage warning above + the `connect()` convenience constructor | surfaces a recoverable error instead |

## 🧪 Tests

This crate is covered by integration- and unit-tests.

The integration tests are copied from [`tower-sessions-stores`](https://github.com/maxcountryman/tower-sessions-stores) and kept in the `tests` crate. SQLite and Turso run out of the box (both fully local, in-memory):

```bash
cargo nextest run --test test_integration
```

PostgreSQL and MySQL need running servers (see [tests/docker-compose.yml](./tests/docker-compose.yml)) and are enabled by feature flags. The URLs are read at compile time:

```bash
(cd tests && docker compose up -d)
POSTGRES_URL="postgresql://postgres:postgres@localhost:5433/postgres" \
MYSQL_URL="mysql://root@localhost:3306/public" \
cargo nextest run --test test_integration --features postgresql,mysql
```

The unit tests are copied from [maxcountryman/tower-sessions/memory-store](https://github.com/maxcountryman/tower-sessions/blob/6ad8933b4f5e71f3202f0c1a28f194f3db5234c8/memory-store/src/lib.rs#L62) and located directly in `src/lib.rs`:

```bash
cargo nextest run -p tower-sessions-toasty-store
```

## 🙏 Credits

Most credits go to the authors of `tower-sessions`, `tower-sessions-stores`, and `toasty`.

<!-- 📦 Release
cargo publish --dry-run -p tower-sessions-toasty-store
-->
