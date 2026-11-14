import { describe, expect, it } from "vitest";
import { shelfOf } from "./shelf.js";

const book = (slug, title, author, translator = "") => ({ slug, title, author, translator });

const books = [
  book("bergson", "Time and Free Will", "Henri Bergson", "F. L. Pogson"),
  book("croce-aesthetic", "Aesthetic", "Benedetto Croce", "Douglas Ainslie"),
  book("croce-logic", "Logic as the Science of the Pure Concept", "Benedetto Croce", "Douglas Ainslie"),
  book("brouwer", "Life, Art, and Mysticism", "L. E. J. Brouwer"),
];

describe("shelfOf", () => {
  it("groups books by author in the order they arrive", () => {
    expect(shelfOf(books).map((shelf) => [shelf.author, shelf.books.map((b) => b.slug)])).toEqual([
      ["Henri Bergson", ["bergson"]],
      ["Benedetto Croce", ["croce-aesthetic", "croce-logic"]],
      ["L. E. J. Brouwer", ["brouwer"]],
    ]);
  });

  it("keeps only books whose title, author or translator has every word searched for", () => {
    expect(shelfOf(books, "croce logic").flatMap((s) => s.books.map((b) => b.slug))).toEqual(["croce-logic"]);
    expect(shelfOf(books, "ainslie").flatMap((s) => s.books.map((b) => b.slug))).toEqual([
      "croce-aesthetic",
      "croce-logic",
    ]);
  });

  it("ignores case and accents when searching", () => {
    const withAccent = [book("velazquez", "Las Meninas", "Diego Velázquez")];
    expect(shelfOf(withAccent, "VELAZQUEZ")).toHaveLength(1);
  });

  it("puts books with no author on their own shelf", () => {
    expect(shelfOf([book("anon", "Untitled", "")])[0].author).toBe("Unknown author");
  });
});
