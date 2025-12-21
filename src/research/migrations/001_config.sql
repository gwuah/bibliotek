-- Research Module Configuration
-- Stores the path to the Research app's SQLite database

CREATE TABLE IF NOT EXISTS research_config (
    id INTEGER PRIMARY KEY CHECK (id = 1), -- Only one config row allowed
    db_path TEXT NOT NULL,
    last_sync_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

