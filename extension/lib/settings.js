export const DEFAULT_SERVER_URL = "http://127.0.0.1:5678";

export class SettingsError extends Error {}

export function normalizeServerUrl(raw) {
  const text = String(raw ?? "").trim() || DEFAULT_SERVER_URL;
  let url;
  try {
    url = new URL(text);
  } catch {
    throw new SettingsError(`"${text}" is not a URL.`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new SettingsError("The server URL must start with http:// or https://.");
  }
  if (url.search || url.hash || url.username || url.password) {
    throw new SettingsError("The server URL must not carry a query, fragment, or credentials.");
  }
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}

// Chrome match patterns do not distinguish ports, so this covers the host on any port.
export function hostPermissionFor(serverUrl) {
  const url = new URL(serverUrl);
  return `${url.protocol}//${url.hostname}/*`;
}
