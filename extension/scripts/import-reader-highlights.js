// Moves highlights made with the iconoclast reader's own highlighter (kept in the browser's
// localStorage) into a Light export file, under the address each book declares (light:url).
// Run with --help for usage. Reads its inputs and writes one file; nothing else is touched.

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { anchorAt, compactText } from "../lib/anchor.js";
import { mergeLibraries } from "../lib/library.js";
import { buildExport, parseExport, validateHighlights } from "../lib/transfer.js";

const CURRENT_PREFIX = "notes:v2:";
const FIRST_VERSION_PREFIX = "highlights:";
const BLOCKS = new Set(["p", "h2", "h3", "h4", "blockquote", "li"]);
const UNPAINTABLE = new Set(["script", "style", "noscript", "textarea", "template"]);
const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);
const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

export function readerPageUrl(base, slug) {
  return `${base.replace(/\/+$/, "")}/reader/${slug}/book/book.html`;
}

export function slugOf(storedPath) {
  const match = decodeURIComponent(storedPath).match(/([^/]+)\/book\/book\.html$/);
  return match ? match[1] : null;
}

// The book as Light and the reader see it: the body's text with all whitespace removed (what
// Light anchors against), and where each of the reader's paragraphs starts in that text. The
// reader counts paragraphs as innermost p, h2-h4, blockquote and li inside <main>.
export function parseBook(html) {
  let compact = "";
  const open = [];
  const blocks = [];
  let inBody = false;

  const addText = (text) => {
    if (!inBody || open.some((element) => UNPAINTABLE.has(element.name))) return;
    compact += compactText(decodeEntities(text));
  };

  for (const token of tokenize(html)) {
    if (token.kind === "text") {
      addText(token.text);
      continue;
    }
    if (token.kind !== "open" && token.kind !== "close") continue;
    if (token.kind === "open") {
      if (token.name === "body") inBody = true;
      if (VOID.has(token.name) || token.selfClosing) continue;
      const inMain = open.some((element) => element.name === "main");
      const element = { name: token.name, block: null };
      if (inMain && BLOCKS.has(token.name)) {
        for (const outer of open) if (outer.block) outer.block.nested = true;
        element.block = { start: compact.length, nested: false };
        blocks.push(element.block);
      }
      open.push(element);
    } else {
      const at = open.findLastIndex((element) => element.name === token.name);
      if (at !== -1) open.length = at;
      if (token.name === "body") inBody = false;
    }
  }
  return { compact, blockStarts: blocks.filter((block) => !block.nested).map((block) => block.start) };
}

function* tokenize(html) {
  let at = 0;
  while (at < html.length) {
    if (html.startsWith("<!--", at)) {
      const end = html.indexOf("-->", at + 4);
      at = end === -1 ? html.length : end + 3;
      continue;
    }
    if (html[at] === "<" && /[a-zA-Z/!]/.test(html[at + 1] ?? "")) {
      const end = tagEnd(html, at);
      const tag = html.slice(at + 1, end);
      at = end + 1;
      if (tag.startsWith("!")) continue;
      const closing = tag.startsWith("/");
      const name = (closing ? tag.slice(1) : tag).match(/^[a-zA-Z0-9-]+/)?.[0].toLowerCase();
      if (!name) continue;
      if (closing) {
        yield { kind: "close", name };
        continue;
      }
      yield { kind: "open", name, selfClosing: tag.endsWith("/") };
      if (name === "script" || name === "style") {
        const close = html.toLowerCase().indexOf(`</${name}`, at);
        const stop = close === -1 ? html.length : close;
        yield { kind: "text", text: html.slice(at, stop) };
        at = stop;
      }
      continue;
    }
    const next = html.indexOf("<", at + 1);
    const stop = next === -1 ? html.length : next;
    yield { kind: "text", text: html.slice(at, stop) };
    at = stop;
  }
}

function tagEnd(html, start) {
  let quote = null;
  for (let at = start + 1; at < html.length; at++) {
    const c = html[at];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === ">") {
      return at;
    }
  }
  return html.length;
}

function decodeEntities(text) {
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (whole, name) => {
    if (name[0] === "#") {
      const code = name[1].toLowerCase() === "x" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    return ENTITIES[name.toLowerCase()] ?? whole;
  });
}

// Where a reader highlight sits in the book's compact text: at its stored paragraph positions
// when the text there is still its quote, else at the occurrence of the quote nearest to them
// (a rebuilt book), else nowhere.
export function placeHighlight(book, item) {
  const quote = compactText(item.quote || "");
  if (!quote) return null;
  const expected = positionOf(book, item.a);
  const end = positionOf(book, item.z);
  if (expected !== null && end !== null && book.compact.slice(expected, end) === quote) {
    return { start: expected, end, exact: true };
  }
  const found = occurrences(book.compact, quote);
  if (!found.length) return null;
  const near = expected ?? 0;
  const start = found.reduce((best, at) => (Math.abs(at - near) < Math.abs(best - near) ? at : best));
  return { start, end: start + quote.length, exact: false };
}

function positionOf(book, point) {
  if (!point || !Number.isInteger(point.b) || !Number.isInteger(point.o)) return null;
  const start = book.blockStarts[point.b];
  return start === undefined ? null : start + point.o;
}

function occurrences(text, quote) {
  const found = [];
  for (let at = text.indexOf(quote); at !== -1; at = text.indexOf(quote, at + 1)) found.push(at);
  return found;
}

export function readerItems(dumps) {
  const items = [];
  for (const dump of dumps) {
    for (const [key, stored] of Object.entries(dump)) {
      const prefix = [CURRENT_PREFIX, FIRST_VERSION_PREFIX].find((p) => key.startsWith(p));
      if (!prefix || !Array.isArray(stored)) continue;
      const slug = slugOf(key.slice(prefix.length));
      for (const item of stored) items.push({ slug, item });
    }
  }
  return items.sort(
    (x, y) =>
      String(x.item.created).localeCompare(String(y.item.created)) ||
      String(x.item.id).localeCompare(String(y.item.id)),
  );
}

// groupIDs are the creation time in ms, so a second run gives every highlight the same id and
// Light's merge skips it. A clash moves the later highlight on by one ms.
function groupIdFor(item, taken) {
  let id = Date.parse(item.created);
  if (!Number.isSafeInteger(id)) return null;
  while (taken.has(id)) id++;
  taken.add(id);
  return id;
}

function lightHighlight(item, url, groupID, compact, place) {
  const highlight = {
    groupID,
    repr: item.quote,
    chunks: [item.quote],
    anchor: anchorAt(compact, place.start, place.end),
    url,
    date: item.created,
  };
  const note = (item.note || "").trim();
  if (note) {
    highlight.comments = [{ id: `reader-${item.id}`, text: note, created: item.created, updated: item.created }];
  }
  return highlight;
}

function idsOutside(library, urls) {
  const ids = new Set();
  for (const [url, highlights] of Object.entries(library)) {
    if (urls.has(url)) continue;
    for (const highlight of highlights) ids.add(Number(highlight.groupID));
  }
  return ids;
}

export async function convert({ dumps, light, base, loadBook, exportedAt }) {
  const items = readerItems(dumps);
  const urls = new Set(items.filter((i) => i.slug).map((i) => readerPageUrl(base, i.slug)));
  const taken = idsOutside(light.highlights, urls);
  const books = new Map();
  const incoming = {};
  const report = { books: {}, skipped: [] };
  const skip = (slug, item, reason) => report.skipped.push({ slug, id: item.id, quote: item.quote, reason });

  for (const { slug, item } of items) {
    if (!slug) {
      skip(slug, item, "not a book page");
      continue;
    }
    if (!books.has(slug)) books.set(slug, await loadBook(slug).then(parseBook, () => null));
    const book = books.get(slug);
    if (!book) {
      skip(slug, item, "book not found");
      continue;
    }
    const place = placeHighlight(book, item);
    if (!place) {
      skip(slug, item, "quote not found in the book");
      continue;
    }
    const groupID = groupIdFor(item, taken);
    if (groupID === null) {
      skip(slug, item, "no creation time");
      continue;
    }
    const url = readerPageUrl(base, slug);
    (incoming[url] ??= []).push(lightHighlight(item, url, groupID, book.compact, place));
    const counts = (report.books[slug] ??= { placed: 0, byQuote: 0 });
    counts[place.exact ? "placed" : "byQuote"]++;
  }

  const merged = mergeLibraries(light.highlights, incoming);
  validateHighlights(merged.library);
  report.added = merged.added;
  report.commentsAdded = merged.commentsAdded;
  return {
    file: buildExport({ installationId: light.installationId, highlights: merged.library, exportedAt }),
    report,
  };
}

const USAGE = `Usage: node extension/scripts/import-reader-highlights.js \\
  --reader reader-highlights.json [--reader more.json] \\
  --light light-for-liotek-<id>-<date>.json \\
  --books ../iconoclast/books \\
  --base http://liotek.test \\
  --output light-with-reader.json

--base must equal LIGHT_BASE in the reader's boot.js. Import the output in Light's
Settings > Import from file, then Sync.`;

async function main() {
  const { values } = parseArgs({
    options: {
      reader: { type: "string", multiple: true },
      light: { type: "string" },
      books: { type: "string" },
      base: { type: "string" },
      output: { type: "string" },
      help: { type: "boolean" },
    },
  });
  if (values.help || !values.reader?.length || !values.light || !values.books || !values.base || !values.output) {
    console.log(USAGE);
    process.exitCode = values.help ? 0 : 2;
    return;
  }

  const dumps = await Promise.all(values.reader.map(async (file) => JSON.parse(await readFile(file, "utf8"))));
  const light = parseExport(await readFile(values.light, "utf8"));
  const loadBook = (slug) => readFile(path.join(values.books, slug, "book", "book.html"), "utf8");
  const { file, report } = await convert({
    dumps,
    light,
    base: values.base,
    loadBook,
    exportedAt: new Date().toISOString(),
  });
  await writeFile(values.output, JSON.stringify(file, null, 2));

  for (const [slug, counts] of Object.entries(report.books).sort()) {
    const moved = counts.byQuote ? ` (${counts.byQuote} found by their quote)` : "";
    console.log(`${slug}: ${counts.placed + counts.byQuote}${moved}`);
  }
  console.log(`\n${report.added} highlights added, ${report.commentsAdded} notes added to existing ones.`);
  if (report.skipped.length) {
    console.log(`\nLeft out ${report.skipped.length}:`);
    for (const s of report.skipped) console.log(`  ${s.slug ?? "?"} ${s.id}: ${s.reason}: "${String(s.quote).slice(0, 60)}"`);
  }
  console.log(`\nWrote ${values.output}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
