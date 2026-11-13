const ENTITIES = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
  mdash: "—",
  ndash: "–",
  hellip: "…",
  lsquo: "‘",
  rsquo: "’",
  ldquo: "“",
  rdquo: "”",
};

function decodeEntities(text) {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name) => {
    if (name[0] !== "#") return ENTITIES[name.toLowerCase()] ?? whole;
    const code = name[1] === "x" || name[1] === "X" ? parseInt(name.slice(2), 16) : Number(name.slice(1));
    return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
  });
}

// Imported notes may carry HTML; it is never interpreted, only reduced to plain text.
export function stripMarkup(text) {
  if (!text) return "";
  const withBreaks = text
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/?(p|div|li|h[1-6]|blockquote)\b[^>]*>/gi, "\n");
  const withoutTags = withBreaks.replace(/<\/?[a-z][^>]*>/gi, "").replace(/<!--[\s\S]*?-->/g, "");
  return decodeEntities(withoutTags)
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function formatBytes(bytes) {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${parseFloat((bytes / 1024 ** exponent).toFixed(1))} ${units[exponent]}`;
}

export function formatDate(timestamp) {
  if (!timestamp) return "";
  const date = new Date(timestamp);
  const day = String(date.getDate()).padStart(2, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  return `${day}/${month}/${String(date.getFullYear()).slice(-2)}`;
}

export const paragraphsOf = (text) => stripMarkup(text).split(/\n{2,}/).filter(Boolean);
