const WELL_FORMED = /^[A-Za-z0-9_-]{1,128}$/;

// The old extension sent its ID to Bibliotek as `light_<uuid>`; liotek's v1 importer
// strips that prefix, so this does too.
const LEGACY_PREFIX = "light_";

export function normalizeInstallationId(raw) {
  if (typeof raw !== "string") return null;
  let id = raw.trim();
  if (id.startsWith(LEGACY_PREFIX)) id = id.slice(LEGACY_PREFIX.length);
  return WELL_FORMED.test(id) ? id : null;
}

export function newInstallationId() {
  return crypto.randomUUID();
}
