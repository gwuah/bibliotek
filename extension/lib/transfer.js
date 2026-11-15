import { normalizeInstallationId } from "./installation.js";
import { countHighlights, mergeLibraries, mergeSamePages } from "./library.js";

export const EXPORT_FORMAT = "liotek-light-export";
export const EXPORT_VERSION = 1;

export class ImportError extends Error {}

export function buildExport({ installationId, highlights, exportedAt }) {
  return {
    format: EXPORT_FORMAT,
    version: EXPORT_VERSION,
    installation: installationId,
    exported_at: exportedAt,
    highlights,
  };
}

export function exportFileName(installationId, exportedAt) {
  return `light-for-liotek-${installationId}-${exportedAt.slice(0, 10)}.json`;
}

export function parseExport(text) {
  let file;
  try {
    file = JSON.parse(text);
  } catch {
    throw new ImportError("This file is not JSON.");
  }
  if (!isObject(file) || file.format !== EXPORT_FORMAT) {
    throw new ImportError(
      `This is not a Light export: expected "format": "${EXPORT_FORMAT}". ` +
        "A copy of the old extension's clipboard export has no installation ID; " +
        "use the snippet in the extension's README to export it instead."
    );
  }
  if (file.version !== EXPORT_VERSION) {
    throw new ImportError(`This export is version ${file.version}; only version ${EXPORT_VERSION} is understood.`);
  }
  const installationId = normalizeInstallationId(file.installation);
  if (!installationId) {
    throw new ImportError("The export's installation ID is missing or malformed.");
  }
  validateHighlights(file.highlights);
  return { installationId, highlights: file.highlights };
}

export function validateHighlights(highlights) {
  if (!isObject(highlights)) throw new ImportError('"highlights" must be an object of pages.');

  for (const [url, page] of Object.entries(highlights)) {
    const where = `page ${url}`;
    if (!url.trim()) throw new ImportError("A page has a blank URL.");
    if (!Array.isArray(page)) throw new ImportError(`${where}: expected a list of highlights.`);

    const groupIds = new Set();
    for (const highlight of page) {
      validateHighlight(highlight, where);
      if (groupIds.has(highlight.groupID)) {
        throw new ImportError(`${where}: highlight ${highlight.groupID} appears twice.`);
      }
      groupIds.add(highlight.groupID);
    }
  }
}

function validateHighlight(highlight, where) {
  if (!isObject(highlight)) throw new ImportError(`${where}: a highlight is not an object.`);
  if (!Number.isSafeInteger(highlight.groupID)) {
    throw new ImportError(`${where}: a highlight's groupID is not an integer.`);
  }
  const here = `${where}, highlight ${highlight.groupID}`;
  for (const field of ["repr", "url", "date"]) {
    if (typeof highlight[field] !== "string") throw new ImportError(`${here}: "${field}" must be text.`);
  }
  if (!Array.isArray(highlight.chunks) || !highlight.chunks.every((c) => typeof c === "string")) {
    throw new ImportError(`${here}: "chunks" must be a list of text.`);
  }
  if (highlight.comments === undefined) return;
  if (!Array.isArray(highlight.comments)) throw new ImportError(`${here}: "comments" must be a list.`);

  const commentIds = new Set();
  for (const comment of highlight.comments) {
    if (!isObject(comment) || typeof comment.id !== "string" || !comment.id.trim()) {
      throw new ImportError(`${here}: a comment has no id.`);
    }
    if (typeof comment.text !== "string" || !comment.text.trim()) {
      throw new ImportError(`${here}: comment ${comment.id} has no text.`);
    }
    for (const field of ["created", "updated"]) {
      const value = comment[field];
      if (value !== undefined && value !== null && typeof value !== "string") {
        throw new ImportError(`${here}: comment ${comment.id} has a malformed "${field}".`);
      }
    }
    if (commentIds.has(comment.id)) throw new ImportError(`${here}: comment ${comment.id} appears twice.`);
    commentIds.add(comment.id);
  }
}

// A library already holding another installation's highlights is refused: one
// installation ID cannot speak for two sets of highlights without liotek holding
// both twice.
export function planImport(current, incoming) {
  if (countHighlights(current.highlights) === 0) {
    const highlights = mergeSamePages(incoming.highlights);
    return {
      installationId: incoming.installationId,
      highlights,
      added: countHighlights(highlights),
      commentsAdded: 0,
      adopted: incoming.installationId !== current.installationId,
    };
  }
  if (incoming.installationId === current.installationId) {
    const { library, added, commentsAdded } = mergeLibraries(current.highlights, incoming.highlights);
    return { installationId: current.installationId, highlights: library, added, commentsAdded, adopted: false };
  }
  throw new ImportError(
    `This extension already holds highlights for installation ${current.installationId}, ` +
      `and the file belongs to ${incoming.installationId}. Import into a fresh copy of the ` +
      "extension (for example in another browser profile) so each installation keeps its own ID."
  );
}

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
