CREATE TABLE upload_sessions (
    id                 TEXT    PRIMARY KEY,
    status             TEXT    NOT NULL CHECK (status IN (
                           'created', 'uploading', 'completing', 'object_ready',
                           'committed', 'failed', 'aborting', 'aborted', 'expired')),
    provider_upload_id TEXT    NOT NULL,
    object_key         TEXT    NOT NULL UNIQUE,
    original_filename  TEXT    NOT NULL,
    file_size          INTEGER NOT NULL CHECK (file_size > 0),
    part_size          INTEGER NOT NULL CHECK (part_size > 0),
    client_resume_key  TEXT,
    metadata_json      TEXT    NOT NULL DEFAULT '{}',
    document_id        INTEGER REFERENCES documents (id) ON DELETE SET NULL,
    last_error         TEXT,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at         TEXT    NOT NULL
);
CREATE INDEX upload_sessions_status ON upload_sessions (status);

CREATE TABLE upload_parts (
    upload_session_id TEXT    NOT NULL REFERENCES upload_sessions (id) ON DELETE CASCADE,
    part_number       INTEGER NOT NULL CHECK (part_number >= 1),
    size              INTEGER NOT NULL,
    etag              TEXT    NOT NULL,
    checksum          TEXT,
    PRIMARY KEY (upload_session_id, part_number)
);

CREATE TABLE import_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source      TEXT    NOT NULL,
    finished_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    stats_json  TEXT    NOT NULL
);
CREATE INDEX import_runs_source ON import_runs (source, id);
