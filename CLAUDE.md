# Working on liotek

Project background, commands, backups, and environments are in [ctx.md](ctx.md).

liotek is a personal library daemon (Rust/Axum + React/Vite, a local SQLite file opened
through libSQL, S3-compatible blobs).

The database is local only; there is no replication. It leaves the machine as backups:
exports uploaded to the object store (`src/backup.rs`). The store, not the database, is
the record of which backups exist.

liotek must never share runtime identity with Bibliotek, its predecessor, which may be
running on the same machine. Never read or write `~/.bibliotek`, port 5678, or Bibliotek's
bucket or Turso database from code, tests, or the shell.

## Layering

`main → http → domain → db/blob`, and `http → import → domain`. See `src/lib.rs`.

- `domain` functions take `&libsql::Connection`, never begin or commit. The caller (an
  HTTP handler or an importer) owns the transaction via `Db::begin()` / `WriteTx::commit()`.
  The one exception is `domain::uploads`, which takes `&Db` and owns its transactions
  because each operation interleaves object-store calls with database steps.
- Reads borrow a connection with `Db::read()` and hold it for one operation only; never
  begin a transaction on it. Never share one read connection between requests: every
  statement on a SQLite connection sees the same snapshot, so a commit made after another
  request's query began would be invisible. Every write goes through `Db::begin()`, which
  uses the dedicated writer connection.
- Never call `libsql::Database::connect()` directly; use `Db`, `ReadOnlyDb`, or
  `db::open_file`, which work around libsql 0.9 closing a connection twice (see `db.rs`).
  That workaround never closes a connection, so the daemon must not open one per
  recurring operation: `Db::snapshot_into` attaches its copy to the writer instead.
- Never hold a `WriteTx` across an object-store call; do the network step, then record it.
- Handlers return `Result<Data<T>, Error>`; JSON bodies are extracted with
  `envelope::Body<T>`. No handler builds an error response by hand.
- Each `http` module exposes `read_routes()` (open) and `write_routes()` (admin token).
  Reads only the owner may see, such as backups, go in `write_routes()` too.
- No generic repository trait, CRUD layer, event bus, or plugin system. `BlobStore` is the
  only trait boundary.

## Style

- Readability first: small functions named for what they do, types that make invalid
  states hard to write. Names carry the meaning, not comments.
- No comments by default, including doc comments. Write one only when it is essential:
  a hack or workaround, an edge case the code cannot make obvious, or context a reader
  could not recover from the code (an upstream bug, a provider quirk, a safety reason).
  Keep it to the point.
- Errors: use `crate::error::Error`; stable snake_case `code`s for `Invalid`/`Conflict`.
- SQL lives next to the function that runs it, as a formatted string literal.
- No `unwrap`/`expect` outside tests, except for true invariants with a message.
- `rustfmt.toml` sets `max_width = 110`. `make check` must pass.

## Schema

`migrations/` is one monotonic sequence registered in `MIGRATIONS` in `src/db.rs`. Until
the first production cutover the existing files may be edited; afterwards they are frozen
and changes are new files. No migration may drop user data.

## Tests

Integration tests live in `tests/` and use `tests/common::TestApp`: a real database and a
filesystem blob store in a temporary directory. No test may read the home directory or
reach a network service. Test names state the behaviour being guaranteed.

## Frontend

`web/` is a Vite + React app embedded into the binary from `web/dist`. All API calls go to
`/api/...`; the Vite dev server proxies `/api` to the daemon on port 6579.
