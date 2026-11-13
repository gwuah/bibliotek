// Nothing is inserted inside highlight spans, so the text stream used to find
// highlights again is untouched. Note text is set with textContent only: stored
// text is never parsed as HTML.

import { noteOf, systemClock, withNote } from "../lib/library.js";
import { HIGHLIGHT_CLASS, highlightAt, highlightsVisible, spansOf } from "./highlighter.js";
import { UI_CSS, pageCss } from "./notes-style.js";

const GAP = 40;
const MAX_WIDTH = 400;
const EDGE = 32;
const MIN_WIDTH = 150;
const STACK_GAP = 14;
const FLOATING_WIDTH = 320;

export function createNotes({ saveNote, deleteHighlight }) {
  const highlights = new Map();
  const notes = new Map();
  let ui = null;
  let editingId = null;
  let armedRemove = false;
  let layoutQueued = false;
  let xTimer = null;

  const noteText = (groupId) => noteOf(highlights.get(groupId));

  function setLibrary(library) {
    highlights.clear();
    for (const page of Object.values(library || {})) {
      for (const h of page) highlights.set(String(h.groupID), h);
    }
    if (editingId && !highlights.has(editingId)) stopEdit();
    refreshMarkers();
  }

  function blockOf(node) {
    for (let el = node.parentElement; el && el !== document.body; el = el.parentElement) {
      const display = getComputedStyle(el).display;
      if (!display.startsWith("inline") && display !== "contents") return el;
    }
    return document.body;
  }

  function pageBackground(el) {
    for (; el; el = el.parentElement) {
      const bg = getComputedStyle(el).backgroundColor;
      if (bg && bg !== "transparent" && !bg.endsWith(", 0)")) return bg;
    }
    return "#fff";
  }

  function ensureUI() {
    if (ui) return ui;

    const pageStyle = document.createElement("style");
    pageStyle.textContent = pageCss();
    (document.head || document.documentElement).appendChild(pageStyle);

    const host = document.createElement("light-notes");
    host.style.cssText = "position:absolute;top:0;left:0;width:0;height:0;overflow:visible;z-index:2147483646;";
    const root = host.attachShadow({ mode: "open" });
    const style = document.createElement("style");
    style.textContent = UI_CSS;
    root.appendChild(style);
    document.documentElement.appendChild(host);

    for (const type of ["keydown", "keyup", "keypress", "input"]) {
      host.addEventListener(type, (e) => e.stopPropagation());
    }

    const x = element("div", "x", "x");
    x.setAttribute("role", "button");
    x.setAttribute("aria-label", "Delete highlight");
    x.addEventListener("mouseenter", () => clearTimeout(xTimer));
    x.addEventListener("mouseleave", hideX);
    x.addEventListener("click", onXClick);
    root.appendChild(x);

    ui = { root, x };
    return ui;
  }

  function showX(span) {
    const { x } = ensureUI();
    clearTimeout(xTimer);
    const rects = span.getClientRects();
    const rect = rects[rects.length - 1];
    if (!rect) return;
    if (x.dataset.groupId !== span.dataset.groupId) disarmX();
    x.dataset.groupId = span.dataset.groupId;
    const size = Math.max(14, Math.min(20, parseFloat(getComputedStyle(span).fontSize) * 0.8 || 16));
    x.style.fontSize = `${size}px`;
    x.style.display = "block";
    x.style.left = `${rect.right + window.scrollX - 4}px`;
    x.style.top = `${rect.top + window.scrollY - size * 0.75}px`;
  }

  function hideX() {
    clearTimeout(xTimer);
    xTimer = setTimeout(() => {
      if (!ui) return;
      ui.x.style.display = "none";
      disarmX();
    }, 300);
  }

  function disarmX() {
    if (!ui) return;
    ui.x.classList.remove("armed");
    ui.x.textContent = "x";
    ui.x.style.background = "";
  }

  function onXClick(e) {
    e.stopPropagation();
    const groupId = ui.x.dataset.groupId;
    if (noteText(groupId) && !ui.x.classList.contains("armed")) {
      ui.x.classList.add("armed");
      ui.x.textContent = "delete?";
      const span = spansOf(groupId).pop();
      if (span) ui.x.style.background = pageBackground(blockOf(span));
      return;
    }
    ui.x.style.display = "none";
    disarmX();
    if (editingId === groupId) stopEdit();
    deleteHighlight(groupId);
  }

  function createNote(groupId) {
    const note = element("div", "note");
    const text = element("div", "text");
    text.spellcheck = true;
    const remove = element("button", "remove", "Remove highlight");
    const hint = element("div", "hint", "⌘↵ to save · Esc to close · ");
    hint.appendChild(remove);
    note.append(text, hint);

    note.addEventListener("mouseenter", () => setActive(groupId, true));
    note.addEventListener("mouseleave", () => editingId !== groupId && setActive(groupId, false));
    text.addEventListener("mousedown", () => editingId !== groupId && startEdit(groupId));
    text.addEventListener("input", queueLayout);
    text.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        commit();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      }
    });
    remove.addEventListener("mousedown", (e) => e.preventDefault()); // keep focus in the note
    remove.addEventListener("click", () => {
      if (!armedRemove) {
        armedRemove = true;
        remove.textContent = "Click again to remove";
        remove.classList.add("armed");
        return;
      }
      const id = editingId;
      stopEdit();
      deleteHighlight(id);
    });

    ensureUI().root.appendChild(note);
    notes.set(groupId, note);
    return note;
  }

  function layout() {
    layoutQueued = false;
    const wanted = new Set();
    if (highlightsVisible()) {
      for (const groupId of highlights.keys()) {
        if (noteText(groupId) || groupId === editingId) wanted.add(groupId);
      }
    }

    for (const [groupId, note] of notes) {
      if (!wanted.has(groupId) || !spansOf(groupId).length) {
        note.remove();
        notes.delete(groupId);
      }
    }

    const docWidth = document.documentElement.clientWidth + window.scrollX;
    const inMargin = [];

    for (const groupId of wanted) {
      const spans = spansOf(groupId);
      if (!spans.length) continue;
      const note = notes.get(groupId) || createNote(groupId);
      if (groupId !== editingId) note.querySelector(".text").textContent = noteText(groupId);

      const block = blockOf(spans[0]);
      const blockRect = block.getBoundingClientRect();
      const first = spans[0].getClientRects()[0] || spans[0].getBoundingClientRect();
      const lastRects = spans[spans.length - 1].getClientRects();
      const last = lastRects[lastRects.length - 1] || first;

      const left = blockRect.right + window.scrollX + GAP;
      const width = Math.min(MAX_WIDTH, docWidth - left - EDGE);
      note.style.color = getComputedStyle(block).color;

      if (width >= MIN_WIDTH) {
        note.classList.remove("floating");
        note.style.background = "";
        note.style.left = `${left}px`;
        note.style.width = `${width}px`;
        note.style.display = "";
        inMargin.push({ note, top: first.top + window.scrollY });
      } else {
        const active = groupId === editingId || note.classList.contains("lit");
        note.classList.add("floating");
        note.style.display = active ? "" : "none";
        note.style.background = pageBackground(block);
        note.style.left = `${Math.max(window.scrollX + 12, first.left + window.scrollX)}px`;
        note.style.width = `${Math.min(FLOATING_WIDTH, document.documentElement.clientWidth - 24)}px`;
        note.style.top = `${last.bottom + window.scrollY + 6}px`;
      }
    }

    inMargin.sort((a, b) => a.top - b.top);
    let floor = -Infinity;
    for (const { note, top } of inMargin) {
      const y = Math.max(top, floor);
      note.style.top = `${y}px`;
      floor = y + note.offsetHeight + STACK_GAP;
    }
  }

  function queueLayout() {
    if (layoutQueued) return;
    layoutQueued = true;
    requestAnimationFrame(layout);
  }

  function setActive(groupId, on) {
    spansOf(groupId).forEach((s) => s.classList.toggle("light-active", on));
    const note = notes.get(groupId);
    note?.classList.toggle("lit", on);
    if (note?.classList.contains("floating")) queueLayout();
  }

  function refreshMarkers() {
    document.querySelectorAll(`.${HIGHLIGHT_CLASS}`).forEach((s) => s.classList.remove("light-has-note"));
    for (const groupId of highlights.keys()) {
      if (noteText(groupId)) spansOf(groupId).forEach((s) => s.classList.add("light-has-note"));
    }
    ensureUI();
    queueLayout();
  }

  function startEdit(groupId) {
    if (!highlights.has(groupId)) return;
    if (editingId && editingId !== groupId) commit();
    editingId = groupId;
    armedRemove = false;
    setActive(groupId, true);
    layout();

    const note = notes.get(groupId);
    if (!note) return;
    const text = note.querySelector(".text");
    const remove = note.querySelector(".remove");
    remove.textContent = "Remove highlight";
    remove.classList.remove("armed");
    note.classList.add("editing");
    text.contentEditable = "plaintext-only";
    text.focus({ preventScroll: true });
    placeCaretAtEnd(text);
  }

  function placeCaretAtEnd(el) {
    const selection = ui.root.getSelection ? ui.root.getSelection() : window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  function stopEdit() {
    const groupId = editingId;
    if (!groupId) return;
    editingId = null;
    armedRemove = false;
    const note = notes.get(groupId);
    if (note) {
      note.classList.remove("editing");
      note.querySelector(".text").contentEditable = "false";
    }
    setActive(groupId, false);
    queueLayout();
  }

  function commit() {
    const groupId = editingId;
    if (!groupId) return;
    const note = notes.get(groupId);
    const value = note ? note.querySelector(".text").innerText.trim() : "";
    stopEdit();
    if (value === noteText(groupId)) return;

    highlights.set(groupId, withNote(highlights.get(groupId), value, systemClock()));
    saveNote(groupId, value);
    refreshMarkers();
  }

  function cancel() {
    const groupId = editingId;
    stopEdit();
    const note = groupId && notes.get(groupId);
    if (note) note.querySelector(".text").textContent = noteText(groupId);
    queueLayout();
  }

  document.addEventListener("mouseover", (e) => {
    const span = highlightAt(e.target);
    if (!span || !highlightsVisible()) return;
    setActive(span.dataset.groupId, true);
    showX(span);
  });

  document.addEventListener("mouseout", (e) => {
    const span = highlightAt(e.target);
    if (!span) return;
    const groupId = span.dataset.groupId;
    const to = highlightAt(e.relatedTarget);
    if (to && to.dataset.groupId === groupId) return;
    hideX();
    if (groupId !== editingId) setActive(groupId, false);
  });

  document.addEventListener("click", (e) => {
    const span = highlightAt(e.target);
    if (!span || !highlightsVisible()) return;
    if (!window.getSelection().isCollapsed) return;
    if (span.closest("a")) return;
    startEdit(span.dataset.groupId);
  });

  document.addEventListener(
    "mousedown",
    (e) => {
      if (!editingId || !ui) return;
      const note = notes.get(editingId);
      if (note && e.composedPath().includes(note)) return;
      const span = highlightAt(e.target);
      if (span && span.dataset.groupId === editingId) return;
      commit();
    },
    true
  );

  window.addEventListener("resize", queueLayout);
  if (window.ResizeObserver) new ResizeObserver(queueLayout).observe(document.documentElement);

  return {
    setLibrary,
    refreshMarkers,

    editNew(highlight) {
      const groupId = String(highlight.groupID);
      if (!highlights.has(groupId)) highlights.set(groupId, highlight);
      window.getSelection().removeAllRanges();
      startEdit(groupId);
    },

    relayout() {
      if (!highlightsVisible()) stopEdit();
      queueLayout();
    },
  };
}

function element(tag, className, text) {
  const el = document.createElement(tag);
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}
