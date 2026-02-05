-- Add config field for resource-specific settings (e.g., chapter boundaries for PDFs)
ALTER TABLE resources ADD COLUMN config TEXT;
