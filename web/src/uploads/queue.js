import { metadataOf } from "../metadata.js";

export const DEFAULT_CONCURRENCY = 3;
export const MAX_ATTEMPTS = 6;
const BACKOFF_BASE_MS = 1000;
const BACKOFF_MAX_MS = 30_000;

const ACCEPTS_PARTS = ["created", "uploading"];
const NEEDS_COMPLETION = ["completing", "object_ready", "failed"];
const GONE = ["aborting", "aborted", "expired"];

export const resumeKeyOf = (file) => `${file.name}:${file.size}:${file.lastModified}`;

export function partRange(upload, number) {
  const start = (number - 1) * upload.part_size;
  return { start, end: Math.min(start + upload.part_size, upload.file_size) };
}

export function missingParts(upload) {
  const held = new Set(upload.uploaded_parts);
  const missing = [];
  for (let number = 1; number <= upload.total_parts; number++) {
    if (!held.has(number)) missing.push(number);
  }
  return missing;
}

export function progressOf(entry) {
  const { upload } = entry;
  if (!upload) return 0;
  if (upload.status === "committed") return 1;
  const bytes = upload.uploaded_parts.reduce((sum, number) => {
    const { start, end } = partRange(upload, number);
    return sum + (end - start);
  }, 0);
  return upload.file_size ? bytes / upload.file_size : 0;
}

export function needsFile(entry) {
  if (entry.file) return false;
  if (!entry.upload) return true;
  return ACCEPTS_PARTS.includes(entry.upload.status) && missingParts(entry.upload).length > 0;
}

export function nextStep(entry) {
  if (!entry.upload) return { type: "create" };
  if (entry.stale) return { type: "refresh" };

  const { status } = entry.upload;
  if (status === "committed") return { type: "finish" };
  if (ACCEPTS_PARTS.includes(status)) {
    const [part] = missingParts(entry.upload);
    if (part === undefined) return { type: "complete" };
    return entry.file ? { type: "put", part, ...partRange(entry.upload, part) } : null;
  }
  if (NEEDS_COMPLETION.includes(status)) return { type: "complete" };
  return null;
}

export function isTransient(error) {
  const status = error?.status;
  return (
    status === 0 ||
    status === 408 ||
    status === 429 ||
    status >= 500 ||
    error?.code === "upload_completing"
  );
}

export const backoffDelay = (attempt) =>
  Math.min(BACKOFF_MAX_MS, BACKOFF_BASE_MS * 2 ** Math.max(0, attempt - 1));

const canStart = (entry, now) => entry.phase === "queued" && entry.retryAt <= now;

export function startable(entries, concurrency, now) {
  const active = entries.filter((entry) => entry.phase === "active").length;
  return entries
    .filter((entry) => canStart(entry, now))
    .slice(0, Math.max(0, concurrency - active))
    .map((entry) => entry.key);
}

export function nextWakeup(entries, now) {
  const times = entries
    .filter((entry) => entry.phase === "queued" && entry.retryAt > now)
    .map((entry) => entry.retryAt);
  return times.length ? Math.min(...times) : null;
}

export const canQueue = (entry) =>
  ["ready", "paused", "failed"].includes(entry.phase) && !entry.reading && !needsFile(entry);

function newEntry(fields) {
  return {
    file: null,
    reading: false,
    upload: null,
    stale: false,
    phase: "ready",
    attempts: 0,
    retryAt: 0,
    error: null,
    ...fields,
  };
}

function settle(entry) {
  if (!entry.upload || !GONE.includes(entry.upload.status)) return entry;
  return {
    ...entry,
    phase: "failed",
    error: `the server has ${entry.upload.status === "aborting" ? "begun aborting" : entry.upload.status} this upload`,
  };
}

function added(entries, files) {
  const next = [...entries];
  for (const file of files) {
    const key = resumeKeyOf(file);
    const index = next.findIndex((entry) => entry.key === key);
    if (index === -1) {
      next.push(
        newEntry({
          key,
          file,
          filename: file.name,
          size: file.size,
          metadata: metadataOf({ title: "" }),
          reading: true,
        }),
      );
    } else if (!next[index].file && next[index].size === file.size) {
      next[index] = { ...next[index], file, stale: Boolean(next[index].upload) };
    }
  }
  return next;
}

function recovered(entries, uploads) {
  const known = new Set(entries.map((entry) => entry.key));
  const sessions = uploads
    .map((upload) => ({ upload, key: upload.resume_key || upload.id }))
    .filter(({ key }) => !known.has(key))
    .map(({ upload, key }) =>
      settle(
        newEntry({
          key,
          filename: upload.filename,
          size: upload.file_size,
          metadata: metadataOf(upload.metadata),
          upload,
          stale: true,
        }),
      ),
    );
  return [...entries, ...sessions];
}

function failed(entry, error, now) {
  const attempts = entry.attempts + 1;
  const message = error?.message ?? String(error);
  if (error?.code === "admin_token_required") {
    return { ...entry, phase: "paused", error: message };
  }
  if (attempts >= MAX_ATTEMPTS) {
    return { ...entry, phase: "failed", attempts, error: message };
  }
  if (error?.code === "upload_incomplete") {
    return { ...entry, phase: "queued", stale: true, attempts, retryAt: now, error: null };
  }
  if (isTransient(error)) {
    return { ...entry, phase: "queued", attempts, retryAt: now + backoffDelay(attempts), error: message };
  }
  return { ...entry, phase: "failed", attempts, error: message };
}

function queued(entry) {
  if (!canQueue(entry)) return entry;
  const gone = entry.upload && GONE.includes(entry.upload.status);
  return {
    ...entry,
    phase: "queued",
    attempts: 0,
    retryAt: 0,
    error: null,
    // A paused or failed entry may have sent a part the queue never heard back about.
    upload: gone ? null : entry.upload,
    stale: Boolean(entry.upload) && !gone,
  };
}

export function reduce(entries, event) {
  const update = (change) =>
    entries.map((entry) => (event.key === undefined || entry.key === event.key ? change(entry) : entry));

  switch (event.type) {
    case "added":
      return added(entries, event.files);
    case "recovered":
      return recovered(entries, event.uploads);
    case "metadata_read":
      return update((entry) => ({
        ...entry,
        reading: false,
        metadata: entry.upload ? entry.metadata : event.metadata,
      }));
    case "queued":
      return update(queued);
    case "started":
      return update((entry) => (entry.phase === "queued" ? { ...entry, phase: "active" } : entry));
    case "progressed":
      return update((entry) =>
        settle({ ...entry, upload: event.upload, stale: false, attempts: 0, error: null }),
      );
    case "finished":
      return update((entry) => ({ ...entry, phase: "done", error: null }));
    case "failed":
      return update((entry) => failed(entry, event.error, event.now));
    case "paused":
      return update((entry) =>
        ["queued", "active"].includes(entry.phase) ? { ...entry, phase: "paused" } : entry,
      );
    case "abort_failed":
      return update((entry) => ({ ...entry, phase: "paused", error: event.error.message }));
    case "removed":
      return entries.filter((entry) => entry.key !== event.key);
    default:
      throw new Error(`unknown upload queue event: ${event.type}`);
  }
}
