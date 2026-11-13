export function metadataOf(document = {}) {
  return {
    title: document.title ?? "",
    description: document.description ?? "",
    authors: document.authors ?? [],
    subjects: document.subjects ?? [],
    tags: document.tags ?? [],
  };
}

const LIST_FIELDS = ["authors", "subjects", "tags"];

const sameList = (a, b) => a.length === b.length && a.every((item, i) => item === b[i]);

export function metadataChanges(before, after) {
  const changes = {};
  if (after.title.trim() !== before.title.trim()) changes.title = after.title.trim();
  if (after.description.trim() !== before.description.trim()) {
    changes.description = after.description.trim() || null;
  }
  for (const field of LIST_FIELDS) {
    if (!sameList(before[field], after[field])) changes[field] = after[field];
  }
  return changes;
}

export function uploadMetadata(metadata) {
  const result = {};
  if (metadata.title.trim()) result.title = metadata.title.trim();
  if (metadata.description.trim()) result.description = metadata.description.trim();
  for (const field of LIST_FIELDS) result[field] = metadata[field];
  return result;
}

// Bibliotek's server-side rules for a PDF's Title/Author/Subject/Keywords, run here
// before the upload is created. Shelves are tried in this order.
const SHELVES = [
  [["mathematics", "math"], "mathematics"],
  [["physics"], "physics"],
  [["biology"], "biology"],
  [["chemistry"], "chemistry"],
  [["computer", "programming"], "computers"],
  [["philosophy"], "philosophy"],
  [["literature", "fiction"], "literature"],
  [["history"], "history"],
  [["economics"], "economics"],
  [["psychology"], "psychology"],
];

export function inferSubject(subject, keywords) {
  const text = `${subject} ${keywords}`.toLowerCase();
  const shelf = SHELVES.find(([words]) => words.some((word) => text.includes(word)));
  return shelf ? shelf[1] : null;
}

export function titleFromFilename(filename) {
  const dot = filename.lastIndexOf(".");
  const stem = dot === -1 ? filename : filename.slice(0, dot);
  return stem.replace(/[_-]/g, " ").trim();
}

const commaList = (text) =>
  text
    .split(",")
    .map((piece) => piece.trim())
    .filter(Boolean);

export function metadataFromPdf(info = {}, filename = "") {
  const text = (value) => (typeof value === "string" ? value : "");
  const subject = text(info.Subject);
  const keywords = text(info.Keywords);
  const shelf = inferSubject(subject, keywords);
  return {
    title: text(info.Title).trim() ? text(info.Title) : titleFromFilename(filename),
    description: subject,
    authors: commaList(text(info.Author)),
    subjects: shelf ? [shelf] : [],
    tags: commaList(keywords).map((tag) => tag.toLowerCase()),
  };
}
