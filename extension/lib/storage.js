import { newInstallationId } from "./installation.js";
import { DEFAULT_SERVER_URL } from "./settings.js";

const KEYS = {
  highlights: "liotek:highlights",
  installationId: "liotek:installation-id",
  serverUrl: "liotek:server-url",
  adminToken: "liotek:admin-token",
  lastSync: "liotek:last-sync",
};

const local = () => chrome.storage.local;

export async function loadLibrary() {
  const stored = await local().get({ [KEYS.highlights]: {}, [KEYS.installationId]: null });
  return { installationId: stored[KEYS.installationId], highlights: stored[KEYS.highlights] };
}

export async function loadHighlights() {
  return (await loadLibrary()).highlights;
}

let queue = Promise.resolve();

// Only the background service worker calls this: one writer with one queue keeps
// read-modify-writes of the whole library from overwriting each other.
export function updateLibrary(change) {
  const run = async () => {
    const current = await loadLibrary();
    current.installationId ??= newInstallationId();
    const next = change(current);
    await local().set({ [KEYS.highlights]: next.highlights, [KEYS.installationId]: next.installationId });
    return next;
  };
  const result = queue.then(run);
  queue = result.catch(() => {});
  return result;
}

export function ensureInstallationId() {
  return updateLibrary((library) => library);
}

export function onHighlightsChanged(listener) {
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes[KEYS.highlights]) {
      listener(changes[KEYS.highlights].newValue || {});
    }
  });
}

export async function loadSettings() {
  const stored = await local().get({ [KEYS.serverUrl]: DEFAULT_SERVER_URL, [KEYS.adminToken]: "" });
  return { serverUrl: stored[KEYS.serverUrl], adminToken: stored[KEYS.adminToken] };
}

export async function saveSettings({ serverUrl, adminToken }) {
  await local().set({ [KEYS.serverUrl]: serverUrl, [KEYS.adminToken]: adminToken });
}

export async function loadLastSync() {
  return (await local().get({ [KEYS.lastSync]: null }))[KEYS.lastSync];
}

export async function recordSync(stats) {
  await local().set({ [KEYS.lastSync]: { at: new Date().toISOString(), stats } });
}
