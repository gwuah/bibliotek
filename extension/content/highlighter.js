export const HIGHLIGHT_CLASS = "text-highlighter-highlight";
const COLOR = "rgb(254, 254, 197)";
const HIDDEN_CLASS = "light-off";

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

  const contents = selection.getRangeAt(0).cloneContents();
  const repr = contents.textContent;
  if (repr.trim() === "") return null;

  const chunks = leavesOf(contents)
    .map((leaf) => leaf.textContent)
    .filter((text) => text.trim() !== "");
  return { repr, chunks };
}

function leavesOf(node, leaves = []) {
  if (node.childNodes.length === 0) leaves.push(node);
  for (const child of node.childNodes) leavesOf(child, leaves);
  return leaves;
}

export function paintHighlight(highlight) {
  const chunks = highlight.chunks || [];
  if (!chunks.length) return;

  const stream = textStream(document.body);
  const start = stream.text.indexOf(chunks.join(""));
  if (start === -1) return;

  const slices = [];
  let offset = start;
  for (const chunk of chunks) {
    slices.push({ from: stream.positionAt(offset), to: stream.positionAt(offset + chunk.length) });
    offset += chunk.length;
  }

  // Wrapping splits text nodes, so go right to left: earlier positions stay valid.
  for (let i = slices.length - 1; i >= 0; i--) {
    wrapRange(slices[i], highlight.groupID, stream);
  }
}

function textStream(root) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (isPaintable(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  const nodes = [];
  while (walker.nextNode()) nodes.push(walker.currentNode);

  const starts = [0];
  for (const node of nodes) starts.push(starts[starts.length - 1] + node.nodeValue.length);

  return {
    nodes,
    text: nodes.map((node) => node.nodeValue).join(""),
    indexOf: new Map(nodes.map((node, i) => [node, i])),
    positionAt(index) {
      let lo = 0;
      let hi = nodes.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (starts[mid] <= index) lo = mid;
        else hi = mid - 1;
      }
      return { node: nodes[lo], offset: index - starts[lo] };
    },
  };
}

function isPaintable(node) {
  if (!node.nodeValue || !node.parentNode) return false;
  const tag = node.parentNode.nodeName.toLowerCase();
  if (tag === "script" || tag === "style" || tag === "noscript") return false;
  return !insideHighlight(node);
}

function insideHighlight(node) {
  return Boolean(highlightAt(node.parentElement));
}

function wrapRange({ from, to }, groupId, stream) {
  const first = stream.indexOf.get(from.node);
  const last = stream.indexOf.get(to.node);
  if (first === undefined || last === undefined) return;

  if (first === last) {
    wrapSlice(from.node, from.offset, to.offset, groupId);
    return;
  }
  wrapSlice(from.node, from.offset, from.node.nodeValue.length, groupId);
  for (let i = first + 1; i < last; i++) {
    const node = stream.nodes[i];
    wrapSlice(node, 0, node.nodeValue.length, groupId);
  }
  wrapSlice(to.node, 0, to.offset, groupId);
}

function wrapSlice(textNode, start, end, groupId) {
  if (!textNode || start >= end || insideHighlight(textNode)) return;

  const text = textNode.nodeValue;
  const fragment = document.createDocumentFragment();
  if (start > 0) fragment.appendChild(document.createTextNode(text.slice(0, start)));
  fragment.appendChild(highlightSpan(text.slice(start, end), groupId));
  if (end < text.length) fragment.appendChild(document.createTextNode(text.slice(end)));
  textNode.parentNode.replaceChild(fragment, textNode);
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
    parent.replaceChild(document.createTextNode(span.textContent), span);
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
