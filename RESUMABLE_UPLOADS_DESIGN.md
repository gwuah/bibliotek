# Resumable Uploads Design Document

## Overview

This document describes the design for implementing resumable uploads in Bibliotek. The feature allows users to resume interrupted uploads without re-uploading the entire file.

**Key design principle:** S3 is the single source of truth. No database tables needed.

## Current State

### How uploads work today

```
┌──────────┐       ┌──────────┐       ┌──────────┐
│  Client  │       │  Server  │       │    S3    │
└────┬─────┘       └────┬─────┘       └────┬─────┘
     │                  │                  │
     │ POST /upload?state=init             │
     │ (file_name)      │                  │
     │─────────────────>│                  │
     │                  │ CreateMultipart  │
     │                  │─────────────────>│
     │                  │     upload_id    │
     │    upload_id     │<─────────────────│
     │<─────────────────│                  │
     │                  │ (store session   │
     │                  │  in memory)      │
     │                  │                  │
     │ POST /upload?state=continue         │
     │ (chunk, part_num)│                  │
     │─────────────────>│                  │
     │                  │ UploadPart       │
     │                  │─────────────────>│
     │                  │      etag        │
     │       ok         │<─────────────────│
     │<─────────────────│                  │
     │                  │                  │
     │     ... more chunks ...             │
     │                  │                  │
     │ POST /upload?state=complete         │
     │─────────────────>│                  │
     │                  │ CompleteMultipart│
     │                  │─────────────────>│
     │                  │       ok         │
     │                  │<─────────────────│
     │                  │                  │
     │                  │ INSERT book      │
     │                  │ (pending→complete)│
     │    book data     │                  │
     │<─────────────────│                  │
```

### Problems with current approach

1. **Sessions stored in memory** - Lost on server restart
2. **No chunk tracking** - Can't resume from a specific chunk
3. **No file identification** - Can't match a file to an existing upload
4. **ETags stored in memory** - Required for S3 CompleteMultipartUpload, lost on restart
5. **No visibility** - Users can't see uploads in progress

Note: S3 already tracks uploaded parts via `ListParts` and in-progress uploads via `ListMultipartUploads` - we just weren't using them.

## Proposed Design

### Goals

1. Uploads survive server restarts
2. Users can resume from the last successful chunk
3. Users see uploads in progress in the UI
4. Failed uploads can be retried without re-uploading completed chunks
5. Abandoned uploads are cleaned up automatically
6. **No database tables** - S3 is the only source of truth

### Non-Goals

1. Pause/resume mid-chunk (chunks are atomic)
2. Client-side chunk deduplication across different files
3. Upload queuing on the server side

## File Signature

The signature uniquely identifies a file for resume matching.

### Computation

```javascript
const computeSignature = async (file) => {
  const input = `${file.name}:${file.size}:${file.lastModified}`
  const encoder = new TextEncoder()
  const data = encoder.encode(input)
  const hashBuffer = await crypto.subtle.digest('SHA-256', data)
  const hashArray = Array.from(new Uint8Array(hashBuffer))
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('')
  return hashHex.substring(0, 16)  // First 16 hex chars = 64 bits
}

// Example:
// file: { name: "book.pdf", size: 52428800, lastModified: 1706234567890 }
// input: "book.pdf:52428800:1706234567890"
// signature: "a3f2b8c1e9d4f7a0"
```

### Why 16 hex characters?

- 16 hex chars = 64 bits of entropy
- Collision probability: ~1 in 18 quintillion for random inputs
- For our use case (same user's files), effectively zero collision risk
- Short enough for readable S3 keys
- URL-safe (hex characters only)

### Rust implementation

```rust
use sha2::{Sha256, Digest};

fn compute_signature(file_name: &str, file_size: i64, last_modified: i64) -> String {
    let input = format!("{}:{}:{}", file_name, file_size, last_modified);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])  // First 8 bytes = 16 hex chars
}
```

## S3 Key Structure

Minimal metadata encoded in the S3 object key:

```
Format: uploads/{signature}/{filename}

Example: uploads/a3f2b8c1e9d4f7a0/My%20Book.pdf
         │       │                └── URL-encoded filename
         │       └── 16-char signature
         └── Prefix for all resumable uploads
```

### What about file_size and chunk_size?

- **chunk_size**: Derived from `ListParts` - S3 returns the size of each part
- **file_size**: Client provides it on init/resume (they have the file)

```rust
const DEFAULT_CHUNK_SIZE: i64 = 5 * 1024 * 1024;  // 5MB

// Get chunk_size from first uploaded part
let chunk_size = response.parts()
    .first()
    .and_then(|p| p.size())
    .unwrap_or(DEFAULT_CHUNK_SIZE);  // No parts yet = use server default
```

**Edge case**: If an upload exists but has 0 parts, we use `DEFAULT_CHUNK_SIZE`. This is fine because:
- New uploads always use default
- Resumed uploads with 0 parts = same as new upload

### Key constraints

- S3 key limit: 1024 bytes
- Filename is URL-encoded to handle special characters
- Even with a 500-char filename, we're well under the limit

### Parsing the key

```rust
struct UploadMetadata {
    signature: String,
    file_name: String,
}

fn parse_upload_key(key: &str) -> Option<UploadMetadata> {
    // key: "uploads/a3f2b8c1e9d4f7a0/My%20Book.pdf"
    let parts: Vec<&str> = key.splitn(3, '/').collect();
    if parts.len() != 3 || parts[0] != "uploads" {
        return None;
    }
    Some(UploadMetadata {
        signature: parts[1].to_string(),
        file_name: urlencoding::decode(parts[2]).ok()?.to_string(),
    })
}
```

## S3 APIs Used

### ListMultipartUploads

Lists all in-progress multipart uploads in the bucket.

```rust
let response = client
    .list_multipart_uploads()
    .bucket(&bucket)
    .prefix("uploads/")  // Only our resumable uploads
    .send()
    .await?;

for upload in response.uploads() {
    let key = upload.key().unwrap_or_default();
    let upload_id = upload.upload_id().unwrap_or_default();
    let initiated = upload.initiated();  // Timestamp

    if let Some(metadata) = parse_upload_key(key) {
        // Now we have: upload_id, signature, file_name
        // Call ListParts to get completed_chunks and chunk_size
    }
}
```

### ListParts

Lists all uploaded parts for a specific multipart upload.

```rust
let response = client
    .list_parts()
    .bucket(&bucket)
    .key(&key)
    .upload_id(&upload_id)
    .send()
    .await?;

let parts = response.parts();
let completed_count = parts.len();

// Derive chunk_size from first part's size
let chunk_size = parts
    .first()
    .and_then(|p| p.size())
    .unwrap_or(DEFAULT_CHUNK_SIZE);

// Sum bytes uploaded
let bytes_uploaded: i64 = parts.iter()
    .filter_map(|p| p.size())
    .sum();

// Build parts list for CompleteMultipartUpload
let completed_parts: Vec<CompletedPart> = parts
    .iter()
    .map(|p| CompletedPart::builder()
        .part_number(p.part_number())
        .e_tag(p.e_tag().unwrap_or_default())
        .build())
    .collect();
```

### Why no database?

| What we need | Where it comes from |
|--------------|---------------------|
| List pending uploads | `ListMultipartUploads` |
| Upload ID | `ListMultipartUploads` |
| File signature | Encoded in S3 key |
| Filename | Encoded in S3 key |
| Chunk size | `ListParts` (size of first part) |
| Completed parts count | `ListParts` |
| Created timestamp | `ListMultipartUploads.initiated` |
| File size | Client provides on init/resume |

Everything we need is in S3 or provided by the client.

## API Changes

### Modified: `POST /upload?state=init`

**Request:**
```
FormData:
  - file_name: string
  - file_size: integer
  - file_signature: string (16 hex chars)
```

**Response:**
```json
{
  "upload_id": "S3-generated-upload-id",
  "status": "ok",
  "chunk_size": 5242880,
  "total_chunks": 10,
  "resume_from_chunk": 0
}
```

**Behavior:**
1. Call `ListMultipartUploads` with prefix `uploads/{signature}/`
2. If found:
   - Call `ListParts` to get completed chunks and derive chunk_size from first part
   - Calculate total_chunks = ceil(file_size / chunk_size)
   - Return existing upload info with `resume_from_chunk` = completed count
3. If not found:
   - Use default chunk_size (5MB)
   - Calculate total_chunks = ceil(file_size / chunk_size)
   - Build key: `uploads/{signature}/{encoded_filename}`
   - Call `CreateMultipartUpload`
   - Return new upload info with `resume_from_chunk` = 0

### Modified: `POST /upload?state=continue`

**Request:**
```
FormData:
  - upload_id: string
  - chunk: bytes
  - part_number: integer
```

**Response:**
```json
{
  "status": "ok",
  "upload_id": "..."
}
```

**Behavior:**
1. Look up the key for this upload_id (via in-memory cache or ListMultipartUploads)
2. Call `UploadPart` to S3
3. Return success

Note: S3 automatically tracks the uploaded part. No database write needed.

### Modified: `POST /upload?state=complete`

**Request:**
```
FormData:
  - upload_id: string
```

**Response:**
```json
{
  "status": "upload completed and book created",
  "books": [{ ... }]
}
```

**Behavior:**
1. Get key for this upload_id
2. Call `ListParts` to get all parts with ETags
3. Call `CompleteMultipartUpload` with the parts
4. Parse filename from key, extract PDF metadata
5. Create book record
6. Return book data

### New: `GET /upload/pending`

**Request:**
```
No parameters
```

**Response:**
```json
{
  "uploads": [
    {
      "upload_id": "...",
      "file_name": "book.pdf",
      "file_signature": "a3f2b8c1e9d4f7a0",
      "completed_chunks": 5,
      "bytes_uploaded": 26214400,
      "created_at": "2024-01-25T12:00:00Z"
    }
  ]
}
```

**Behavior:**
1. Call `ListMultipartUploads` with prefix `uploads/`
2. For each upload:
   - Parse key to get signature and filename
   - Call `ListParts` to count completed chunks and sum bytes uploaded
3. Return list sorted by created_at (most recent first)

Note: We return `bytes_uploaded` (sum of part sizes) instead of percentage. Client calculates percentage when they select the file to resume (since they know file_size).

### New: `GET /upload/status`

**Request:**
```
Query params:
  - upload_id: string
```

**Response:**
```json
{
  "upload_id": "...",
  "file_name": "book.pdf",
  "file_signature": "a3f2b8c1e9d4f7a0",
  "completed_chunks": 5,
  "bytes_uploaded": 26214400,
  "chunk_size": 5242880,
  "created_at": "2024-01-25T12:00:00Z"
}
```

### New: `POST /upload/abort`

**Request:**
```
FormData:
  - upload_id: string
```

**Response:**
```json
{
  "status": "ok"
}
```

**Behavior:**
1. Get key for upload_id
2. Call `AbortMultipartUpload`
3. Return success

## Frontend Changes

### Key Principle: Unified Upload Queue

The upload queue shows both:
- Files currently uploading
- Pending uploads from server (incomplete, waiting for file)

No separate "pending uploads" view. Everything is in one place.

### File signature computation

```javascript
const computeSignature = async (file) => {
  const input = `${file.name}:${file.size}:${file.lastModified}`
  const encoder = new TextEncoder()
  const data = encoder.encode(input)
  const hashBuffer = await crypto.subtle.digest('SHA-256', data)
  const hashArray = Array.from(new Uint8Array(hashBuffer))
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('')
  return hashHex.substring(0, 16)
}
```

### Upload Queue State

```javascript
// Each entry in the upload queue
const uploadEntry = {
  id: string,                    // signature or unique id
  file_name: string,
  file_signature: string,
  status: 'pending' | 'uploading' | 'completed' | 'error',

  // For pending (from server, no file yet)
  bytes_uploaded: number,        // From server
  completed_chunks: number,      // From server

  // For uploading (has file)
  file: File | null,             // null if pending from server
  upload_id: string | null,
  chunk_size: number | null,
  total_chunks: number | null,
  current_chunk: number,
  progress: number,              // 0-100
}
```

### On Page Load

```javascript
const initializeUploadQueue = async () => {
  // Fetch pending uploads from server
  const res = await fetch('/upload/pending')
  const { uploads } = await res.json()

  // Add to queue as "pending" status (no file attached yet)
  const pendingEntries = uploads.map(u => ({
    id: u.file_signature,
    file_name: u.file_name,
    file_signature: u.file_signature,
    status: 'pending',
    bytes_uploaded: u.bytes_uploaded,
    completed_chunks: u.completed_chunks,
    file: null,
    upload_id: u.upload_id,
    chunk_size: null,
    total_chunks: null,
    current_chunk: u.completed_chunks,
    progress: 0,  // Can't calculate % without file_size
  }))

  setUploadQueue(pendingEntries)
}
```

### When User Selects Files

```javascript
const onFilesSelected = async (files) => {
  for (const file of files) {
    const signature = await computeSignature(file)

    // Check if this file matches a pending upload
    const existingEntry = uploadQueue.find(
      e => e.file_signature === signature && e.status === 'pending'
    )

    if (existingEntry) {
      // RESUME: Attach file to existing pending entry
      updateEntry(existingEntry.id, {
        file: file,
        status: 'uploading',
        progress: (existingEntry.bytes_uploaded / file.size) * 100,
      })
      resumeUpload(existingEntry, file)
    } else {
      // NEW: Add new entry to queue
      const newEntry = {
        id: signature,
        file_name: file.name,
        file_signature: signature,
        status: 'uploading',
        bytes_uploaded: 0,
        completed_chunks: 0,
        file: file,
        upload_id: null,
        chunk_size: null,
        total_chunks: null,
        current_chunk: 0,
        progress: 0,
      }
      addToQueue(newEntry)
      startUpload(newEntry, file)
    }
  }
}
```

### Start New Upload

```javascript
const startUpload = async (entry, file) => {
  // 1. Init
  const initRes = await fetch('/upload?state=init', {
    method: 'POST',
    body: formData({
      file_name: file.name,
      file_size: file.size,
      file_signature: entry.file_signature
    })
  })
  const { upload_id, chunk_size, total_chunks, resume_from_chunk } = await initRes.json()

  updateEntry(entry.id, { upload_id, chunk_size, total_chunks })

  // 2. Upload chunks
  await uploadChunks(entry.id, file, upload_id, chunk_size, total_chunks, resume_from_chunk)

  // 3. Complete
  await completeUpload(entry.id, upload_id)
}
```

### Resume Upload

```javascript
const resumeUpload = async (entry, file) => {
  // 1. Init (server will find existing upload and return resume point)
  const initRes = await fetch('/upload?state=init', {
    method: 'POST',
    body: formData({
      file_name: file.name,
      file_size: file.size,
      file_signature: entry.file_signature
    })
  })
  const { upload_id, chunk_size, total_chunks, resume_from_chunk } = await initRes.json()

  updateEntry(entry.id, { upload_id, chunk_size, total_chunks })

  // 2. Upload remaining chunks (starting from resume_from_chunk)
  await uploadChunks(entry.id, file, upload_id, chunk_size, total_chunks, resume_from_chunk)

  // 3. Complete
  await completeUpload(entry.id, upload_id)
}
```

### Upload Chunks (shared by new and resume)

```javascript
const uploadChunks = async (entryId, file, uploadId, chunkSize, totalChunks, startFrom) => {
  for (let i = startFrom; i < totalChunks; i++) {
    const start = i * chunkSize
    const end = Math.min(start + chunkSize, file.size)
    const chunk = file.slice(start, end)

    await fetch('/upload?state=continue', {
      method: 'POST',
      body: formData({
        upload_id: uploadId,
        chunk: chunk,
        part_number: i + 1  // 1-indexed
      })
    })

    updateEntry(entryId, {
      current_chunk: i + 1,
      bytes_uploaded: end,
      progress: ((i + 1) / totalChunks) * 100,
    })
  }
}

const completeUpload = async (entryId, uploadId) => {
  const res = await fetch('/upload?state=complete', {
    method: 'POST',
    body: formData({ upload_id: uploadId })
  })
  const data = await res.json()

  updateEntry(entryId, { status: 'completed', progress: 100 })

  if (data.books?.[0]) {
    onBookCreated(data.books[0])
  }
}
```

### Upload Queue UI

```jsx
const UploadQueue = ({ entries }) => (
  <div className="upload-queue">
    {entries.map(entry => (
      <div key={entry.id} className="upload-entry">
        <span className="filename">{entry.file_name}</span>

        {entry.status === 'pending' && (
          // No file attached yet - show bytes uploaded, prompt to select
          <div className="pending">
            <span>{formatBytes(entry.bytes_uploaded)} uploaded</span>
            <span className="hint">Select file to resume</span>
          </div>
        )}

        {entry.status === 'uploading' && (
          <div className="progress-bar" style={{ width: `${entry.progress}%` }} />
        )}

        {entry.status === 'completed' && (
          <span className="checkmark">✓</span>
        )}

        {entry.status === 'error' && (
          <button onClick={() => retryUpload(entry)}>Retry</button>
        )}

        {entry.status !== 'completed' && (
          <button onClick={() => cancelUpload(entry)}>×</button>
        )}
      </div>
    ))}
  </div>
)
```

### Cancel/Abort Upload

```javascript
const cancelUpload = async (entry) => {
  if (entry.upload_id) {
    await fetch('/upload/abort', {
      method: 'POST',
      body: formData({ upload_id: entry.upload_id })
    })
  }
  removeFromQueue(entry.id)
}
```

## Backend Changes

### Remove in-memory session storage

The `sessions: Arc<Mutex<HashMap<String, UploadSession>>>` in `s3.rs` is no longer needed.

### New module: `resumable_upload.rs`

```rust
pub struct ResumableUploadManager {
    client: Client,
    bucket: String,
}

impl ResumableUploadManager {
    /// Find existing upload by signature or create new one.
    pub async fn init_or_resume(
        &self,
        signature: &str,
        file_name: &str,
        file_size: i64,
    ) -> Result<InitResponse>

    /// List all pending uploads with their progress.
    pub async fn list_pending(&self) -> Result<Vec<PendingUpload>>

    /// Get status of a specific upload.
    pub async fn get_status(&self, upload_id: &str) -> Result<UploadStatus>

    /// Upload a part.
    pub async fn upload_part(
        &self,
        upload_id: &str,
        key: &str,
        data: Vec<u8>,
        part_number: i32,
    ) -> Result<()>

    /// Complete upload: ListParts + CompleteMultipartUpload.
    pub async fn complete(&self, upload_id: &str, key: &str) -> Result<String>

    /// Abort an upload.
    pub async fn abort(&self, upload_id: &str, key: &str) -> Result<()>

    /// Clean up uploads older than max_age.
    pub async fn cleanup_expired(&self, max_age_hours: u64) -> Result<usize>
}

const DEFAULT_CHUNK_SIZE: i64 = 5 * 1024 * 1024;  // 5MB

#[derive(Debug)]
pub struct InitResponse {
    pub upload_id: String,
    pub key: String,
    pub chunk_size: i64,       // From ListParts or DEFAULT_CHUNK_SIZE
    pub total_chunks: i64,     // ceil(file_size / chunk_size)
    pub resume_from_chunk: i64, // 0 for new, N for resume
}
```

### Cleanup task

```rust
// Every hour, clean up uploads older than 24 hours
let manager = resumable_upload_manager.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    loop {
        interval.tick().await;
        if let Err(e) = manager.cleanup_expired(24).await {
            tracing::warn!("Failed to cleanup expired uploads: {}", e);
        }
    }
});
```

Cleanup uses `ListMultipartUploads`, checks `initiated` timestamp, and calls `AbortMultipartUpload` for old ones.

## Migration Path

### Phase 1: Backend
1. Implement `ResumableUploadManager` with S3 APIs
2. Add new endpoints (`/upload/pending`, `/upload/status`, `/upload/abort`)
3. Modify existing upload handlers to use new key structure
4. Keep old in-memory logic as fallback

### Phase 2: Frontend
1. Add signature computation
2. Modify init to send file_size and signature
3. Handle resume_from_chunk in upload loop
4. Add UI for pending uploads

### Phase 3: Cleanup
1. Remove in-memory session storage from `s3.rs`
2. Remove old cleanup logic
3. Test resume after server restart

## User Flow Examples

### Normal Upload
```
1. User drops "book.pdf" onto upload zone
2. Client computes signature, adds entry to queue (status: uploading)
3. Client calls /upload?state=init → gets upload_id, chunk_size, total_chunks
4. Client uploads chunks 1→10, progress bar updates
5. Client calls /upload?state=complete → book created
6. Entry marked as completed ✓
```

### Page Refresh Mid-Upload
```
1. User is uploading "book.pdf" (5 of 10 chunks done)
2. User refreshes page (or closes browser)
3. Upload queue state is lost

--- Page reloads ---

4. On load, client calls GET /upload/pending
5. Server returns: [{ file_name: "book.pdf", signature: "abc123", bytes_uploaded: 25MB }]
6. Client shows entry in queue: "book.pdf - 25MB uploaded (select file to resume)"
7. User drops "book.pdf" again
8. Client computes signature → matches pending entry!
9. Client calls /upload?state=init → server returns resume_from_chunk: 5
10. Client uploads chunks 6→10
11. Complete → done ✓
```

### User Selects Wrong File
```
1. Pending upload shows: "book.pdf - 25MB uploaded"
2. User drops "other-book.pdf"
3. Client computes signature → no match
4. Treated as new upload, added to queue separately
5. Original pending "book.pdf" still waiting
```

## Edge Cases

### Server restart during upload
- **Before:** Upload lost, user must restart
- **After:** User can resume from last successful chunk (S3 retained everything)

### Network failure mid-chunk
- Chunk upload is atomic - either succeeds or fails
- Client retries the failed chunk
- S3 accepts the retry (overwrites previous attempt for same part_number)

### Duplicate chunk upload
- S3 handles this natively - uploading same part_number overwrites previous
- On resume, `ListParts` returns the latest version of each part

### File modified between resume
- `file_signature` includes `lastModified` timestamp
- Modified file = different signature = new upload

### S3 multipart upload expires
- S3 multipart uploads expire after 7 days by default
- Our cleanup runs at 24 hours (configurable)
- If expired: `ListParts` will fail, user must start fresh

### Concurrent uploads of same file
- Same signature = finds same upload via `ListMultipartUploads`
- Both clients work on same upload (wasteful but correct)
- Part uploads are idempotent

### Finding upload_id from signature
- `ListMultipartUploads` with prefix `uploads/{signature}/`
- Returns at most one match (one upload per signature)

## Security Considerations

1. **Signature spoofing** - Signature is convenience, not security. Malicious client could claim different file. Not a security issue for single-user app.
2. **Storage exhaustion** - Abandoned uploads consume S3 storage. Cleanup task handles this.
3. **Upload enumeration** - `ListMultipartUploads` could reveal pending uploads. Fine for single-user; add auth for multi-user.

## Monitoring

### Metrics to track
- Uploads started vs completed
- Resume rate (how often users resume)
- Average chunks per upload
- Cleanup: uploads expired per day

### Logging
- Log resume events: `"Resumed upload {signature} from chunk {n}/{total}"`
- Log completions: `"Completed upload {signature}: {chunks} chunks, {size} bytes"`
- Log expirations: `"Expired {count} uploads older than {hours}h"`

## Testing Plan

1. **Unit tests:**
   - Signature computation (JS and Rust produce same result)
   - Key parsing
   - ListParts response handling

2. **Integration tests:**
   - Full upload flow
   - Resume after server restart
   - Concurrent chunk uploads
   - Cleanup of expired uploads
   - ListMultipartUploads prefix filtering

3. **Manual tests:**
   - Upload large file, kill server mid-upload, restart, resume
   - Upload same file from two tabs
   - Modify file, try to resume (should start fresh)
   - Check S3 console to verify key structure

## Open Questions

1. How long before we expire incomplete uploads? (proposed: 24h)
2. What chunk size should the server use? (proposed: 5MB to meet S3 minimum)
3. Should cleanup be more aggressive for very old uploads (e.g., 7 days)?

## Appendix: S3 Multipart Upload Limits

- Minimum part size: 5MB (except last part)
- Maximum part size: 5GB
- Maximum parts: 10,000
- Maximum object size: 5TB
- Maximum key length: 1024 bytes

Our 5MB chunk size means:
- 10,000 parts × 5MB = 50GB maximum file size
- For larger files, increase chunk size dynamically

## Summary

By leveraging S3's native APIs (`ListMultipartUploads`, `ListParts`) and a minimal key structure, we achieve resumable uploads with:

- **Zero database tables** - S3 is the only source of truth
- **Minimal S3 key**: `uploads/{signature}/{filename}`
- **Derived data**: chunk_size from `ListParts`, file_size from client
- **Unified UI**: Upload queue shows both active and pending uploads
- **Simple resume**: Select same file → signature matches → resume automatically
- **Survives everything**: Server restart, browser refresh, network failure
