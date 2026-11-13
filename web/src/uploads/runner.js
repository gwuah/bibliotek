import * as api from "../api.js";
import { DEFAULT_CONCURRENCY, nextStep, nextWakeup, reduce, resumeKeyOf, startable } from "./queue.js";
import { uploadMetadata } from "../metadata.js";

const isPdf = (file) => file.type === "application/pdf" || file.name.toLowerCase().endsWith(".pdf");

function createRunner() {
  let snapshot = { entries: [], recovery: "pending" };
  const listeners = new Set();
  const committedListeners = new Set();
  const controllers = new Map();
  let wakeup = null;

  const entry = (key) => snapshot.entries.find((candidate) => candidate.key === key);

  function publish(changes) {
    snapshot = { ...snapshot, ...changes };
    listeners.forEach((listener) => listener());
  }

  function dispatch(event) {
    publish({ entries: reduce(snapshot.entries, event) });
  }

  function schedule() {
    clearTimeout(wakeup);
    let key;
    while ((key = startable(snapshot.entries, DEFAULT_CONCURRENCY, Date.now())[0])) {
      dispatch({ type: "started", key });
      work(key);
    }
    const next = nextWakeup(snapshot.entries, Date.now());
    if (next !== null) wakeup = setTimeout(schedule, next - Date.now());
  }

  function perform(step, current, signal) {
    const { upload } = current;
    switch (step.type) {
      case "create":
        return api.createUpload(
          {
            filename: current.filename,
            size: current.size,
            resume_key: current.key,
            metadata: uploadMetadata(current.metadata),
          },
          signal,
        );
      case "refresh":
        return api.getUpload(upload.id);
      case "put":
        return api.putUploadPart(upload.id, step.part, current.file.slice(step.start, step.end), signal);
      case "complete":
        return api.completeUpload(upload.id, signal);
      default:
        throw new Error(`no request for step ${step.type}`);
    }
  }

  async function work(key) {
    const controller = new AbortController();
    controllers.set(key, controller);
    try {
      for (;;) {
        const current = entry(key);
        if (current?.phase !== "active") return;
        const step = nextStep(current);
        if (!step) return;
        if (step.type === "finish") {
          dispatch({ type: "finished", key });
          committedListeners.forEach((listener) => listener(current.upload.document_id));
          return;
        }
        dispatch({ type: "progressed", key, upload: await perform(step, current, controller.signal) });
      }
    } catch (error) {
      if (error.name !== "AbortError" && entry(key)?.phase === "active") {
        dispatch({ type: "failed", key, error, now: Date.now() });
      }
    } finally {
      controllers.delete(key);
      schedule();
    }
  }

  function pause(key) {
    dispatch({ type: "paused", key });
    controllers.get(key)?.abort();
    schedule();
  }

  async function readMetadata(files) {
    const { readPdfMetadata } = await import("./pdfMetadata.js");
    for (const file of files) {
      const metadata = await readPdfMetadata(file);
      dispatch({ type: "metadata_read", key: resumeKeyOf(file), metadata });
    }
  }

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getSnapshot: () => snapshot,

    onCommitted(listener) {
      committedListeners.add(listener);
      return () => committedListeners.delete(listener);
    },

    async recover() {
      if (snapshot.recovery !== "pending") return;
      publish({ recovery: "loading" });
      try {
        dispatch({ type: "recovered", uploads: await api.listUploads() });
        publish({ recovery: "done" });
      } catch (error) {
        console.error("Failed to fetch pending uploads:", error);
        publish({ recovery: "pending" });
      }
    },

    add(fileList) {
      const files = [...fileList].filter(isPdf);
      dispatch({ type: "added", files });
      readMetadata(files.filter((file) => entry(resumeKeyOf(file))?.reading));
    },

    start() {
      dispatch({ type: "queued" });
      schedule();
    },

    async remove(key) {
      pause(key);
      const { upload } = entry(key) ?? {};
      try {
        if (upload && upload.status !== "committed") await api.abortUpload(upload.id);
        dispatch({ type: "removed", key });
      } catch (error) {
        if (error.status === 404) dispatch({ type: "removed", key });
        else dispatch({ type: "abort_failed", key, error });
      }
    },
  };
}

export const uploadRunner = createRunner();
