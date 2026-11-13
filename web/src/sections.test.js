import { describe, expect, it } from "vitest";
import {
  chaptersOf,
  groupByPage,
  highlightsInChapter,
  printedPageOf,
  unchapteredHighlights,
} from "./sections.js";

const at = (id, pdfPage) => ({
  id,
  text: `highlight ${id}`,
  location: pdfPage === null ? null : { version: 1, kind: "pdf_region", payload: { page_number: pdfPage } },
});

const ids = (highlights) => highlights.map((h) => h.id);

describe("printedPageOf", () => {
  it("subtracts the page offset from the PDF page", () => {
    expect(printedPageOf(at(1, 14), 4)).toBe(10);
  });

  it("is null for highlights without a page, such as web highlights", () => {
    expect(printedPageOf(at(1, null), 4)).toBeNull();
    expect(printedPageOf({ location: { kind: "web_text", payload: { url: "x" } } })).toBeNull();
  });
});

describe("chapters", () => {
  const chapters = chaptersOf([
    { title: "One", start_page: 1, end_page: 10 },
    { title: "Two", start_page: 11, end_page: 20 },
  ]);

  it("keys sections 1, 2, … in reading order", () => {
    expect(chapters).toEqual([
      { key: 1, title: "One", startPage: 1, endPage: 10 },
      { key: 2, title: "Two", startPage: 11, endPage: 20 },
    ]);
  });

  it("files highlights under a chapter by printed page, applying the offset", () => {
    const highlights = [at("a", 15), at("b", 5), at("c", 14)];
    expect(ids(highlightsInChapter(highlights, chapters[0], 4))).toEqual(["b", "c"]);
    expect(ids(highlightsInChapter(highlights, chapters[1], 4))).toEqual(["a"]);
  });

  it("counts highlights outside every chapter, and those with no page, as unchaptered", () => {
    const highlights = [at("out", 25), at("none", null), at("in", 2)];
    expect(ids(unchapteredHighlights(highlights, chapters))).toEqual(["out", "none"]);
  });
});

describe("groupByPage", () => {
  it("orders pages ascending with the page-less highlights last", () => {
    const groups = groupByPage([at("x", null), at("y", 9), at("z", 3), at("w", 9)]);
    expect(groups.map((g) => [g.page, ids(g.highlights)])).toEqual([
      [3, ["z"]],
      [9, ["y", "w"]],
      [null, ["x"]],
    ]);
  });
});
