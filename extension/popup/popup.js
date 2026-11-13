import * as commands from "../lib/commands.js";
import { loadHighlights, loadLastSync, loadLibrary, loadSettings, onHighlightsChanged, recordSync } from "../lib/storage.js";
import { EMPTY_SNAPSHOT, describeStats, sendSnapshot } from "../lib/sync.js";
import { formatDate, libraryStats, renderList } from "./list.js";

const $ = (id) => document.getElementById(id);

const expanded = new Set();

function render(library) {
  const stats = libraryStats(library);
  $("total-websites").textContent = stats.websites;
  $("total-highlights").textContent = stats.highlights;
  renderList($("highlights-list"), library, {
    open: expanded,
    onShow: (highlight) => sendToPage({ action: "scrollToHighlight", groupID: highlight.groupID }),
    onDelete: (highlight) =>
      commands.deleteHighlight(highlight.groupID).catch((error) => showStatus(error.message, "error")),
  });
}

async function sendToPage(message) {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (tab?.id === undefined) return;
  chrome.tabs.sendMessage(tab.id, message).catch(() => {});
}

function showStatus(message, kind) {
  const status = $("status");
  status.textContent = message;
  status.className = `status ${kind}`;
  status.hidden = false;
}

async function showLastSync() {
  const last = await loadLastSync();
  $("last-sync").textContent = last ? `Last synced ${formatDate(last.at)} at ${new Date(last.at).toLocaleTimeString()}` : "";
}

async function sync({ confirmEmpty = false } = {}) {
  $("confirm-empty").hidden = true;
  $("sync").disabled = true;
  showStatus("Syncing…", "syncing");
  try {
    const [settings, library] = await Promise.all([loadSettings(), loadLibrary()]);
    const stats = await sendSnapshot({ ...settings, ...library, confirmEmpty });
    await recordSync(stats);
    showStatus(describeStats(stats), "success");
    showLastSync();
  } catch (error) {
    if (error.code === EMPTY_SNAPSHOT && !confirmEmpty) askToConfirmEmpty(error.message);
    else showStatus(`Sync failed: ${error.message}`, "error");
  } finally {
    $("sync").disabled = false;
  }
}

// Never resend with confirm_empty=true without the person agreeing: liotek would
// then remove everything this installation synced.
function askToConfirmEmpty(message) {
  $("status").hidden = true;
  $("confirm-empty-message").textContent = message;
  $("confirm-empty").hidden = false;
}

$("toggle").addEventListener("click", () => sendToPage({ action: "toggleHighlight" }));
$("sync").addEventListener("click", () => sync());
$("settings").addEventListener("click", () => chrome.runtime.openOptionsPage());
$("confirm-empty-accept").addEventListener("click", () => sync({ confirmEmpty: true }));
$("confirm-empty-cancel").addEventListener("click", () => {
  $("confirm-empty").hidden = true;
  showStatus("Nothing was sent. Your highlights in liotek are unchanged.", "success");
});

onHighlightsChanged(render);
loadHighlights().then(render);
showLastSync();
