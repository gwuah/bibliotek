// Text is matched with all whitespace removed: the whitespace between paragraphs depends
// on the page's markup, and highlights saved before anchors existed have none of it.

const CONTEXT = 32;

export function compactView(text) {
  let compact = "";
  const origin = [];
  for (let i = 0; i < text.length; i++) {
    if (/\s/.test(text[i])) continue;
    compact += text[i];
    origin.push(i);
  }
  return { compact, origin };
}

export function compactText(text) {
  return text.replace(/\s+/g, "");
}

export function compactIndexOf(origin, index) {
  let lo = 0;
  let hi = origin.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (origin[mid] < index) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export function anchorAt(compact, start, end) {
  const quote = compact.slice(start, end);
  return {
    prefix: compact.slice(Math.max(0, start - CONTEXT), start),
    suffix: compact.slice(end, end + CONTEXT),
    occurrence: occurrencesOf(compact, quote).indexOf(start),
  };
}

export function locate(compact, quote, anchor) {
  if (!quote) return -1;
  const matches = occurrencesOf(compact, quote);
  if (!matches.length) return -1;
  if (!anchor) return matches[0];

  let best = matches[0];
  let bestRank = null;
  matches.forEach((start, ordinal) => {
    const rank = {
      context:
        similarityAtEnd(compact.slice(Math.max(0, start - CONTEXT), start), anchor.prefix || "") +
        similarityAtEnd(
          reversed(compact.slice(start + quote.length, start + quote.length + CONTEXT)),
          reversed(anchor.suffix || "")
        ),
      distance: Math.abs(ordinal - (anchor.occurrence ?? 0)),
    };
    if (!bestRank || outranks(rank, bestRank)) {
      best = start;
      bestRank = rank;
    }
  });
  return best;
}

function outranks(a, b) {
  return a.context > b.context || (a.context === b.context && a.distance < b.distance);
}

function occurrencesOf(text, quote) {
  const found = [];
  for (let at = text.indexOf(quote); at !== -1; at = text.indexOf(quote, at + 1)) found.push(at);
  return found;
}

// Alignment score of the two strings' ends, where each matching character earns a point
// and each changed, added or removed one costs a point. Where they start does not matter,
// so text shifted in from beyond the context window costs nothing extra.
function similarityAtEnd(a, b) {
  let previous = new Array(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i++) {
    const row = [0];
    for (let j = 1; j <= b.length; j++) {
      const diagonal = previous[j - 1] + (a[i - 1] === b[j - 1] ? 1 : -1);
      row[j] = Math.max(0, diagonal, previous[j] - 1, row[j - 1] - 1);
    }
    previous = row;
  }
  return previous[b.length];
}

function reversed(text) {
  return Array.from(text).reverse().join("");
}
