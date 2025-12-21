-- Add external_id support for idempotent syncs from external sources
-- This allows tracking the original ID from source systems (Research, Light, etc.)

ALTER TABLE resources ADD COLUMN external_id TEXT;
ALTER TABLE annotations ADD COLUMN external_id TEXT;
ALTER TABLE notes ADD COLUMN external_id TEXT;
ALTER TABLE comments ADD COLUMN external_id TEXT;

-- Indexes for fast lookups by external_id
CREATE INDEX IF NOT EXISTS idx_resources_external_id ON resources (external_id);
CREATE INDEX IF NOT EXISTS idx_annotations_external_id ON annotations (external_id);
CREATE INDEX IF NOT EXISTS idx_notes_external_id ON notes (external_id);
CREATE INDEX IF NOT EXISTS idx_comments_external_id ON comments (external_id);

