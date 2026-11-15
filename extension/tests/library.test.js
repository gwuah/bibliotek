import assert from "node:assert/strict";
import { test } from "node:test";

import {
  addHighlight,
  countHighlights,
  groupBySite,
  highlightsForPage,
  mergeLibraries,
  mergeSamePages,
  noteOf,
  removeHighlight,
  setNote,
} from "../lib/library.js";

const clock = { now: "2025-06-01T10:00:00.000Z", newId: () => "c-new" };

function highlight(groupID, url, extra = {}) {
  return { groupID, repr: `text ${groupID}`, chunks: [`text ${groupID}`], url, date: "2025-05-01T09:00:00.000Z", ...extra };
}

test("adding a highlight appends it to its page without touching the original", () => {
  const before = { "https://a.example/1": [highlight(1, "https://a.example/1")] };
  const after = addHighlight(before, highlight(2, "https://a.example/1"));
  assert.equal(after["https://a.example/1"].length, 2);
  assert.equal(before["https://a.example/1"].length, 1);
});

test("removing a highlight finds it under any page and drops pages left empty", () => {
  const library = {
    "https://a.example/1": [highlight(1, "https://a.example/1")],
    "https://a.example/2": [highlight(2, "https://a.example/2"), highlight(3, "https://a.example/2")],
  };
  const after = removeHighlight(library, "1");
  assert.deepEqual(Object.keys(after), ["https://a.example/2"]);
  assert.equal(countHighlights(removeHighlight(after, 2)), 1);
});

test("a first note creates one comment, and editing it keeps its id and created time", () => {
  const url = "https://a.example/";
  let library = { [url]: [highlight(1, url)] };
  library = setNote(library, "1", "  a thought  ", clock);
  assert.deepEqual(library[url][0].comments, [
    { id: "c-new", text: "a thought", created: clock.now, updated: clock.now },
  ]);

  const later = { now: "2025-06-02T10:00:00.000Z", newId: () => "unused" };
  library = setNote(library, "1", "a better thought", later);
  assert.deepEqual(library[url][0].comments, [
    { id: "c-new", text: "a better thought", created: clock.now, updated: later.now },
  ]);
});

test("an unchanged note is not re-timestamped and a blank one removes the comment", () => {
  const url = "https://a.example/";
  const library = setNote({ [url]: [highlight(1, url)] }, "1", "same", clock);
  const unchanged = setNote(library, "1", "same", { now: "later", newId: () => "x" });
  assert.equal(unchanged[url][0], library[url][0]);
  assert.deepEqual(setNote(library, "1", "   ", clock)[url][0].comments, []);
  assert.equal(noteOf(unchanged[url][0]), "same");
});

test("merging adds missing highlights and missing comments but never overwrites", () => {
  const url = "https://a.example/";
  const existing = { [url]: [highlight(1, url, { repr: "mine", comments: [{ id: "c1", text: "kept" }] })] };
  const incoming = {
    [url]: [
      highlight(1, url, { repr: "theirs", comments: [{ id: "c1", text: "ignored" }, { id: "c2", text: "added" }] }),
      highlight(2, url),
    ],
  };
  const { library, added, commentsAdded } = mergeLibraries(existing, incoming);
  assert.equal(added, 1);
  assert.equal(commentsAdded, 1);
  assert.equal(library[url][0].repr, "mine");
  assert.deepEqual(library[url][0].comments.map((c) => c.text), ["kept", "added"]);
});

test("a page shows only the highlights saved under its own address", () => {
  const chatA = "https://claude.ai/chat/eb266478";
  const chatB = "https://claude.ai/chat/5a1807b4";
  const library = {
    [chatA]: [highlight(1, chatA)],
    [chatB]: [highlight(2, chatB)],
    "https://news.example/item?id=7": [highlight(3, "https://news.example/item?id=7")],
  };
  const ids = (pageUrl) => highlightsForPage(library, pageUrl, null).map((h) => h.groupID);

  assert.deepEqual(ids(`${chatA}#reply`), [1]);
  assert.deepEqual(ids(chatB), [2]);
  assert.deepEqual(ids("https://claude.ai/new"), []);
  assert.deepEqual(ids("https://news.example/item?id=8"), [], "a different query is a different page");
});

test("http and https, with or without www, are the same page", () => {
  const library = { "http://www.a.example/essay": [highlight(1, "http://www.a.example/essay")] };
  assert.deepEqual(highlightsForPage(library, "https://a.example/essay", null).map((h) => h.groupID), [1]);
});

test("a new highlight joins the page already saved under another form of its address", () => {
  const library = { "https://a.example/essay": [highlight(1, "https://a.example/essay")] };
  const after = addHighlight(library, highlight(2, "http://www.a.example/essay"));
  assert.deepEqual(Object.keys(after), ["https://a.example/essay"]);
  assert.deepEqual(after["https://a.example/essay"].map((h) => [h.groupID, h.url]), [
    [1, "https://a.example/essay"],
    [2, "https://a.example/essay"],
  ]);
});

test("one page saved under several addresses is merged under its https address, losing nothing", () => {
  const note = (id) => ({ id, text: id, created: clock.now, updated: clock.now });
  const library = {
    "http://www.a.example/essay": [highlight(1, "http://www.a.example/essay", { comments: [note("old")] })],
    "https://a.example/essay": [highlight(1, "https://a.example/essay", { comments: [note("new")] }), highlight(2, "https://a.example/essay")],
    "https://b.example/": [highlight(3, "https://b.example/")],
  };
  const merged = mergeSamePages(library);

  assert.deepEqual(Object.keys(merged).sort(), ["https://a.example/essay", "https://b.example/"]);
  const essay = merged["https://a.example/essay"];
  assert.deepEqual(essay.map((h) => h.groupID), [1, 2]);
  assert.deepEqual(essay[0].comments.map((c) => c.id), ["new", "old"]);
  assert.ok(essay.every((h) => h.url === "https://a.example/essay"));
  assert.deepEqual(mergeSamePages(merged), merged, "running it again changes nothing");
});

test("a page that declares light:url gets exactly that address's highlights", () => {
  const quine = "http://liotek.test/reader/quine-two-dogmas/book/book.html";
  const croce = "http://liotek.test/reader/croce-logic/book/book.html";
  const library = {
    [quine]: [highlight(1, quine), highlight(2, quine)],
    [croce]: [highlight(3, croce)],
  };
  const opened = (pageUrl) => highlightsForPage(library, pageUrl, quine).map((h) => h.groupID);

  assert.deepEqual(opened("http://127.0.0.1:5678/reader/quine-two-dogmas/book/book.html#page-42"), [1, 2]);
  assert.deepEqual(opened("file:///Users/me/iconoclast/books/quine-two-dogmas/book/book.html"), [1, 2]);
  assert.deepEqual(highlightsForPage(library, croce, `${croce}#page-9`).map((h) => h.groupID), [3]);
  assert.deepEqual(highlightsForPage(library, croce, "http://liotek.test/reader/unread/book/book.html"), []);
});

test("the popup lists sites and pages newest first", () => {
  const at = (date) => ({ date });
  const library = {
    "https://old.example/": [{ ...highlight(1, "https://old.example/"), ...at("2024-01-01T00:00:00Z") }],
    "https://new.example/a": [{ ...highlight(2, "https://new.example/a"), ...at("2025-01-01T00:00:00Z") }],
    "https://new.example/b": [{ ...highlight(3, "https://new.example/b"), ...at("2025-03-01T00:00:00Z") }],
  };
  const sites = groupBySite(library);
  assert.deepEqual(sites.map((s) => s.site), ["new.example", "old.example"]);
  assert.deepEqual(sites[0].pages.map((p) => p.url), ["https://new.example/b", "https://new.example/a"]);
});
