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

function reportError(error) {
  console.error("Light for liotek:", error);
}

async function highlightSelection() {
  const captured = captureSelection();
  if (!captured) return;

  const highlight = {
    groupID: Date.now(),
    repr: captured.repr,
    chunks: captured.chunks,
    url: window.location.href,
    date: new Date().toISOString(),
  };
  try {
    await commands.addHighlight(highlight);
  } catch (error) {
    reportError(error);
    return;
  }
  paintHighlight(highlight);
  notes.editNew(highlight);
}

async function deleteHighlight(groupId) {
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
