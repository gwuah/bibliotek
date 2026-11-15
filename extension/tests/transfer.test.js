import assert from "node:assert/strict";
import { test } from "node:test";

import { normalizeInstallationId } from "../lib/installation.js";
import { ImportError, buildExport, parseExport, planImport } from "../lib/transfer.js";

const URL_A = "https://a.example/";
const UUID = "0f8fad5b-d9cb-469f-a165-70867728950e";

function highlight(groupID, extra = {}) {
  return { groupID, repr: "words", chunks: ["words"], url: URL_A, date: "2025-05-01T09:00:00.000Z", ...extra };
}

function exportText(installation, highlights) {
  return JSON.stringify({ format: "liotek-light-export", version: 1, installation, exported_at: "x", highlights });
}

test("installation ids keep their bare form and lose the old light_ prefix", () => {
  assert.equal(normalizeInstallationId(UUID), UUID);
  assert.equal(normalizeInstallationId(`light_${UUID}`), UUID);
  assert.equal(normalizeInstallationId(` ${UUID} `), UUID);
  assert.equal(normalizeInstallationId("has space"), null);
  assert.equal(normalizeInstallationId("light_"), null);
  assert.equal(normalizeInstallationId("x".repeat(129)), null);
  assert.equal(normalizeInstallationId(null), null);
});

test("an export parses back to the same installation and highlights", () => {
  const highlights = { [URL_A]: [highlight(1, { comments: [{ id: "c1", text: "a thought", created: null }] })] };
  const file = buildExport({ installationId: UUID, highlights, exportedAt: "2025-06-01T00:00:00.000Z" });
  assert.deepEqual(parseExport(JSON.stringify(file)), { installationId: UUID, highlights });
});

test("the old extension's light_ source id is accepted in an export", () => {
  assert.equal(parseExport(exportText(`light_${UUID}`, {})).installationId, UUID);
});

test("files liotek could not sync are refused with the reason", () => {
  const refused = [
    ["not json", /not JSON/],
    [JSON.stringify({ [URL_A]: [highlight(1)] }), /not a Light export/], // the old clipboard export
    [exportText("bad id!", {}), /installation ID/],
    [exportText(UUID, { [URL_A]: [highlight("1")] }), /groupID is not an integer/],
    [exportText(UUID, { [URL_A]: [highlight(1, { chunks: "words" })] }), /"chunks"/],
    [exportText(UUID, { [URL_A]: [highlight(1), highlight(1)] }), /appears twice/],
    [exportText(UUID, { [URL_A]: [highlight(1, { comments: [{ id: "c1", text: "  " }] })] }), /has no text/],
    [exportText(UUID, { [URL_A]: [highlight(1, { comments: [{ text: "x" }] })] }), /has no id/],
  ];
  for (const [text, reason] of refused) {
    assert.throws(() => parseExport(text), (error) => error instanceof ImportError && reason.test(error.message));
  }
});

test("an empty extension adopts the file's installation and highlights", () => {
  const incoming = { installationId: UUID, highlights: { [URL_A]: [highlight(1), highlight(2)] } };
  const plan = planImport({ installationId: "fresh", highlights: {} }, incoming);
  assert.equal(plan.installationId, UUID);
  assert.equal(plan.adopted, true);
  assert.equal(plan.added, 2);
  assert.deepEqual(plan.highlights, incoming.highlights);
});

test("the same installation's file merges into what is here", () => {
  const current = { installationId: UUID, highlights: { [URL_A]: [highlight(1)] } };
  const plan = planImport(current, { installationId: UUID, highlights: { [URL_A]: [highlight(1), highlight(2)] } });
  assert.equal(plan.installationId, UUID);
  assert.equal(plan.adopted, false);
  assert.equal(plan.added, 1);
});

test("another installation's file is refused once this one holds highlights", () => {
  const current = { installationId: "mine", highlights: { [URL_A]: [highlight(1)] } };
  assert.throws(
    () => planImport(current, { installationId: UUID, highlights: {} }),
    (error) => error instanceof ImportError && /already holds highlights/.test(error.message)
  );
});
