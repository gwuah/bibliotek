import { describe, expect, it } from "vitest";
import {
  MAX_ATTEMPTS,
  backoffDelay,
  canQueue,
  missingParts,
  needsFile,
  nextStep,
  nextWakeup,
  partRange,
  progressOf,
  reduce,
  resumeKeyOf,
  startable,
} from "./queue.js";

const file = (name = "book.pdf", size = 20, lastModified = 1700000000000) => ({
  name,
  size,
  lastModified,
  type: "application/pdf",
});

const session = (fields = {}) => ({
  id: "u1",
  status: "uploading",
  filename: "book.pdf",
  file_size: 20,
  part_size: 8,
  total_parts: 3,
  uploaded_parts: [],
  resume_key: resumeKeyOf(file()),
  metadata: { title: null, description: null, authors: [], subjects: [], tags: [] },
  document_id: null,
  ...fields,
});

function queueWith(...events) {
  const key = resumeKeyOf(file());
  const start = [
    { type: "added", files: [file()] },
    { type: "metadata_read", key, metadata: { title: "", description: "", authors: [], subjects: [], tags: [] } },
  ];
  return [...start, ...events].reduce(reduce, []);
}

const only = (entries) => entries[0];
const key = resumeKeyOf(file());
const serverError = { status: 503, code: "object_store_failed", message: "store unavailable" };

describe("parts", () => {
  it("gives every part the part size except the last, which holds the remainder", () => {
    const upload = session();
    expect(partRange(upload, 1)).toEqual({ start: 0, end: 8 });
    expect(partRange(upload, 2)).toEqual({ start: 8, end: 16 });
    expect(partRange(upload, 3)).toEqual({ start: 16, end: 20 });
  });

  it("finds the gaps in a sparse part list", () => {
    expect(missingParts(session({ total_parts: 5, uploaded_parts: [1, 2, 4] }))).toEqual([3, 5]);
  });

  it("measures progress in bytes, counting a short last part as short", () => {
    const entries = queueWith({ type: "queued" }, { type: "started", key });
    const withParts = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [3] }) });
    expect(progressOf(only(withParts))).toBeCloseTo(4 / 20);
  });

  it("derives the resume key from name, size and modification time", () => {
    expect(resumeKeyOf(file("a.pdf", 5, 42))).toBe("a.pdf:5:42");
  });
});

describe("next step", () => {
  it("creates a session first, then sends only the missing parts, then completes", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    expect(nextStep(only(entries))).toEqual({ type: "create" });

    entries = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [1, 3] }) });
    expect(nextStep(only(entries))).toEqual({ type: "put", part: 2, start: 8, end: 16 });

    entries = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [1, 2, 3] }) });
    expect(nextStep(only(entries))).toEqual({ type: "complete" });
  });

  it("finishes once the server has committed the upload", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, {
      type: "progressed",
      key,
      upload: session({ status: "committed", uploaded_parts: [1, 2, 3], document_id: 9 }),
    });
    expect(nextStep(only(entries))).toEqual({ type: "finish" });
  });
});

describe("completion after upload_incomplete", () => {
  it("re-reads the session, sends the parts the server lacks, and completes again", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [1, 2, 3] }) });
    expect(nextStep(only(entries)).type).toBe("complete");

    entries = reduce(entries, {
      type: "failed",
      key,
      now: 1000,
      error: { status: 409, code: "upload_incomplete", message: "part 2 is missing" },
    });
    expect(only(entries)).toMatchObject({ phase: "queued", stale: true, retryAt: 1000 });
    expect(startable(entries, 1, 1000)).toEqual([key]);

    entries = reduce(entries, { type: "started", key });
    expect(nextStep(only(entries))).toEqual({ type: "refresh" });

    entries = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [1, 3] }) });
    expect(nextStep(only(entries))).toEqual({ type: "put", part: 2, start: 8, end: 16 });
  });
});

describe("retry policy", () => {
  it("backs off exponentially from one second, capped at thirty", () => {
    expect([1, 2, 3, 4, 5, 6, 7].map(backoffDelay)).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000]);
  });

  it("requeues a transient failure after its backoff and frees the slot meanwhile", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, { type: "failed", key, now: 5000, error: serverError });
    expect(only(entries)).toMatchObject({ phase: "queued", attempts: 1, retryAt: 6000, error: "store unavailable" });
    expect(startable(entries, 3, 5999)).toEqual([]);
    expect(nextWakeup(entries, 5999)).toBe(6000);
    expect(startable(entries, 3, 6000)).toEqual([key]);
  });

  it("gives up after the maximum number of attempts", () => {
    let entries = queueWith({ type: "queued" });
    for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
      entries = reduce(entries, { type: "started", key });
      entries = reduce(entries, { type: "failed", key, now: 0, error: serverError });
    }
    expect(only(entries)).toMatchObject({ phase: "failed", attempts: MAX_ATTEMPTS });
  });

  it("fails at once on an error that retrying cannot fix", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, {
      type: "failed",
      key,
      now: 0,
      error: { status: 400, code: "file_too_large", message: "too many parts" },
    });
    expect(only(entries)).toMatchObject({ phase: "failed", error: "too many parts" });
  });

  it("treats network failures as transient", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, { type: "failed", key, now: 0, error: { status: 0, code: "network_error", message: "offline" } });
    expect(only(entries).phase).toBe("queued");
  });

  it("pauses rather than fails when the admin token is declined", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, {
      type: "failed",
      key,
      now: 0,
      error: { status: 401, code: "admin_token_required", message: "needs the token" },
    });
    expect(only(entries)).toMatchObject({ phase: "paused", error: "needs the token" });
  });
});

describe("pause and resume", () => {
  it("takes a paused entry out of the schedule and re-reads its session on resume", () => {
    let entries = queueWith({ type: "queued" }, { type: "started", key });
    entries = reduce(entries, { type: "progressed", key, upload: session({ uploaded_parts: [1] }) });
    entries = reduce(entries, { type: "paused", key });
    expect(only(entries).phase).toBe("paused");
    expect(startable(entries, 3, Infinity)).toEqual([]);

    entries = reduce(entries, { type: "queued", key });
    expect(only(entries)).toMatchObject({ phase: "queued", stale: true });
    entries = reduce(entries, { type: "started", key });
    expect(nextStep(only(entries))).toEqual({ type: "refresh" });
  });
});

describe("scheduling", () => {
  it("starts no more entries than the concurrency limit allows", () => {
    const files = ["a", "b", "c", "d"].map((name) => file(`${name}.pdf`));
    let entries = reduce([], { type: "added", files });
    for (const f of files) entries = reduce(entries, { type: "metadata_read", key: resumeKeyOf(f), metadata: only(entries).metadata });
    entries = reduce(entries, { type: "queued" });
    entries = reduce(entries, { type: "started", key: resumeKeyOf(files[0]) });
    expect(startable(entries, 2, 0)).toEqual([resumeKeyOf(files[1])]);
  });

  it("does not start a file whose own metadata is still being read", () => {
    const entries = reduce(reduce([], { type: "added", files: [file()] }), { type: "queued" });
    expect(only(entries).phase).toBe("ready");
    expect(canQueue(only(entries))).toBe(false);
  });
});

describe("resuming after a reload", () => {
  it("waits for the file, then matches it by resume key and re-reads the session", () => {
    let entries = reduce([], { type: "recovered", uploads: [session({ uploaded_parts: [1, 3] })] });
    expect(needsFile(only(entries))).toBe(true);
    expect(canQueue(only(entries))).toBe(false);

    entries = reduce(entries, { type: "added", files: [file()] });
    expect(entries).toHaveLength(1);
    expect(only(entries)).toMatchObject({ stale: true, reading: false });
    expect(canQueue(only(entries))).toBe(true);
  });

  it("can finish a recovered session that only needs completing, without the file", () => {
    const entries = reduce([], {
      type: "recovered",
      uploads: [session({ status: "object_ready", uploaded_parts: [1, 2, 3] })],
    });
    expect(needsFile(only(entries))).toBe(false);
    expect(canQueue(only(entries))).toBe(true);
  });

  it("marks a session the server is aborting as failed", () => {
    const entries = reduce([], { type: "recovered", uploads: [session({ status: "aborting" })] });
    expect(only(entries).phase).toBe("failed");
  });
});
