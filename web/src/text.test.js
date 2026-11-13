import { describe, expect, it } from "vitest";
import { formatBytes, formatDate, paragraphsOf, stripMarkup } from "./text.js";

describe("stripMarkup", () => {
  it("drops tags and keeps their text", () => {
    expect(stripMarkup("<p>A <strong>bold</strong> claim</p>")).toBe("A bold claim");
  });

  it("turns line-breaking tags into newlines", () => {
    expect(stripMarkup("first<br>second<br/>third<p>fourth</p>fifth")).toBe("first\nsecond\nthird\nfourth\nfifth");
  });

  it("never lets markup through, however it is written", () => {
    const hostile = `<img src=x onerror="alert(1)"><script>alert(2)</script><a href="javascript:x">link</a>`;
    expect(stripMarkup(hostile)).toBe("alert(2)link");
  });

  it("decodes entities into plain characters", () => {
    expect(stripMarkup("Fish &amp; chips &lt;3 &#8212; &#x263A; &nbsp;")).toBe("Fish & chips <3 — ☺");
    expect(stripMarkup("wait&hellip; &ldquo;so&rdquo; &mdash; yes")).toBe("wait… “so” — yes");
  });

  it("leaves text that only looks like markup alone", () => {
    expect(stripMarkup("x < y and y > z")).toBe("x < y and y > z");
    expect(stripMarkup("&unknown; stays")).toBe("&unknown; stays");
  });

  it("removes comments and collapses runs of blank lines", () => {
    expect(stripMarkup("a<!-- hidden --><p></p><p></p><p></p>b")).toBe("a\n\nb");
  });

  it("returns an empty string for nothing", () => {
    expect(stripMarkup(null)).toBe("");
  });
});

describe("formatBytes", () => {
  it("uses the largest whole unit with at most one decimal", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(8 * 1024 * 1024)).toBe("8 MB");
  });
});

describe("formatDate", () => {
  it("writes day, month and two-digit year", () => {
    expect(formatDate("2025-03-05T12:00:00")).toBe("05/03/25");
    expect(formatDate(null)).toBe("");
  });
});

describe("paragraphsOf", () => {
  it("splits stripped text into paragraphs", () => {
    expect(paragraphsOf("<p>One <em>two</em></p><p>Three</p>")).toEqual(["One two", "Three"]);
    expect(paragraphsOf("")).toEqual([]);
  });
});
