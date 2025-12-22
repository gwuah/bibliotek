-- Content hash for detecting modifications
ALTER TABLE resources ADD COLUMN content_hash TEXT;
ALTER TABLE annotations ADD COLUMN content_hash TEXT;
ALTER TABLE notes ADD COLUMN content_hash TEXT;
ALTER TABLE comments ADD COLUMN content_hash TEXT;

-- Soft deletes for all entities (preserves replicas when source deletes)
ALTER TABLE resources ADD COLUMN deleted_at TEXT;
ALTER TABLE annotations ADD COLUMN deleted_at TEXT;
ALTER TABLE notes ADD COLUMN deleted_at TEXT;
ALTER TABLE comments ADD COLUMN deleted_at TEXT;

-- Indexes for lookups
CREATE INDEX IF NOT EXISTS idx_resources_content_hash ON resources (content_hash);
CREATE INDEX IF NOT EXISTS idx_annotations_content_hash ON annotations (content_hash);
CREATE INDEX IF NOT EXISTS idx_notes_content_hash ON notes (content_hash);
CREATE INDEX IF NOT EXISTS idx_comments_content_hash ON comments (content_hash);

CREATE INDEX IF NOT EXISTS idx_resources_deleted_at ON resources (deleted_at);
CREATE INDEX IF NOT EXISTS idx_annotations_deleted_at ON annotations (deleted_at);
CREATE INDEX IF NOT EXISTS idx_notes_deleted_at ON notes (deleted_at);
CREATE INDEX IF NOT EXISTS idx_comments_deleted_at ON comments (deleted_at);

