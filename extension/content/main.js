import * as commands from "../lib/commands.js";
import { highlightsForSite } from "../lib/library.js";
import { loadHighlights, onHighlightsChanged } from "../lib/storage.js";
import {
  captureSelection,
  highlightsVisible,
  paintHighlight,
  paintedGroupIds,
  scrollToHighlight,
  setHighlightsVisible,
  unpaintHighlight,
} from "./highlighter.js";
import { createNotes } from "./notes.js";

const PAINT_ATTEMPTS = 7;
const PAINT_INTERVAL_MS = 800;

const notes = createNotes({
  saveNote: (groupId, text) => commands.setNote(groupId, text).catch(reportError),
  deleteHighlight,
});

const STALE_NOTICE = "Light for liotek was updated or reloaded. Refresh this page to keep highlighting.";

// After the extension is reloaded or updated, Chrome leaves this script running in open
// tabs but cuts it off from the extension, and does not inject a new one until a reload.
function extensionGone() {
  return !globalThis.chrome?.runtime?.id;
}

function reportError(error) {
  if (extensionGone()) notes.showNotice(STALE_NOTICE);
  else console.error("Light for liotek:", error);
}

async function highlightSelection() {
  if (extensionGone()) {
    notes.showNotice(STALE_NOTICE);
    return;
  }
  const captured = captureSelection();
  if (!captured) return;

  const highlight = {
    groupID: Date.now(),
    repr: captured.repr,
    chunks: captured.chunks,
    anchor: captured.anchor,
    url: window.location.href,
    date: new Date().toISOString(),
  };
  try {
    await commands.addHighlight(highlight);
  } catch (error) {
    reportError(error);
    return;
  }
  if (!paintHighlight(highlight)) {
    console.error("Light for liotek: the selected text changed before it could be highlighted.");
    commands.deleteHighlight(highlight.groupID).catch(reportError);
    return;
  }
  notes.editNew(highlight);
}

async function deleteHighlight(groupId) {
  if (extensionGone()) {
    notes.showNotice(STALE_NOTICE);
    return;
  }
  try {
    await commands.deleteHighlight(groupId);
    unpaintHighlight(groupId);
  } catch (error) {
    reportError(error);
  }
}

function paintSite(library) {
  for (const highlight of highlightsForSite(library, window.location.href)) {
    paintHighlight(highlight);
  }
  notes.refreshMarkers();
}

async function restoreHighlights() {
  const library = await loadHighlights();
  notes.setLibrary(library);
  for (let attempt = 0; attempt < PAINT_ATTEMPTS; attempt++) {
    paintSite(library);
    await new Promise((resolve) => setTimeout(resolve, PAINT_INTERVAL_MS));
  }
}

onHighlightsChanged((library) => {
  const stored = new Set(highlightsForSite(library, window.location.href).map((h) => String(h.groupID)));
  for (const groupId of paintedGroupIds()) {
    if (!stored.has(groupId)) unpaintHighlight(groupId);
  }
  notes.setLibrary(library);
});

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  switch (request?.action) {
    case "highlight":
      highlightSelection();
      break;
    case "toggleHighlight":
      setHighlightsVisible(!highlightsVisible());
      notes.relayout();
      break;
    case "scrollToHighlight":
      scrollToHighlight(request.groupID);
      sendResponse({ found: true });
      break;
  }
});

document.addEventListener("keydown", (event) => {
  if (event.ctrlKey && event.key === "a") highlightSelection();
});

if (document.readyState === "complete") restoreHighlights();
else window.addEventListener("load", restoreHighlights, { once: true });
