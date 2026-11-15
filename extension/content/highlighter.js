import { anchorAt, compactIndexOf, compactText, compactView, locate } from "../lib/anchor.js";

export const HIGHLIGHT_CLASS = "text-highlighter-highlight";
const COLOR = "rgb(254, 254, 197)";
const HIDDEN_CLASS = "light-off";
const UNPAINTABLE = new Set(["script", "style", "noscript", "textarea", "template"]);
// A span is not allowed directly inside these, so whitespace between their children stays unwrapped.
const NO_INLINE_CHILDREN = new Set(["table", "thead", "tbody", "tfoot", "tr", "colgroup", "ul", "ol", "dl", "select"]);

export function spansOf(groupId) {
  return Array.from(
    document.querySelectorAll(`.${HIGHLIGHT_CLASS}[data-group-id="${CSS.escape(String(groupId))}"]`)
  );
}

export function paintedGroupIds() {
  return new Set(Array.from(document.querySelectorAll(`.${HIGHLIGHT_CLASS}`), (s) => s.dataset.groupId));
}

export function highlightAt(node) {
  return node && node.closest ? node.closest(`.${HIGHLIGHT_CLASS}`) : null;
}

export function captureSelection() {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;

  const stream = textStream(document.body);
  const bounds = stream.boundsOf(selection.getRangeAt(0));
  if (!bounds) return null;

  const repr = stream.text.slice(bounds.start, bounds.end);
  if (repr.trim() === "") return null;

  const chunks = stream
    .slicesBetween(bounds.start, bounds.end)
    .map(({ node, from, to }) => node.nodeValue.slice(from, to))
    .filter((text) => text.trim() !== "");
  const start = compactIndexOf(stream.origin, bounds.start);
  const end = compactIndexOf(stream.origin, bounds.end);
  return { repr, chunks, anchor: anchorAt(stream.compact, start, end) };
}

export function paintHighlight(highlight) {
  if (spansOf(highlight.groupID).length) {
    if (paintedTextMatches(highlight)) return true;
    unpaintHighlight(highlight.groupID);
  }
  const quote = compactText((highlight.chunks || []).join(""));
  if (!quote) return false;

  const stream = textStream(document.body);
  const at = locate(stream.compact, quote, highlight.anchor);
  if (at === -1) return false;

  const start = stream.origin[at];
  const end = stream.origin[at + quote.length - 1] + 1;
  for (const slice of stream.slicesBetween(start, end)) wrapSlice(slice, highlight.groupID);
  return spansOf(highlight.groupID).length > 0;
}

// A page that reuses its elements for new content can leave a highlight wrapped around
// text that is no longer the text it was made on.
function paintedTextMatches(highlight) {
  const painted = spansOf(highlight.groupID).map((span) => span.textContent).join("");
  return compactText(painted) === compactText((highlight.chunks || []).join(""));
}

function textStream(root) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (isPaintable(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  const nodes = [];
  while (walker.nextNode()) nodes.push(walker.currentNode);

  const starts = [0];
  for (const node of nodes) starts.push(starts[starts.length - 1] + node.nodeValue.length);
  const text = nodes.map((node) => node.nodeValue).join("");
  const { compact, origin } = compactView(text);

  function nodeIndexAt(index) {
    let lo = 0;
    let hi = nodes.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= index) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }

  return {
    text,
    compact,
    origin,

    boundsOf(range) {
      let start = null;
      let end = null;
      nodes.forEach((node, i) => {
        if (!range.intersectsNode(node)) return;
        const from = node === range.startContainer ? range.startOffset : 0;
        const to = node === range.endContainer ? range.endOffset : node.nodeValue.length;
        if (from >= to) return;
        start ??= starts[i] + from;
        end = starts[i] + to;
      });
      return start === null ? null : { start, end };
    },

    slicesBetween(start, end) {
      if (!nodes.length || start >= end) return [];
      const slices = [];
      for (let i = nodeIndexAt(start); i < nodes.length && starts[i] < end; i++) {
        const from = Math.max(0, start - starts[i]);
        const to = Math.min(nodes[i].nodeValue.length, end - starts[i]);
        if (from < to) slices.push({ node: nodes[i], from, to });
      }
      return slices;
    },
  };
}

function isPaintable(node) {
  if (!node.nodeValue || !node.parentNode) return false;
  return !UNPAINTABLE.has(node.parentNode.nodeName.toLowerCase());
}

function wrapSlice({ node, from, to }, groupId) {
  const text = node.nodeValue;
  const part = text.slice(from, to);
  if (part.trim() === "" && NO_INLINE_CHILDREN.has(node.parentNode.nodeName.toLowerCase())) return;

  const fragment = document.createDocumentFragment();
  if (from > 0) fragment.appendChild(document.createTextNode(text.slice(0, from)));
  fragment.appendChild(highlightSpan(part, groupId));
  if (to < text.length) fragment.appendChild(document.createTextNode(text.slice(to)));
  node.parentNode.replaceChild(fragment, node);
}

function highlightSpan(text, groupId) {
  const span = document.createElement("span");
  span.className = HIGHLIGHT_CLASS;
  span.dataset.groupId = String(groupId);
  span.textContent = text;
  span.style.color = "black";
  if (highlightsVisible()) showSpan(span);
  return span;
}

export function unpaintHighlight(groupId) {
  for (const span of spansOf(groupId)) {
    const parent = span.parentNode;
    span.replaceWith(...span.childNodes);
    parent.normalize();
  }
}

export function highlightsVisible() {
  return !document.documentElement.classList.contains(HIDDEN_CLASS);
}

export function setHighlightsVisible(visible) {
  document.documentElement.classList.toggle(HIDDEN_CLASS, !visible);
  for (const span of document.querySelectorAll(`.${HIGHLIGHT_CLASS}`)) {
    if (visible) showSpan(span);
    else hideSpan(span);
  }
}

function showSpan(span) {
  span.style.backgroundColor = COLOR;
  span.style.position = "relative";
}

function hideSpan(span) {
  span.style.backgroundColor = "";
  span.style.position = "";
}

export function scrollToHighlight(groupId) {
  spansOf(groupId)[0]?.scrollIntoView({ behavior: "smooth", block: "center" });
}
