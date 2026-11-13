import assert from "node:assert/strict";
import { test } from "node:test";

const stored = {};
const listeners = { message: null };

globalThis.chrome = {
  runtime: {
    id: "this-extension",
    onInstalled: { addListener() {} },
    onMessage: { addListener: (fn) => (listeners.message = fn) },
  },
  contextMenus: { create() {}, onClicked: { addListener() {} } },
  tabs: { sendMessage() {} },
  storage: {
    onChanged: { addListener() {} },
    local: {
      async get(defaults) {
        return Object.fromEntries(
          Object.entries(defaults).map(([key, fallback]) => [key, key in stored ? structuredClone(stored[key]) : fallback])
        );
      },
      async set(values) {
        await new Promise((resolve) => setTimeout(resolve, 1));
        Object.assign(stored, structuredClone(values));
      },
    },
  },
};

await import("../background.js");

function send(message, sender = { id: "this-extension" }) {
  return new Promise((resolve) => {
    const async = listeners.message(message, sender, resolve);
    if (!async) resolve(undefined);
  });
}

const URL_A = "https://a.example/";
const highlight = (groupID) => ({ groupID, repr: "w", chunks: ["w"], url: URL_A, date: "2025-05-01T09:00:00.000Z" });

test("every key the extension stores carries the liotek: prefix", async () => {
  await send({ type: "addHighlight", highlight: highlight(1) });
  assert.ok(Object.keys(stored).length > 0);
  for (const key of Object.keys(stored)) assert.match(key, /^liotek:/);
  assert.ok(stored["liotek:installation-id"], "an installation id was generated");
});

test("changes sent together are applied one after another, none lost", async () => {
  await Promise.all([2, 3, 4, 5].map((id) => send({ type: "addHighlight", highlight: highlight(id) })));
  await send({ type: "setNote", groupId: "3", text: "a thought" });
  await send({ type: "deleteHighlight", groupId: "2" });
  const page = stored["liotek:highlights"][URL_A];
  assert.deepEqual(page.map((h) => h.groupID), [1, 3, 4, 5]);
  assert.equal(page[1].comments[0].text, "a thought");
});

test("importing another installation's file into a non-empty library is refused and changes nothing", async () => {
  const before = structuredClone(stored);
  const file = JSON.stringify({ format: "liotek-light-export", version: 1, installation: "other", highlights: {} });
  const reply = await send({ type: "importLibrary", fileText: file });
  assert.equal(reply.ok, false);
  assert.match(reply.error, /already holds highlights/);
  assert.deepEqual(stored, before);
});

test("an emptied library adopts an imported installation id", async () => {
  for (const id of [1, 3, 4, 5]) await send({ type: "deleteHighlight", groupId: String(id) });
  const file = JSON.stringify({
    format: "liotek-light-export",
    version: 1,
    installation: "light_0f8fad5b-d9cb-469f-a165-70867728950e",
    highlights: { [URL_A]: [highlight(9)] },
  });
  const reply = await send({ type: "importLibrary", fileText: file });
  assert.deepEqual(reply, {
    ok: true,
    value: { installationId: "0f8fad5b-d9cb-469f-a165-70867728950e", added: 1, commentsAdded: 0, adopted: true },
  });
  assert.equal(stored["liotek:installation-id"], "0f8fad5b-d9cb-469f-a165-70867728950e");
});

test("messages from elsewhere and unknown commands are ignored", async () => {
  assert.equal(await send({ type: "deleteHighlight", groupId: "9" }, { id: "another-extension" }), undefined);
  assert.equal(await send({ type: "toString" }), undefined);
  assert.equal(stored["liotek:highlights"][URL_A].length, 1);
});
