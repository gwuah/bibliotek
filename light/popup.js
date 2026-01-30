const COMMONPLACE_API = "http://localhost:5678";

let activeTab = "highlights";

document.addEventListener("DOMContentLoaded", () => {
  loadAndRenderHighlights();
  loadAndRenderWords();
  setupTabs();
});

const toggleBtn = document.getElementById("toggle");
toggleBtn.addEventListener("click", () => {
  chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
    chrome.tabs.sendMessage(
      tabs[0].id,
      { action: "toggleHighlight" },
      (response) => {},
    );
  });
});

const exportBtn = document.getElementById("export");
exportBtn.addEventListener("click", () => {
  chrome.storage.local.get({ highlights: {} }, (data) => {
    navigator.clipboard
      .writeText(JSON.stringify(data.highlights))
      .then(() => {
        showSyncStatus("Copied to clipboard!", "success");
        setTimeout(hideSyncStatus, 1500);
      })
      .catch((err) => {
        console.error("Failed to copy: ", err);
        showSyncStatus("Failed to copy", "error");
        setTimeout(hideSyncStatus, 2000);
      });
  });
});

const importBtn = document.getElementById("import");
const importContainer = document.getElementById("import-container");
const importInput = document.getElementById("import-input");
const importCancel = document.getElementById("import-cancel");
const importConfirm = document.getElementById("import-confirm");

importBtn.addEventListener("click", () => {
  importContainer.style.display =
    importContainer.style.display === "none" ? "block" : "none";
  importInput.value = "";
});

importCancel.addEventListener("click", () => {
  importContainer.style.display = "none";
  importInput.value = "";
});

importConfirm.addEventListener("click", () => {
  const jsonText = importInput.value.trim();
  if (!jsonText) {
    showSyncStatus("No JSON provided", "error");
    setTimeout(hideSyncStatus, 2000);
    return;
  }

  let imported;
  try {
    imported = JSON.parse(jsonText);
  } catch (e) {
    showSyncStatus("Invalid JSON", "error");
    setTimeout(hideSyncStatus, 2000);
    return;
  }

  if (typeof imported !== "object" || Array.isArray(imported)) {
    showSyncStatus("JSON must be an object", "error");
    setTimeout(hideSyncStatus, 2000);
    return;
  }

  chrome.storage.local.get({ highlights: {} }, (data) => {
    const existing = data.highlights;
    let added = 0;

    for (const [url, highlights] of Object.entries(imported)) {
      if (!Array.isArray(highlights)) continue;

      if (!existing[url]) {
        existing[url] = [];
      }

      const existingIds = new Set(existing[url].map((h) => h.groupID));
      for (const h of highlights) {
        if (h.groupID && !existingIds.has(h.groupID)) {
          existing[url].push(h);
          added++;
        }
      }
    }

    chrome.storage.local.set({ highlights: existing }, () => {
      importContainer.style.display = "none";
      importInput.value = "";
      showSyncStatus(`Imported ${added} highlights`, "success");
      setTimeout(hideSyncStatus, 2000);
      loadAndRenderHighlights();
    });
  });
});

const syncHighlightsBtn = document.getElementById("sync-highlights");
syncHighlightsBtn.addEventListener("click", () => {
  syncHighlightsToCommonplace();
});

const syncWordsBtn = document.getElementById("sync-words");
syncWordsBtn.addEventListener("click", () => {
  syncWordsToCommonplace();
});

function setupTabs() {
  const tabHighlights = document.getElementById("tab-highlights");
  const tabWords = document.getElementById("tab-words");
  const contentHighlights = document.getElementById("tab-content-highlights");
  const contentWords = document.getElementById("tab-content-words");

  tabHighlights.addEventListener("click", () => {
    activeTab = "highlights";
    tabHighlights.classList.add("active");
    tabWords.classList.remove("active");
    contentHighlights.style.display = "block";
    contentWords.style.display = "none";
  });

  tabWords.addEventListener("click", () => {
    activeTab = "words";
    tabWords.classList.add("active");
    tabHighlights.classList.remove("active");
    contentWords.style.display = "block";
    contentHighlights.style.display = "none";
  });
}

function showSyncStatus(message, type) {
  const statusEl = document.getElementById("sync-status");
  statusEl.textContent = message;
  statusEl.className = `sync-status barlow-regular ${type}`;
  statusEl.style.display = "block";
}

function hideSyncStatus() {
  const statusEl = document.getElementById("sync-status");
  statusEl.style.display = "none";
}

async function syncHighlightsToCommonplace() {
  showSyncStatus("Syncing highlights...", "syncing");

  chrome.storage.local.get({ highlights: {}, sourceId: null }, async (data) => {
    const highlights = data.highlights;
    let sourceId = data.sourceId;

    // Generate sourceId if it doesn't exist (for existing installations)
    if (!sourceId) {
      sourceId = crypto.randomUUID();
      chrome.storage.local.set({ sourceId });
    }

    const source = `light_${sourceId}`;

    if (Object.keys(highlights).length === 0) {
      showSyncStatus("No highlights to sync", "error");
      setTimeout(hideSyncStatus, 2000);
      return;
    }

    try {
      const response = await fetch(`${COMMONPLACE_API}/light/sync_highlights`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ source, highlights }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const result = await response.json();
      const {
        annotations_created,
        annotations_updated,
        annotations_deleted,
        annotations_unchanged,
      } = result.data;

      const message = `+${annotations_created} new, ${annotations_updated} updated, ${annotations_deleted} deleted, ${annotations_unchanged} unchanged`;
      showSyncStatus(message, "success");

      // Store last sync timestamp
      chrome.storage.local.set({ lastSync: new Date().toISOString() });

      setTimeout(hideSyncStatus, 3000);
    } catch (err) {
      console.error("Sync failed:", err);
      showSyncStatus(`Sync failed: ${err.message}`, "error");
      setTimeout(hideSyncStatus, 3000);
    }
  });
}

async function syncWordsToCommonplace() {
  showSyncStatus("Syncing words...", "syncing");

  chrome.storage.local.get({ words: [], sourceId: null }, async (data) => {
    const words = data.words;
    let sourceId = data.sourceId;

    // Generate sourceId if it doesn't exist (for existing installations)
    if (!sourceId) {
      sourceId = crypto.randomUUID();
      chrome.storage.local.set({ sourceId });
    }

    const source = `light_${sourceId}`;

    if (words.length === 0) {
      showSyncStatus("No words to sync", "error");
      setTimeout(hideSyncStatus, 2000);
      return;
    }

    try {
      const response = await fetch(`${COMMONPLACE_API}/light/sync_words`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ source, words }),
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }

      const result = await response.json();
      const { words_created, words_updated, words_deleted, words_unchanged } =
        result.data;

      const message = `+${words_created} new, ${words_updated} updated, ${words_deleted} deleted, ${words_unchanged} unchanged`;
      showSyncStatus(message, "success");

      // Store last sync timestamp
      chrome.storage.local.set({ lastWordsSync: new Date().toISOString() });

      setTimeout(hideSyncStatus, 3000);
    } catch (err) {
      console.error("Words sync failed:", err);
      showSyncStatus(`Sync failed: ${err.message}`, "error");
      setTimeout(hideSyncStatus, 3000);
    }
  });
}

function loadAndRenderHighlights() {
  chrome.storage.local.get({ highlights: {} }, (data) => {
    renderHighlights(data.highlights);
  });
}

function getmultipagedomains(highlightsByUrl) {
  const urls = {};
  Object.keys(highlightsByUrl).forEach((url) => {
    const hostname = new URL(url).hostname;
    if (!urls[hostname]) {
      urls[hostname] = 1;
    } else {
      urls[hostname]++;
    }
  });

  return (url) => {
    return urls[new URL(url).hostname] > 1;
  };
}

function getLongestString(strings) {
  if (!Array.isArray(strings) || strings.length === 0) {
    return null; // or throw an error depending on use case
  }

  return strings.reduce((longest, current) => {
    return current.length > longest.length ? current : longest;
  }, "");
}

function cleanUrl(url) {
  try {
    const urlObj = new URL(url);
    const searchParams = new URLSearchParams(urlObj.search);

    // Remove UTM parameters
    const utmParams = [
      "utm_source",
      "utm_medium",
      "utm_campaign",
      "utm_term",
      "utm_content",
    ];
    utmParams.forEach((param) => searchParams.delete(param));

    // Reconstruct the URL with cleaned parameters
    const cleanedSearch = searchParams.toString();
    return urlObj.pathname + (cleanedSearch ? "?" + cleanedSearch : "");
  } catch (e) {
    return url;
  }
}

function getLatestTimestamp(highlights) {
  return Math.max(...highlights.map((h) => new Date(h.date).getTime()));
}

function formatDate(timestamp) {
  const date = new Date(timestamp);
  const day = String(date.getDate()).padStart(2, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const year = String(date.getFullYear()).slice(-2);
  return `${day}/${month}/${year}`;
}

function renderHighlights(highlightsByUrl) {
  const stats = {
    totalWebsites: 0,
    totalHighlights: 0,
  };

  const highlightsList = document.getElementById("highlights-list");
  const totalWebsitesSpan = document.getElementById("total-websites");
  const totalHighlightsSpan = document.getElementById("total-highlights");

  // Clear the list before rendering
  highlightsList.innerHTML = "";

  // Group URLs by domain
  const domainGroups = {};
  Object.keys(highlightsByUrl).forEach((url) => {
    const hostname = new URL(url).hostname;
    if (!domainGroups[hostname]) {
      domainGroups[hostname] = {};
    }
    domainGroups[hostname][url] = highlightsByUrl[url];
  });

  // Sort domains by latest highlight timestamp
  const sortedDomains = Object.keys(domainGroups).sort((a, b) => {
    const latestA = Math.max(
      ...Object.values(domainGroups[a])
        .flat()
        .map((h) => new Date(h.date).getTime()),
    );
    const latestB = Math.max(
      ...Object.values(domainGroups[b])
        .flat()
        .map((h) => new Date(h.date).getTime()),
    );
    return latestB - latestA;
  });

  stats.totalWebsites = sortedDomains.length;

  sortedDomains.forEach((domain) => {
    const urlsInDomain = domainGroups[domain];

    // Count total highlights for this domain
    const totalHighlightsInDomain = Object.values(urlsInDomain).reduce(
      (sum, highlights) => sum + highlights.length,
      0,
    );
    stats.totalHighlights += totalHighlightsInDomain;

    // Get latest timestamp for this domain
    const latestTimestamp = Math.max(
      ...Object.values(urlsInDomain)
        .flat()
        .map((h) => new Date(h.date).getTime()),
    );
    const formattedDate = formatDate(latestTimestamp);

    // Create domain header
    const domainItem = document.createElement("div");
    domainItem.className = "url-item";
    domainItem.style.display = "flex";
    domainItem.style.justifyContent = "space-between";
    domainItem.style.alignItems = "center";

    const domainText = document.createElement("span");
    domainText.textContent = domain;

    const dateText = document.createElement("span");
    dateText.textContent = formattedDate;
    dateText.style.fontSize = "12px";
    dateText.style.color = "#666";

    domainItem.appendChild(domainText);
    domainItem.appendChild(dateText);

    const domainContent = document.createElement("div");
    domainContent.className = "domain-content";
    domainContent.style.display = "none";

    // Sort URLs within domain by latest highlight timestamp
    const sortedUrls = Object.keys(urlsInDomain).sort((a, b) => {
      const latestA = getLatestTimestamp(urlsInDomain[a]);
      const latestB = getLatestTimestamp(urlsInDomain[b]);
      return latestB - latestA;
    });

    sortedUrls.forEach((url) => {
      const highlights = urlsInDomain[url];
      const cleanPath = cleanUrl(url);

      const pathItem = document.createElement("div");
      pathItem.className = "url-item";
      pathItem.style.paddingLeft = "20px";
      pathItem.textContent = cleanPath;

      const highlightsForUrl = document.createElement("div");
      highlightsForUrl.className = "highlights-for-url";
      highlightsForUrl.style.display = "none";
      highlightsForUrl.style.paddingLeft = "30px";

      highlights.forEach((highlight) => {
        const highlightDiv = document.createElement("div");
        highlightDiv.className = "highlight-text";

        const highlightText = document.createElement("p");
        highlightText.textContent = `${highlight.repr}`;
        highlightText.addEventListener("dblclick", (ev) => {
          chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
            chrome.tabs.sendMessage(
              tabs[0].id,
              { action: "scrollToHighlight", groupID: highlight.groupID },
              (response) => {
                console.log("Response from content script:", response);
              },
            );
          });
        });

        const deleteBtn = document.createElement("span");
        deleteBtn.textContent = " x";
        deleteBtn.style.color = "red";
        deleteBtn.style.cursor = "pointer";
        deleteBtn.addEventListener("click", (e) => {
          e.stopPropagation();
          deleteHighlightFromPopup(url, highlight.groupID);
        });

        highlightText.appendChild(deleteBtn);
        highlightDiv.appendChild(highlightText);
        highlightsForUrl.appendChild(highlightDiv);
      });

      pathItem.addEventListener("click", () => {
        const isDisplayed = highlightsForUrl.style.display === "block";
        highlightsForUrl.style.display = isDisplayed ? "none" : "block";
      });

      domainContent.appendChild(pathItem);
      domainContent.appendChild(highlightsForUrl);
    });

    domainItem.addEventListener("click", () => {
      const isDisplayed = domainContent.style.display === "block";
      domainContent.style.display = isDisplayed ? "none" : "block";
    });

    highlightsList.appendChild(domainItem);
    highlightsList.appendChild(domainContent);
  });

  totalWebsitesSpan.textContent = stats.totalWebsites;
  totalHighlightsSpan.textContent = stats.totalHighlights;
}

function deleteHighlightFromPopup(url, groupID) {
  chrome.storage.local.get({ highlights: {} }, (data) => {
    let highlights = data.highlights;
    if (highlights[url]) {
      highlights[url] = highlights[url].filter((h) => h.groupID !== groupID);
      if (highlights[url].length === 0) {
        delete highlights[url];
      }
    }
    chrome.storage.local.set({ highlights: highlights }, () => {
      // Re-render the highlights
      renderHighlights(highlights);
    });
  });
}

// Words functions
function loadAndRenderWords() {
  chrome.storage.local.get({ words: [] }, (data) => {
    renderWords(data.words);
  });
}

function renderWords(words) {
  const wordsList = document.getElementById("words-list");
  const totalWordsSpan = document.getElementById("total-words");

  wordsList.innerHTML = "";

  // Sort words alphabetically
  const sortedWords = [...words].sort((a, b) =>
    a.word.toLowerCase().localeCompare(b.word.toLowerCase()),
  );

  totalWordsSpan.textContent = sortedWords.length;

  sortedWords.forEach((wordEntry) => {
    const wordItem = document.createElement("div");
    wordItem.className = "word-item";

    const wordText = document.createElement("span");
    wordText.className = "word-text";
    wordText.textContent = wordEntry.word;

    const deleteBtn = document.createElement("span");
    deleteBtn.className = "word-delete";
    deleteBtn.textContent = "x";
    deleteBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      deleteWordFromPopup(wordEntry.id);
    });

    wordItem.appendChild(wordText);
    wordItem.appendChild(deleteBtn);
    wordsList.appendChild(wordItem);
  });
}

function deleteWordFromPopup(wordId) {
  chrome.storage.local.get({ words: [] }, (data) => {
    const words = data.words.filter((w) => w.id !== wordId);
    chrome.storage.local.set({ words: words }, () => {
      renderWords(words);
    });
  });
}
