# liotek context

Background and reference for working on liotek. The README covers only getting started;
coding conventions are in [CLAUDE.md](CLAUDE.md).

## What it is

A personal library daemon: one binary that keeps documents, highlights, comments, and
notes in a local SQLite file, and the heavy objects (PDFs) behind an S3-compatible store.

- **Documents** are books, PDFs, and web pages. One document can be known to several
  sources at once: an upload, the Research app, a page highlighted in the Light extension.
- **Reads are shareable.** Anyone who can reach the daemon can list and read.
- **Writes are the owner's.** Every change needs the admin token.
- **The library is a file.** `liotek export` writes it as one consistent SQLite database.
- **Backups live in the object store**, made on request or on a schedule.

liotek is the successor to Bibliotek. The plan it implements is
`../bibliotek/docs/REARCHITECTURE_PLAN.md`.

## Commands

| Command | Purpose |
| --- | --- |
| `liotek serve` | Run the daemon. |
| `liotek doctor [--check-objects]` | Read-only report of build, schema, paths, port, and stores. |
| `liotek health` | Is the daemon up, and is it this exact build? |
| `liotek export --output FILE` | Write the library as one portable SQLite file. |
| `liotek backup` | Back the library up to the object store now. |
| `liotek backups list` | List the backups in the object store, newest first. |
| `liotek restore --from NAME_OR_FILE` | Restore a backup or export into an empty database location. |
| `liotek uploads gc [--dry-run]` | Expire abandoned upload sessions. |
| `liotek import-v1 --from SNAPSHOT [--dry-run]` | Import a Bibliotek database snapshot. |
| `liotek --version` | Version, Git revision, and supported schema version. |

Every command takes `--config PATH` (default `~/.liotek/config.yaml`). Relative paths in
the config resolve against the config file; secrets are written inline or read from an
environment variable (`{ env: NAME }`).

## Layout

```text
migrations/        the schema's single, monotonic lineage
src/
  main.rs          command line and daemon lifecycle
  config.rs        configuration, resolved and validated once
  db.rs            SQLite: migrations, connections, write transactions, snapshots
  blob/            the object-store boundary (S3 in production, a directory in development)
  domain/          the library's rules: documents, annotations, uploads, merging duplicates
  import/          one-way imports from Light, Research, and Bibliotek
  http/            the list / read / write API
  export.rs        the portable export
  backup.rs        backups in the object store, their schedule, and restore
  doctor.rs        the read-only self-check
web/               the React frontend (a replica of Bibliotek's), embedded into the binary
extension/         Light for liotek, the web-highlighting browser extension
scripts/           launchd install and uninstall
```

## Database

The database is one local SQLite file opened through libSQL. There is no replication:
Turso's synced database never pushes local writes (tursodatabase/turso#5237), so it was
removed in favour of backups. libSQL 0.9 also closes connections twice
(tursodatabase/libsql#2251, fixed upstream but unreleased); `src/db.rs` works around it by
never closing connections, which is why it keeps a fixed set of them.

## Backups

A backup is an export (every table, with upload and import bookkeeping emptied) uploaded
to the object store:

```text
<namespace>backups/manual/20261113T020304Z.sqlite      liotek backup, POST /api/backups
<namespace>backups/scheduled/20261113T020304Z.sqlite   the optional schedule
```

The store, not the database, is the record of which backups exist, so they can be listed
when the database is gone. A backup's name is its trigger and UTC time, e.g.
`manual-20261113T020304Z`.

- **On request:** `liotek backup`, or `POST /api/backups`. Manual backups are never
  deleted by liotek.
- **On a schedule:** `backups: { every_hours: 24, keep: 30 }` makes `serve` back up
  whenever the newest scheduled backup is `every_hours` old (straight away if none
  exists), then delete scheduled backups beyond the newest `keep`. A failed run is
  logged and retried one interval later.
- **Checking:** `liotek doctor --check-objects` reports the newest backup and its age,
  and warns if the schedule has fallen more than two intervals behind.
- **Downloading:** `GET /api/backups` and `GET /api/backups/{name}/file`. Every backup
  route needs the admin token, since a backup holds every annotation.

To restore, stop the daemon, move any old database aside, and run
`liotek restore --from manual-20261113T020304Z` (or `--from path/to/export.sqlite`). It
refuses to replace an existing database or to run while something listens on the
configured address, and verifies the copy (opens, passes `PRAGMA integrity_check`, has a
liotek schema no newer than the binary) before moving it into place.

## Environments

| Environment | Port | Data | Object store |
| --- | --- | --- | --- |
| development (`make dev`) | 6579 | `.dev/` in the checkout | filesystem, no credentials |
| preview | 6578 | `~/.liotek/preview` | Tigris bucket `liotek-preview` |
| production | 5678 at cutover | `~/.liotek` | Bibliotek's buckets, taken over last |

The development admin token is `dev-admin-token`. The preview config is
`~/.liotek/config.preview.yaml` (mode 0600; it holds the bucket key, which can reach only
`liotek-preview`).

The extension's ID is pinned by the `key` in `extension/manifest.json`, so its origin is
`chrome-extension://emeinmdallkcjcidlcliaiefahmimoel` on every machine; that is what
`extension_origins` lists. The matching private key, needed only to package the
extension, is `~/.liotek/extension-key.pem`.

## Isolation from Bibliotek

While both run on one machine they must never meet. liotek refuses to start with a
configuration that could reach Bibliotek's resources.

| | Bibliotek | liotek |
| --- | --- | --- |
| Binary | `bibliotek` | `liotek` |
| launchd label | `com.<user>.bibliotek` | `com.<user>.liotek` |
| Port | 5678 | 6578 preview, 6579 development |
| Config and data | `~/.bibliotek` | `~/.liotek` |
| Logs | `/tmp/bibliotek*.log` | `~/Library/Logs/liotek/` |
| Object namespace | bucket root | own bucket, or `liotek/<environment>/` |

Port 5678 is accepted only with `environment: production`, for the final cutover.

Bibliotek's `src/db.rs` ignores its config file's location and opens
`$MONO_DATA_DIR/<db>` or `~/.bibliotek/<db>`. Running a Bibliotek binary for reference
therefore needs both `HOME` and `MONO_DATA_DIR` pointed at a scratch directory, checked
with `lsof`, or it opens the live database.
