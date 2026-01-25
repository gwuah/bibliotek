# Upload System Fixes Plan

## Issues to Fix

1. Race Condition on Upload Completion
2. No Retry Button for Failed Uploads
3. Orphaned Upload Sessions
4. S3 and Database Out of Sync

---

## Issue #1: Race Condition on Upload Completion

**Problem:** Frontend ignores the book data returned by `/upload?state=complete` and triggers a full refresh via `loadData()`.

**File:** `web/static/js/App.jsx`

**Changes:**
- In `uploadFile()`, parse the JSON response from the complete endpoint
- Extract the book from `response.books[0]` if present
- Pass the book up to the parent via a new callback (e.g., `onBookCreated(book)`)
- In `BooksPage`, append the new book to state instead of refreshing

**Code locations:**
- `uploadFile()` at line 93-98
- `MassUploader` component props
- `BooksPage` at line 565

---

## Issue #2: Retry Button for Failed Uploads

**Problem:** No explicit way for users to retry failed uploads (though clicking Upload does reset errors).

**File:** `web/static/js/App.jsx`

**Changes:**
- Add a "Retry Failed" button in `MassUploader` that appears when there are files with `status: 'error'`
- Button resets error files to pending and triggers upload
- Clearer UX than relying on the main Upload button

**Code location:**
- Upload controls section at lines 186-195

---

## Issue #3: Orphaned Upload Sessions

**Problem:** Failed/abandoned uploads leave sessions in memory forever with no cleanup.

**Files:**
- `src/s3.rs`
- `src/main.rs`

**Changes in s3.rs:**
- Add `created_at: std::time::Instant` field to `UploadSession` struct
- Add method `cleanup_stale_sessions(&self, max_age_secs: u64)` that:
  - Locks sessions
  - Removes entries older than max_age
  - Returns list of removed upload_ids for S3 abort (optional)

**Changes in main.rs:**
- Spawn a tokio background task on startup
- Task runs every 5 minutes, calls `s3.cleanup_stale_sessions(1800)` (30 min)

---

## Issue #4: S3 and Database Out of Sync

**Problem:** S3 upload completes before DB insert. If DB fails, file is orphaned in S3.

**Files:**
- `src/handler.rs`
- `src/db.rs`
- `src/migrations/` (new migration)

**Changes:**

### Step 1: Add status column to books table
- Create migration `003_add_book_status.sql`
- Add `status TEXT DEFAULT 'complete'` column
- Existing books default to 'complete'

### Step 2: Update db.rs
- Modify `create_book()` to accept optional `status` parameter
- Add `update_book_status(book_id, status)` method
- Add `cleanup_pending_books(max_age_secs)` method for stale pending records

### Step 3: Update handler.rs complete flow
New order:
1. Extract metadata from chunks
2. Create book record with `status = 'pending'`
3. Complete S3 multipart upload
4. Update book status to `'complete'`
5. If S3 fails, delete the pending book record

### Step 4: Add cleanup for stale pending books (optional)
- In the same background task from Issue #3, also clean up pending books older than 1 hour

---

## Implementation Order

1. **Issue #1** - Quick frontend fix, immediate UX improvement
2. **Issue #2** - Small frontend addition
3. **Issue #3** - Backend session cleanup
4. **Issue #4** - Requires migration, most complex

---

## Testing Checklist

- [ ] Upload single file, verify it appears without full page refresh
- [ ] Upload multiple files concurrently, verify all appear
- [ ] Simulate network error, verify retry button works
- [ ] Abandon upload midway, verify session is cleaned up after 30 min
- [ ] Simulate DB failure after S3 complete, verify no orphaned files
