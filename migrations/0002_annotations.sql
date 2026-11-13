-- Rows with a document_source_id are owned by that source and removed when its complete
-- snapshot no longer contains them; rows without one are never touched by an import.

CREATE TABLE highlights (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id        INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    document_source_id INTEGER NOT NULL REFERENCES document_sources (id) ON DELETE CASCADE,
    external_id        TEXT    NOT NULL,
    text               TEXT    NOT NULL,
    color              TEXT,
    location_json      TEXT,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (document_source_id, external_id)
);
CREATE INDEX highlights_document ON highlights (document_id);

CREATE TABLE comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    highlight_id INTEGER NOT NULL REFERENCES highlights (id) ON DELETE CASCADE,
    external_id  TEXT,
    body         TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (highlight_id, external_id)
);

CREATE TABLE notes (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id        INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    document_source_id INTEGER REFERENCES document_sources (id) ON DELETE CASCADE,
    external_id        TEXT,
    body               TEXT    NOT NULL,
    created_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (document_source_id, external_id),
    CHECK ((document_source_id IS NULL) = (external_id IS NULL))
);
CREATE INDEX notes_document ON notes (document_id);
