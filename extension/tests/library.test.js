import assert from "node:assert/strict";
import { test } from "node:test";

import {
  addHighlight,
  countHighlights,
  groupBySite,
  highlightsForPage,
  highlightsForSite,
  mergeLibraries,
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

test("a page shows every highlight stored for its site, whatever the path", () => {
  const library = {
    "https://a.example/one": [highlight(1, "https://a.example/one")],
    "https://a.example/two?x=1": [highlight(2, "https://a.example/two?x=1")],
    "https://b.example/one": [highlight(3, "https://b.example/one")],
    "not a url": [highlight(4, "not a url")],
  };
  const ids = highlightsForSite(library, "https://a.example/one#section").map((h) => h.groupID);
  assert.deepEqual(ids.sort(), [1, 2]);
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

test("a page without light:url still gets every highlight on its host", () => {
  const library = {
    "https://a.example/one": [highlight(1, "https://a.example/one")],
    "https://a.example/two": [highlight(2, "https://a.example/two")],
  };
  const ids = highlightsForPage(library, "https://a.example/one", null).map((h) => h.groupID);
  assert.deepEqual(ids.sort(), [1, 2]);
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
