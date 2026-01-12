chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "highlight",
    title: "Highlight Text",
    contexts: ["selection"],
  });

  // Generate unique sourceId on first install
  chrome.storage.local.get({ sourceId: null }, (data) => {
    if (!data.sourceId) {
      const sourceId = crypto.randomUUID();
      chrome.storage.local.set({ sourceId });
      console.log("Generated new sourceId:", sourceId);
    }
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "highlight") {
    chrome.tabs.sendMessage(tab.id, { action: "highlight" });
  }
});
