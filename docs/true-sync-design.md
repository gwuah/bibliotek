# True Sync Design

**Problem**: Current sync is insert-only with deduplication. When annotations are modified or deleted at the source, bibliotek has no way to detect or propagate these changes.

This affects both sync sources:

- **Light** (browser extension) — push-based via API
- **Research** (PDF reader app) — pull-based from local SQLite database

---

## Current Behavior

### Light Sync

| Source Action                     | Bibliotek Behavior                      |
| --------------------------------- | --------------------------------------- |
| New highlight                     | ✅ Created                              |
| Existing highlight (by `groupID`) | ⏭️ Skipped                              |
| Modified highlight                | ❌ Not detected (skipped as "existing") |
| Deleted highlight                 | ❌ Orphaned forever                     |

### Research Sync

| Source Action                   | Bibliotek Behavior                      |
| ------------------------------- | --------------------------------------- |
| New item/annotation/note        | ✅ Created                              |
| Existing (by `external_id`)     | ⏭️ Skipped                              |
| Modified annotation/note        | ❌ Not detected (skipped as "existing") |
| Deleted (soft delete in source) | ❌ Orphaned in bibliotek                |

**Note**: Research uses soft deletes (`deleted_at IS NULL`), so deleted records are filtered out during fetch—but bibliotek never removes the orphaned replicas.

---

## Solution: Checksum-Based Full State Reconciliation

Each sync reads the **complete state** from the source. Bibliotek performs a 3-way reconciliation:

1. **CREATE** — source record not found locally
2. **UPDATE** — source record exists locally but content differs
3. **SOFT DELETE** — local record not present in source (set `deleted_at`, preserve replica)

---

## Schema Changes

### Migration: `003_sync_metadata.sql`

```sql
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
CREATE INDEX idx_resources_content_hash ON resources (content_hash);
CREATE INDEX idx_annotations_content_hash ON annotations (content_hash);
CREATE INDEX idx_notes_content_hash ON notes (content_hash);
CREATE INDEX idx_comments_content_hash ON comments (content_hash);

CREATE INDEX idx_resources_deleted_at ON resources (deleted_at);
CREATE INDEX idx_annotations_deleted_at ON annotations (deleted_at);
CREATE INDEX idx_notes_deleted_at ON notes (deleted_at);
CREATE INDEX idx_comments_deleted_at ON comments (deleted_at);
```

---

## Source Identification

To scope deletions correctly (only soft-delete records from the source being synced), each record must identify its source.

### Approach: Prefix `external_id`

Format: `{source}:{original_id}`

The `source` identifier allows multiple instances of the same tool (e.g., Light running on different browsers/machines) to sync independently without interfering with each other.

Examples:

- `light-macbook:1766149301308` — highlight from Light on MacBook
- `light-work:1766149301308` — highlight from Light on work machine
- `research:abc123-def456` — annotation from Research app
- `kindle:B08XYZ_loc123` — (future) highlight from Kindle

**Advantages**:

- No schema change required
- Single column for lookup and source identification
- Easily extensible to new sources
- Supports multiple instances of the same tool

---

## Soft Deletes

When a record is deleted at the source, bibliotek **preserves the replica** by setting `deleted_at` instead of hard-deleting.

**Benefits**:

- Replicas survive source deletions (valuable if source data is lost)
- Audit trail
- Sync conflicts visible

### Query Rules

**All queries MUST filter out deleted items.** Once deleted, a record stays deleted.

```sql
-- ✅ Correct: Always filter deleted items
SELECT * FROM annotations WHERE deleted_at IS NULL

-- ✅ Correct: Lookups must also filter
SELECT * FROM annotations WHERE external_id = ? AND deleted_at IS NULL

-- ❌ Wrong: Never query without the filter
SELECT * FROM annotations WHERE external_id = ?
```

This means if a record is soft-deleted and later reappears at the source with the same ID, it will be treated as a **new record** (the deleted replica remains as a historical artifact).

**Soft delete operation**:

```sql
UPDATE annotations
SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = ?
```

---

## Light Sync Design

### Characteristics

- **Direction**: Push (extension → bibliotek API)
- **Entity types**: Resources (websites), Annotations (highlights)
- **ID format**: `{source}:{groupID}` (source is user-configurable, groupID is timestamp-based)

### Algorithm

```
INPUT: source, highlights_by_url

seen_external_ids = Set()
stats = { created: 0, updated: 0, deleted: 0, unchanged: 0 }

FOR EACH (url, highlights) IN highlights_by_url:
    resource = find_or_create_resource(url)

    FOR EACH highlight IN highlights:
        external_id = "{source}:{highlight.groupID}"
        content_hash = SHA256(highlight.repr)
        seen_external_ids.add(external_id)

        -- Note: find_* always filters deleted_at IS NULL
        existing = find_annotation_by_external_id(external_id)

        IF existing IS NULL:
            create_annotation(external_id, content_hash, ...)
            stats.created += 1
        ELSE IF existing.content_hash != content_hash:
            update_annotation(existing.id, content_hash, ...)
            stats.updated += 1
        ELSE:
            stats.unchanged += 1

-- Soft delete pass: mark orphaned records as deleted
orphans = find_annotations_where(
    external_id LIKE '{source}:%'
    AND deleted_at IS NULL
    AND external_id NOT IN seen_external_ids
)

FOR EACH orphan IN orphans:
    soft_delete_annotation(orphan.id)
    stats.deleted += 1

RETURN stats
```

### API Contract

**Request**:

```json
{
  "source": "light-macbook",
  "highlights": {
    "https://example.com/article": [
      {
        "groupID": 1766149301308,
        "repr": "highlighted text",
        "chunks": ["text snippet 1", "text snippet 2"],
        "date": "2025-12-19T13:01:41.308Z",
        "url": "https://example.com/article"
      }
    ]
  }
}
```

**Response**:

```json
{
  "data": {
    "resources_created": 1,
    "annotations_created": 5,
    "annotations_updated": 2,
    "annotations_deleted": 1,
    "annotations_unchanged": 12
  }
}
```

### Content Hash

```rust
fn compute_light_hash(highlight: &LightHighlight) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(highlight.repr.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

---

## Research Sync Design

### Characteristics

- **Direction**: Pull (bibliotek reads from Research's local SQLite)
- **Entity types**: Resources (PDFs), Annotations, Comments, Notes
- **ID format**: `research:{uuid}` (Research uses stable UUIDs)
- **Source soft deletes**: Research filters with `WHERE deleted_at IS NULL`

### Entity Hierarchy

```
Research Item (PDF)
  └── resources (external_id = "research:{item.id}")
        ├── annotations (external_id = "research:{annotation.id}")
        │     └── comments (external_id = "research:{comment.id}")
        └── notes (external_id = "research:{note.id}")
```

### Algorithm

```
INPUT: research_db_connection

-- Collect all seen IDs per entity type
seen_resources = Set()
seen_annotations = Set()
seen_comments = Set()
seen_notes = Set()

stats = {
    resources: { created: 0, updated: 0, deleted: 0, unchanged: 0 },
    annotations: { created: 0, updated: 0, deleted: 0, unchanged: 0 },
    comments: { created: 0, updated: 0, deleted: 0, unchanged: 0 },
    notes: { created: 0, updated: 0, deleted: 0, unchanged: 0 }
}

-- Phase 1: Upsert all entities
items = fetch_research_items()  -- WHERE deleted_at IS NULL

FOR EACH item IN items:
    external_id = "research:{item.id}"
    content_hash = SHA256(item.title)
    seen_resources.add(external_id)

    -- Note: find_* always filters deleted_at IS NULL
    existing = find_resource_by_external_id(external_id)

    IF existing IS NULL:
        resource_id = create_resource(external_id, content_hash, ...)
        stats.resources.created += 1
    ELSE IF existing.content_hash != content_hash:
        resource_id = update_resource(existing.id, content_hash, ...)
        stats.resources.updated += 1
    ELSE:
        resource_id = existing.id
        stats.resources.unchanged += 1

    -- Sync annotations for this item
    annotations = fetch_research_annotations(item.id)

    FOR EACH annotation IN annotations:
        ann_external_id = "research:{annotation.id}"
        ann_hash = SHA256(annotation.text + annotation.color)
        seen_annotations.add(ann_external_id)

        existing_ann = find_annotation_by_external_id(ann_external_id)

        IF existing_ann IS NULL:
            annotation_id = create_annotation(ann_external_id, ann_hash, ...)
            stats.annotations.created += 1
        ELSE IF existing_ann.content_hash != ann_hash:
            annotation_id = update_annotation(existing_ann.id, ann_hash, ...)
            stats.annotations.updated += 1
        ELSE:
            annotation_id = existing_ann.id
            stats.annotations.unchanged += 1

        -- Sync comments for this annotation
        comments = fetch_research_comments(annotation.id)

        FOR EACH comment IN comments:
            cmt_external_id = "research:{comment.id}"
            cmt_hash = SHA256(comment.content)
            seen_comments.add(cmt_external_id)

            existing_cmt = find_comment_by_external_id(cmt_external_id)

            IF existing_cmt IS NULL:
                create_comment(cmt_external_id, cmt_hash, ...)
                stats.comments.created += 1
            ELSE IF existing_cmt.content_hash != cmt_hash:
                update_comment(existing_cmt.id, cmt_hash, ...)
                stats.comments.updated += 1
            ELSE:
                stats.comments.unchanged += 1

    -- Sync notes for this item
    notes = fetch_research_notes(item.id)

    FOR EACH note IN notes:
        note_external_id = "research:{note.id}"
        note_hash = SHA256(note.content)
        seen_notes.add(note_external_id)

        existing_note = find_note_by_external_id(note_external_id)

        IF existing_note IS NULL:
            create_note(note_external_id, note_hash, ...)
            stats.notes.created += 1
        ELSE IF existing_note.content_hash != note_hash:
            update_note(existing_note.id, note_hash, ...)
            stats.notes.updated += 1
        ELSE:
            stats.notes.unchanged += 1

-- Phase 2: Soft delete orphans
-- Order doesn't matter for soft deletes (no FK constraint issues)

orphan_comments = find_comments_where(
    external_id LIKE 'research:%'
    AND deleted_at IS NULL
    AND external_id NOT IN seen_comments
)
FOR EACH orphan IN orphan_comments:
    soft_delete_comment(orphan.id)
    stats.comments.deleted += 1

orphan_annotations = find_annotations_where(
    external_id LIKE 'research:%'
    AND deleted_at IS NULL
    AND external_id NOT IN seen_annotations
)
FOR EACH orphan IN orphan_annotations:
    soft_delete_annotation(orphan.id)
    stats.annotations.deleted += 1

orphan_notes = find_notes_where(
    external_id LIKE 'research:%'
    AND deleted_at IS NULL
    AND external_id NOT IN seen_notes
)
FOR EACH orphan IN orphan_notes:
    soft_delete_note(orphan.id)
    stats.notes.deleted += 1

orphan_resources = find_resources_where(
    external_id LIKE 'research:%'
    AND deleted_at IS NULL
    AND external_id NOT IN seen_resources
)
FOR EACH orphan IN orphan_resources:
    soft_delete_resource(orphan.id)
    stats.resources.deleted += 1

RETURN stats
```

### API Response

```json
{
  "data": {
    "resources_created": 2,
    "resources_updated": 1,
    "resources_deleted": 0,
    "resources_unchanged": 15,
    "annotations_created": 10,
    "annotations_updated": 3,
    "annotations_deleted": 2,
    "annotations_unchanged": 45,
    "comments_created": 5,
    "comments_updated": 1,
    "comments_deleted": 0,
    "comments_unchanged": 20,
    "notes_created": 3,
    "notes_updated": 0,
    "notes_deleted": 1,
    "notes_unchanged": 12
  }
}
```

### Content Hash Functions

```rust
fn compute_resource_hash(item: &ResearchItem) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(item.title.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn compute_annotation_hash(annotation: &ResearchAnnotation) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(annotation.text.as_bytes());
    if let Some(color) = &annotation.color {
        hasher.update(color.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn compute_comment_hash(comment: &ResearchComment) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(comment.content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn compute_note_hash(note: &ResearchNote) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(note.content.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

---

## Edge Cases

### 1. Same highlight, different URLs (Light)

If a user moves a highlight to a different page (URL), it will appear as:

- SOFT DELETE from old resource (orphan detection)
- CREATE on new resource

This is correct behavior—the resource association changed. The old replica is preserved.

### 2. Partial sync scope (Light)

If the extension only syncs highlights for one URL, the deletion pass would incorrectly soft-delete all other highlights.

**Solution**: The request should include scope if partial:

```json
{
  "source": "light-macbook",
  "scope": "https://example.com/article",
  "highlights": { ... }
}
```

Deletion query becomes:

```sql
WHERE external_id LIKE 'light-macbook:%'
  AND deleted_at IS NULL
  AND resource_id = ?  -- scoped resource
  AND external_id NOT IN (...)
```

### 3. Research database unavailable

If the Research SQLite file is missing or locked:

- Sync should fail gracefully with an error
- No soft deletions should occur (can't distinguish "source unavailable" from "everything deleted")

---

## Implementation Checklist

### Schema & Core

- [ ] Create migration `003_sync_metadata.sql`
- [ ] Add `content_hash` and `deleted_at` to all entity structs
- [ ] Update all `Create*` / `Update*` DTOs to include `content_hash`
- [ ] Add `sha2` crate dependency

### Query Layer

- [ ] Add `find_*_by_source_prefix(prefix: &str)` queries for each entity
- [ ] Add `soft_delete_*` methods for each entity
- [ ] Add `update_*` methods that include `content_hash`
- [ ] **Update ALL queries to filter `WHERE deleted_at IS NULL`**

### Light Sync

- [ ] Modify `sync_highlights` handler with upsert + soft delete algorithm
- [ ] Update Light extension to send `source` field (user-configurable)
- [ ] Handle partial sync scope

### Research Sync

- [ ] Modify `sync` handler with full reconciliation algorithm
- [ ] Add orphan soft-deletion pass
- [ ] Handle database unavailable gracefully

---

## Future Considerations

### Bidirectional Sync

If bibliotek becomes the primary and needs to push changes back to sources, each source would need:

- A `last_synced_at` cursor
- An outbox/changelog table
- Source-specific push adapters

This is significantly more complex and should only be pursued if truly needed.

### Incremental Sync (Research)

Instead of full-state sync every time, Research could expose:

- `updated_at` timestamps on all entities
- Query: `WHERE updated_at > last_sync_at`

This would be more efficient for large libraries but requires:

- Reliable timestamp tracking in Research
- Handling clock skew
- Still need periodic full sync for soft deletes (or use tombstones)

### Hard Delete Cleanup

Periodically purge soft-deleted records older than N days:

```sql
DELETE FROM annotations
WHERE deleted_at IS NOT NULL
  AND deleted_at < datetime('now', '-90 days')
```
