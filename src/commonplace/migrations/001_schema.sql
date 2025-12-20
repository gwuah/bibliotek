-- Annotations Library Schema
-- Centralizes annotations across all reading devices

-- Resources: books, websites, PDFs, etc.
CREATE TABLE IF NOT EXISTS resources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    type TEXT NOT NULL CHECK (type IN ('website', 'pdf')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Annotations: highlights, underlines, etc. linked to a resource
CREATE TABLE IF NOT EXISTS annotations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    color TEXT,
    boundary TEXT, -- JSON storing position/location data
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);

-- Comments: notes attached to annotations
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    annotation_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (annotation_id) REFERENCES annotations (id) ON DELETE CASCADE
);

-- Notes: standalone notes linked to a resource (not tied to annotations)
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);

-- Words: vocabulary/definitions linked to a resource
CREATE TABLE IF NOT EXISTS words (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    resource_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    meaning TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    FOREIGN KEY (resource_id) REFERENCES resources (id) ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_annotations_resource_id ON annotations (resource_id);
CREATE INDEX IF NOT EXISTS idx_comments_annotation_id ON comments (annotation_id);
CREATE INDEX IF NOT EXISTS idx_notes_resource_id ON notes (resource_id);
CREATE INDEX IF NOT EXISTS idx_words_resource_id ON words (resource_id);
CREATE INDEX IF NOT EXISTS idx_words_name ON words (name);
