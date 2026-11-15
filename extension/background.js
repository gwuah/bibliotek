import * as storage from "./lib/storage.js";
import { addHighlight, mergeSamePages, removeHighlight, setNote, systemClock } from "./lib/library.js";
import { parseExport, planImport } from "./lib/transfer.js";

const HIGHLIGHT_MENU = "highlight";

// Chrome keeps an extension's menu items across updates and reloads, and onInstalled runs
// on each of them, so the item is removed before it is created again.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: HIGHLIGHT_MENU, title: "Highlight Text", contexts: ["selection"] });
  });
  storage.updateLibrary((library) => ({ ...library, highlights: mergeSamePages(library.highlights) }));
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === HIGHLIGHT_MENU && tab?.id !== undefined) {
    chrome.tabs.sendMessage(tab.id, { action: "highlight" });
  }
});

storage.ensureInstallationId();

const commands = {
  async addHighlight({ highlight }) {
    await storage.updateLibrary((library) => ({
      ...library,
      highlights: addHighlight(library.highlights, highlight),
    }));
  },

  async deleteHighlight({ groupId }) {
    await storage.updateLibrary((library) => ({
      ...library,
      highlights: removeHighlight(library.highlights, groupId),
    }));
  },

  async setNote({ groupId, text }) {
    await storage.updateLibrary((library) => ({
      ...library,
      highlights: setNote(library.highlights, groupId, text, systemClock()),
    }));
  },

  async importLibrary({ fileText }) {
    const incoming = parseExport(fileText);
    const { installationId, added, commentsAdded, adopted } = await storage.updateLibrary((library) =>
      planImport(library, incoming)
    );
    return { installationId, added, commentsAdded, adopted };
  },
};

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  // Only the extension's own pages and content scripts may change the library.
  if (sender.id !== chrome.runtime.id) return false;
  const command = Object.hasOwn(commands, message?.type) ? commands[message.type] : null;
  if (!command) return false;

  command(message).then(
    (value) => sendResponse({ ok: true, value }),
    (error) => sendResponse({ ok: false, error: error.message })
  );
  return true; // the answer is sent asynchronously
});
