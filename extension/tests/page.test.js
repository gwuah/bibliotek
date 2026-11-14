import assert from "node:assert/strict";
import { test } from "node:test";

import { highlightUrl, withoutFragment } from "../lib/page.js";

test("a place in the page is dropped from its address", () => {
  assert.equal(withoutFragment("https://a.example/essay#section-2"), "https://a.example/essay");
  assert.equal(withoutFragment("https://a.example/essay?id=3#"), "https://a.example/essay?id=3");
  assert.equal(withoutFragment("https://a.example/essay"), "https://a.example/essay");
});

test("a hash route names a different view, so it is kept", () => {
  assert.equal(withoutFragment("https://mail.example/#/inbox/42"), "https://mail.example/#/inbox/42");
  assert.equal(withoutFragment("https://old.example/#!/thread/7"), "https://old.example/#!/thread/7");
});

test("a highlight is saved under the address the page declares, else its own", () => {
  const declared = "http://liotek.test/reader/croce-logic/book/book.html";
  assert.equal(highlightUrl("file:///books/croce-logic/book/book.html#page-3", declared), declared);
  assert.equal(highlightUrl("https://a.example/essay#notes", null), "https://a.example/essay");
});
