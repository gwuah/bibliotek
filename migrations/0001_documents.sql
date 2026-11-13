CREATE TABLE documents (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    kind              TEXT    NOT NULL CHECK (kind IN ('pdf', 'web')),
    title             TEXT    NOT NULL,
    description       TEXT,
    canonical_url     TEXT    UNIQUE,
    -- A key inside the configured object store, never a provider URL.
    object_key        TEXT    UNIQUE,
    original_filename TEXT,
    page_offset       INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE document_sources (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id  INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    source       TEXT    NOT NULL,
    external_id  TEXT    NOT NULL,
    last_seen_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (source, external_id)
);
CREATE INDEX document_sources_document ON document_sources (document_id);

CREATE TABLE authors (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT    NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE document_authors (
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    author_id   INTEGER NOT NULL REFERENCES authors (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    PRIMARY KEY (document_id, author_id)
);
CREATE INDEX document_authors_author ON document_authors (author_id);

CREATE TABLE labels (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT    NOT NULL CHECK (kind IN ('subject', 'tag')),
    name      TEXT    NOT NULL COLLATE NOCASE,
    parent_id INTEGER REFERENCES labels (id) ON DELETE SET NULL,
    UNIQUE (kind, name)
);

CREATE TABLE document_labels (
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    label_id    INTEGER NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (document_id, label_id)
);
CREATE INDEX document_labels_label ON document_labels (label_id);

CREATE TABLE sections (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    title       TEXT    NOT NULL,
    start_page  INTEGER NOT NULL,
    end_page    INTEGER NOT NULL,
    position    INTEGER NOT NULL,
    CHECK (start_page <= end_page),
    UNIQUE (document_id, position)
);

CREATE TABLE document_aliases (
    alias_document_id    INTEGER PRIMARY KEY,
    survivor_document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    merged_at            TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- `basis` is the evidence that made the pair a candidate; the pair is proposed again
-- only if it changes.
CREATE TABLE document_match_decisions (
    left_document_id  INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    right_document_id INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
    decision          TEXT    NOT NULL CHECK (decision IN ('dismissed')),
    basis             TEXT    NOT NULL,
    decided_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (left_document_id, right_document_id),
    CHECK (left_document_id < right_document_id)
);
