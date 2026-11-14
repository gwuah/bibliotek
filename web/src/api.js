const BASE = "/api";
const TOKEN_KEY = "liotek:admin-token";

export class ApiError extends Error {
  constructor(status, code, message) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

let promptForToken = async () => null;
let pendingPrompt = null;

export function setTokenPrompt(prompt) {
  promptForToken = prompt;
}

function storedToken() {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

function storeToken(token) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {}
}

function askForToken(rejected) {
  pendingPrompt ??= promptForToken({ rejected })
    .then((token) => {
      storeToken(token?.trim() || null);
      return token?.trim() || null;
    })
    .finally(() => {
      pendingPrompt = null;
    });
  return pendingPrompt;
}

const tokenDeclined = () =>
  new ApiError(401, "admin_token_required", "this change needs the admin token");

function url(path, query) {
  const params = new URLSearchParams();
  for (const [name, value] of Object.entries(query ?? {})) {
    if (value !== undefined && value !== null && value !== "") params.set(name, value);
  }
  const search = params.toString();
  return `${BASE}${path}${search ? `?${search}` : ""}`;
}

function encode(body) {
  if (body === undefined) return { headers: {} };
  if (body instanceof Blob) {
    return { headers: { "Content-Type": "application/octet-stream" }, body };
  }
  return { headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) };
}

async function send(method, path, { query, body, signal, token } = {}) {
  const encoded = encode(body);
  const headers = { Accept: "application/json", ...encoded.headers };
  if (token) headers.Authorization = `Bearer ${token}`;

  let response;
  try {
    response = await fetch(url(path, query), { method, headers, body: encoded.body, signal });
  } catch (error) {
    if (error.name === "AbortError") throw error;
    throw new ApiError(0, "network_error", "could not reach the liotek daemon");
  }

  const payload = await response.json().catch(() => null);
  if (response.ok && payload && "data" in payload) return payload.data;
  const error = payload?.error;
  throw new ApiError(
    response.status,
    error?.code ?? "unexpected_response",
    error?.message ?? `the server answered ${response.status} ${response.statusText}`,
  );
}

const read = (path, query) => send("GET", path, { query });

async function write(method, path, options = {}) {
  let token = storedToken();
  let rejected = false;
  for (;;) {
    token ??= await askForToken(rejected);
    if (!token) throw tokenDeclined();
    try {
      return await send(method, path, { ...options, token });
    } catch (error) {
      if (!(error instanceof ApiError) || error.status !== 401) throw error;
      storeToken(null);
      token = null;
      rejected = true;
    }
  }
}

export const listDocuments = (filters) => read("/documents", filters);

export async function listAllDocuments(filters) {
  const first = await listDocuments({ ...filters, page: 1 });
  const pages = Math.ceil(first.total / first.page_size);
  const rest = await Promise.all(
    Array.from({ length: Math.max(0, pages - 1) }, (_, i) => listDocuments({ ...filters, page: i + 2 })),
  );
  return [first, ...rest].flatMap((page) => page.documents);
}
export const getDocument = (id) => read(`/documents/${id}`);
export const updateDocument = (id, patch) => write("PATCH", `/documents/${id}`, { body: patch });
export const documentFileUrl = (id) => `${BASE}/documents/${id}/file`;
export const getFacets = () => read("/facets");

export const listReaderBooks = () => read("/reader/books");

export const listMatches = () => read("/document-matches");
export const mergeMatch = (id, survivorId) =>
  write("POST", `/document-matches/${id}/merge`, { body: { survivor_id: survivorId } });
export const dismissMatch = (id) => write("POST", `/document-matches/${id}/dismiss`);

export const listUploads = () => read("/uploads");
export const getUpload = (id) => read(`/uploads/${id}`);
export const createUpload = (request, signal) => write("POST", "/uploads", { body: request, signal });
export const putUploadPart = (id, number, bytes, signal) =>
  write("PUT", `/uploads/${id}/parts/${number}`, { body: bytes, signal });
export const completeUpload = (id, signal) => write("POST", `/uploads/${id}/complete`, { signal });
export const abortUpload = (id) => write("DELETE", `/uploads/${id}`);

export const importResearch = ({ confirmEmpty = false } = {}) =>
  write("POST", "/imports/research", { query: { confirm_empty: confirmEmpty || undefined } });
