import assert from "node:assert/strict";
import { test } from "node:test";

import { anchorAt, compactIndexOf, compactText, compactView, locate } from "../lib/anchor.js";

const PAGE = [
  "The quick brown fox jumps over the lazy dog.",
  "Sphinx of black quartz, judge my vow. How vexingly quick daft zebras jump.",
  "Waltz, bad nymph, for quick jigs vex.",
].join("\n");

function anchorFor(text, quote, nth) {
  const { compact, origin } = compactView(text);
  let at = -1;
  for (let i = 0; i <= nth; i++) at = text.indexOf(quote, at + 1);
  const start = compactIndexOf(origin, at);
  const end = compactIndexOf(origin, at + quote.length);
  return { compact, start, anchor: anchorAt(compact, start, end) };
}

test("a repeated word is found where it was selected, not where it first appears", () => {
  const { compact, start, anchor } = anchorFor(PAGE, "quick", 2);
  assert.equal(locate(compact, "quick", anchor), start);
  assert.notEqual(locate(compact, "quick", anchor), compact.indexOf("quick"));
});

test("a highlight saved without an anchor is found at the first occurrence", () => {
  const { compact } = compactView(PAGE);
  assert.equal(locate(compact, "quick", undefined), compact.indexOf("quick"));
});

test("text spanning two paragraphs matches although its chunks carry no whitespace between them", () => {
  const { compact } = compactView(PAGE);
  const chunks = ["the lazy dog.", "Sphinx of"];
  assert.ok(locate(compact, compactText(chunks.join("")), undefined) > 0);
});

test("when the surrounding text cannot tell occurrences apart, the saved occurrence decides", () => {
  const text = "same same same same";
  const { compact, start, anchor } = anchorFor(text, "same", 2);
  assert.equal(locate(compact, "same", { ...anchor, prefix: "", suffix: "" }), start);
});

test("a highlight still matches after text near it changed", () => {
  const { anchor } = anchorFor(PAGE, "quick", 1);
  const edited = PAGE.replace("Sphinx of black quartz", "A sphinx of quartz");
  const { compact } = compactView(edited);
  assert.equal(locate(compact, "quick", anchor), compact.indexOf("quickdaft"));
});

test("a repeated word is still found after the characters right beside it changed", () => {
  const original = "alpha beta gamma quick delta epsilon zeta. omega psi chi quick phi upsilon tau.";
  const { anchor } = anchorFor(original, "quick", 1);
  const edited = "quick start. " + original.replace("chi quick phi", "chX quick Xhi");
  const { compact } = compactView(edited);
  assert.equal(locate(compact, "quick", anchor), compact.indexOf("chXquick") + 3);
});

test("text that is not on the page is not found", () => {
  const { compact } = compactView(PAGE);
  assert.equal(locate(compact, "absent", { prefix: "", suffix: "", occurrence: 0 }), -1);
});

test("positions map between the page text and its whitespace-free form", () => {
  const { compact, origin } = compactView("a b\n c");
  assert.equal(compact, "abc");
  assert.deepEqual(origin, [0, 2, 5]);
  assert.equal(compactIndexOf(origin, 3), 2);
});
