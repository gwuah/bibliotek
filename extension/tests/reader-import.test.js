import assert from "node:assert/strict";
import { test } from "node:test";

import { compactText, locate } from "../lib/anchor.js";
import { convert, parseBook } from "../scripts/import-reader-highlights.js";

const BOOK = `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>Two Dogmas of Empiricism</title>
<script src="../../reader/boot.js"></script>
<script>const ignored = "two dogmas";</script>
</head><body><main>
<h1>Two Dogmas of Empiricism</h1>
<!-- two dogmas in a comment -->
<h2><span class="pb" id="page-20" data-page="20"></span>II<br>Background</h2>
<p class="paragraph">Modern empiricism has been conditioned in large part by two dogmas.</p>
<blockquote><p>A quoted &lt;line&gt; of text.</p></blockquote>
<p class="paragraph">The other dogma is <em>reductionism</em>, and two dogmas again.</p>
</main></body></html>`;

const BASE = "http://liotek.test";
const QUINE = "http://liotek.test/reader/quine-two-dogmas/book/book.html";
const WEB = "https://a.example/essay";
const STORED_PATH = "/Users/me/iconoclast/books/quine-two-dogmas/book/book.html";
const dense = (text) => text.replace(/\s+/g, "");

const webHighlight = { groupID: 5, repr: "words", chunks: ["words"], url: WEB, date: "2025-01-01T00:00:00.000Z" };
const light = () => ({ installationId: "abc123", highlights: { [WEB]: [webHighlight] } });

function readerItem(id, block, before, quote, extra = {}) {
  const o = dense(before).length;
  return {
    id,
    a: { b: block, o },
    z: { b: block, o: o + dense(quote).length },
    quote,
    note: "",
    created: "2026-10-20T10:00:00.000Z",
    ...extra,
  };
}

async function run(items, from = light(), books = { "quine-two-dogmas": BOOK }) {
  return convert({
    dumps: [{ [`notes:v2:${STORED_PATH}`]: items, "reader-settings": { theme: "night" } }],
    light: from,
    base: BASE,
    loadBook: async (slug) => {
      if (!(slug in books)) throw new Error("no such book");
      return books[slug];
    },
    exportedAt: "2026-11-14T00:00:00.000Z",
  });
}

test("the book's text and paragraphs are read as the reader counts them", () => {
  const book = parseBook(BOOK);

  assert.ok(book.compact.startsWith("TwoDogmasofEmpiricismIIBackgroundModernempiricism"));
  assert.ok(!book.compact.includes("ignored"));
  assert.ok(!book.compact.includes("inacomment"));
  assert.deepEqual(
    book.blockStarts.map((start) => book.compact.slice(start, start + 12)),
    ["IIBackground", "Modernempiri", "Aquoted<line", "Theotherdogm"],
  );
});

test("a reader highlight lands on the book's declared address with its note and creation time", async () => {
  const item = readerItem("r1", 3, "The other dogma is ", "reductionism", { note: " the second dogma " });

  const { file, report } = await run([item]);

  assert.equal(file.installation, "abc123");
  assert.deepEqual(file.highlights[WEB], [webHighlight]);
  const [saved] = file.highlights[QUINE];
  assert.equal(saved.url, QUINE);
  assert.equal(saved.groupID, Date.parse(item.created));
  assert.equal(saved.date, item.created);
  assert.equal(saved.repr, "reductionism");
  assert.deepEqual(saved.chunks, ["reductionism"]);
  assert.deepEqual(saved.comments, [
    { id: "reader-r1", text: "the second dogma", created: item.created, updated: item.created },
  ]);
  assert.deepEqual(report.books, { "quine-two-dogmas": { placed: 1, byQuote: 0 } });
});

test("the anchor picks out the right occurrence when the quote appears twice", async () => {
  const item = readerItem("r2", 3, "The other dogma is reductionism, and ", "two dogmas");
  const compact = parseBook(BOOK).compact;

  const { file } = await run([item]);

  const [saved] = file.highlights[QUINE];
  assert.equal(locate(compact, compactText(saved.repr), saved.anchor), compact.lastIndexOf("twodogmas"));
});

test("text set as entities is matched as the characters it stands for", async () => {
  const { file, report } = await run([readerItem("r3", 2, "A quoted ", "<line>")]);

  assert.equal(file.highlights[QUINE].length, 1);
  assert.deepEqual(report.books["quine-two-dogmas"], { placed: 1, byQuote: 0 });
});

test("a quote whose stored position no longer fits is found by its nearest occurrence", async () => {
  const item = readerItem("r4", 3, "The other dogma is reductionism, an", "two dogmas");
  const compact = parseBook(BOOK).compact;

  const { file, report } = await run([item]);

  const [saved] = file.highlights[QUINE];
  assert.equal(locate(compact, compactText(saved.repr), saved.anchor), compact.lastIndexOf("twodogmas"));
  assert.deepEqual(report.books["quine-two-dogmas"], { placed: 0, byQuote: 1 });
});

test("first-version highlights, which have only a quote, are found by it", async () => {
  const { file } = await convert({
    dumps: [{ [`highlights:${STORED_PATH}`]: [{ id: "old", quote: "Modern empiricism", created: "2026-10-01T09:00:00.000Z" }] }],
    light: light(),
    base: BASE,
    loadBook: async () => BOOK,
    exportedAt: "2026-11-14T00:00:00.000Z",
  });

  assert.equal(file.highlights[QUINE][0].repr, "Modern empiricism");
});

test("highlights that cannot be placed are left out and reported, never guessed", async () => {
  const missing = readerItem("r5", 3, "", "a sentence the book never had");
  const { file, report } = await convert({
    dumps: [{
      [`notes:v2:${STORED_PATH}`]: [missing],
      "notes:v2:/Users/me/iconoclast/books/unknown-book/book/book.html": [readerItem("r6", 0, "", "Background")],
    }],
    light: light(),
    base: BASE,
    loadBook: async (slug) => (slug === "quine-two-dogmas" ? BOOK : Promise.reject(new Error("missing"))),
    exportedAt: "2026-11-14T00:00:00.000Z",
  });

  assert.equal(file.highlights[QUINE], undefined);
  assert.deepEqual(
    report.skipped.map((s) => [s.id, s.reason]).sort(),
    [["r5", "quote not found in the book"], ["r6", "book not found"]],
  );
});

test("two highlights made in the same millisecond get different groupIDs", async () => {
  const { file } = await run([
    readerItem("r7", 1, "", "Modern"),
    readerItem("r8", 3, "The other dogma is ", "reductionism"),
  ]);

  const ids = file.highlights[QUINE].map((h) => h.groupID);
  assert.equal(new Set(ids).size, 2);
});

test("running the conversion on its own output adds nothing", async () => {
  const items = [
    readerItem("r7", 1, "", "Modern", { note: "first" }),
    readerItem("r8", 3, "The other dogma is ", "reductionism"),
  ];
  const first = await run(items);

  const second = await run(items, { installationId: "abc123", highlights: first.file.highlights });

  assert.equal(second.report.added, 0);
  assert.equal(second.report.commentsAdded, 0);
  assert.deepEqual(second.file.highlights, first.file.highlights);
});
