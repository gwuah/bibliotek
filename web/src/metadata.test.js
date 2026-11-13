import { describe, expect, it } from "vitest";
import { inferSubject, metadataChanges, metadataFromPdf, metadataOf, titleFromFilename, uploadMetadata } from "./metadata.js";

const book = metadataOf({
  title: "A Grain of Wheat",
  description: null,
  authors: ["Ngũgĩ wa Thiong'o"],
  subjects: ["Fiction"],
  tags: ["kenya"],
});

describe("metadataChanges", () => {
  it("is empty when nothing changed", () => {
    expect(metadataChanges(book, { ...book, title: " A Grain of Wheat " })).toEqual({});
  });

  it("contains only the fields that changed", () => {
    expect(metadataChanges(book, { ...book, tags: ["kenya", "independence"] })).toEqual({
      tags: ["kenya", "independence"],
    });
  });

  it("clears a description emptied by the person", () => {
    const described = { ...book, description: "On the Emergency" };
    expect(metadataChanges(described, { ...described, description: "  " })).toEqual({ description: null });
  });

  it("treats a reordering of authors as a change", () => {
    const two = { ...book, authors: ["A", "B"] };
    expect(metadataChanges(two, { ...two, authors: ["B", "A"] })).toEqual({ authors: ["B", "A"] });
  });
});

describe("uploadMetadata", () => {
  it("leaves out a blank title so the server can use the filename", () => {
    expect(uploadMetadata({ ...book, title: " " })).toEqual({
      authors: book.authors,
      subjects: book.subjects,
      tags: book.tags,
    });
  });
});

describe("metadataFromPdf", () => {
  it("reads Title, Author, Subject and Keywords as Bibliotek did", () => {
    expect(
      metadataFromPdf(
        { Title: "Petals of Blood", Author: "Ngũgĩ wa Thiong'o, Ann Other", Subject: "Postcolonial fiction", Keywords: "Kenya, Novel ,1977" },
        "petals.pdf",
      ),
    ).toEqual({
      title: "Petals of Blood",
      description: "Postcolonial fiction",
      authors: ["Ngũgĩ wa Thiong'o", "Ann Other"],
      subjects: ["literature"],
      tags: ["kenya", "novel", "1977"],
    });
  });

  it("titles a PDF without metadata after its filename", () => {
    expect(metadataFromPdf({}, "weep_not-child.pdf")).toEqual({ ...metadataOf({}), title: "weep not child" });
    expect(metadataFromPdf(undefined, "a.pdf").title).toBe("a");
  });
});

describe("titleFromFilename", () => {
  it("drops only the last extension and reads _ and - as spaces", () => {
    expect(titleFromFilename("The_Republic-Plato.v2.pdf")).toBe("The Republic Plato.v2");
    expect(titleFromFilename("README")).toBe("README");
  });
});

describe("inferSubject", () => {
  it("picks the first of Bibliotek's shelves named in the subject or keywords", () => {
    expect(inferSubject("Applied Mathematics", "")).toBe("mathematics");
    expect(inferSubject("", "lisp, programming")).toBe("computers");
    expect(inferSubject("History of physics", "")).toBe("physics");
    expect(inferSubject("Cooking", "food")).toBeNull();
  });
});
