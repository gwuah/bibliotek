import assert from "node:assert/strict";
import { test } from "node:test";

import { SettingsError, hostPermissionFor, normalizeServerUrl } from "../lib/settings.js";
import { EMPTY_SNAPSHOT, NO_TOKEN, UNREACHABLE, buildSyncRequest, describeStats, sendSnapshot } from "../lib/sync.js";

const highlights = {
  "https://a.example/": [
    { groupID: 1, repr: "words", chunks: ["words"], url: "https://a.example/", date: "2025-05-01T09:00:00.000Z" },
  ],
};

const snapshot = {
  serverUrl: "http://127.0.0.1:5678",
  adminToken: "dev-admin-token",
  installationId: "abc-123",
  highlights,
};

const stats = {
  import_run_id: 1,
  source: "light:abc-123",
  documents: { created: 1, linked: 0, unchanged: 0, unlinked: 0, deleted: 0 },
  highlights: { created: 1, updated: 0, unchanged: 0, deleted: 0 },
  comments: { created: 0, updated: 0, unchanged: 0, deleted: 0 },
  notes: { created: 0, updated: 0, unchanged: 0, deleted: 0 },
  comments_rehomed_as_notes: 0,
};

function respond(status, body) {
  return async () => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

test("a sync is a PUT of exactly the highlights object, refusing empty snapshots by default", () => {
  const { url, init } = buildSyncRequest(snapshot);
  assert.equal(url, "http://127.0.0.1:5678/api/imports/light/abc-123?confirm_empty=false");
  assert.equal(init.method, "PUT");
  assert.equal(init.headers.Authorization, "Bearer dev-admin-token");
  assert.deepEqual(Object.keys(JSON.parse(init.body)), ["highlights"]);
  assert.deepEqual(JSON.parse(init.body).highlights, highlights);

  assert.match(buildSyncRequest({ ...snapshot, confirmEmpty: true }).url, /confirm_empty=true$/);
});

test("a successful sync returns the server's stats", async () => {
  assert.deepEqual(await sendSnapshot(snapshot, respond(200, { data: stats })), stats);
  assert.match(describeStats(stats), /\+1 new/);
});

test("server errors surface with their code and message", async () => {
  const body = { error: { code: EMPTY_SNAPSHOT, message: "would remove all 1 documents" } };
  await assert.rejects(sendSnapshot(snapshot, respond(409, body)), (error) => {
    assert.equal(error.code, EMPTY_SNAPSHOT);
    assert.equal(error.status, 409);
    assert.equal(error.message, "would remove all 1 documents");
    return true;
  });
});

test("an unreachable server and a missing token are explained, not thrown raw", async () => {
  const offline = async () => {
    throw new TypeError("Failed to fetch");
  };
  await assert.rejects(sendSnapshot(snapshot, offline), (error) => error.code === UNREACHABLE && /extension_origins/.test(error.message));
  await assert.rejects(sendSnapshot({ ...snapshot, adminToken: "" }, offline), (error) => error.code === NO_TOKEN);
});

test("a non-JSON failure still says what happened", async () => {
  const html = async () => new Response("<html>", { status: 502 });
  await assert.rejects(sendSnapshot(snapshot, html), /HTTP 502/);
});

test("server URLs are normalised and checked", () => {
  assert.equal(normalizeServerUrl(""), "http://127.0.0.1:5678");
  assert.equal(normalizeServerUrl(" http://host.local:5678/ "), "http://host.local:5678");
  assert.equal(normalizeServerUrl("https://lib.example/liotek/"), "https://lib.example/liotek");
  assert.throws(() => normalizeServerUrl("ftp://x"), SettingsError);
  assert.throws(() => normalizeServerUrl("http://x/?a=1"), SettingsError);
  assert.throws(() => normalizeServerUrl("nonsense"), SettingsError);
  assert.equal(hostPermissionFor("http://127.0.0.1:5678"), "http://127.0.0.1/*");
});
