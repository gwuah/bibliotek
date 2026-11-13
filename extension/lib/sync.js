export class SyncError extends Error {
  constructor(code, message, status = null) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

export const EMPTY_SNAPSHOT = "empty_snapshot";
export const UNREACHABLE = "unreachable";
export const NO_TOKEN = "no_admin_token";
export const NO_INSTALLATION = "no_installation_id";

// The body is always the whole library: liotek removes whatever this installation
// sent before and no longer holds. `confirmEmpty` may only be true once the person
// has agreed to an empty snapshot.
export function buildSyncRequest({ serverUrl, adminToken, installationId, highlights, confirmEmpty = false }) {
  const path = `/api/imports/light/${encodeURIComponent(installationId)}`;
  return {
    url: `${serverUrl}${path}?confirm_empty=${confirmEmpty ? "true" : "false"}`,
    init: {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${adminToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ highlights }),
    },
  };
}

export async function sendSnapshot(snapshot, fetchImpl = fetch) {
  if (!snapshot.adminToken) {
    throw new SyncError(NO_TOKEN, "Set the admin token in Settings before syncing.");
  }
  if (!snapshot.installationId) {
    throw new SyncError(NO_INSTALLATION, "This installation has no ID yet. Close the popup and try again.");
  }
  const request = buildSyncRequest(snapshot);

  let response;
  try {
    response = await fetchImpl(request.url, request.init);
  } catch (error) {
    throw new SyncError(
      UNREACHABLE,
      `Could not reach liotek at ${snapshot.serverUrl} (${error.message}). Check that the daemon ` +
        "is running and that this extension's origin is listed in extension_origins."
    );
  }

  const body = await readJson(response);
  if (response.ok && body?.data) return body.data;

  const error = body?.error;
  if (error?.message) throw new SyncError(error.code, error.message, response.status);
  throw new SyncError("unexpected_response", `liotek answered HTTP ${response.status} without an explanation.`, response.status);
}

async function readJson(response) {
  try {
    return await response.json();
  } catch {
    return null;
  }
}

export function describeStats(stats) {
  const parts = [`Highlights ${describeCounts(stats.highlights)}`];
  if (changed(stats.comments)) parts.push(`comments ${describeCounts(stats.comments)}`);
  if (stats.comments_rehomed_as_notes) {
    parts.push(`${stats.comments_rehomed_as_notes} comments kept as notes`);
  }
  return `Synced. ${parts.join(" · ")}`;
}

function describeCounts({ created = 0, updated = 0, deleted = 0, unchanged = 0 }) {
  return `+${created} new, ${updated} updated, ${deleted} removed, ${unchanged} unchanged`;
}

function changed({ created = 0, updated = 0, deleted = 0 } = {}) {
  return created + updated + deleted > 0;
}
