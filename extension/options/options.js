import * as commands from "../lib/commands.js";
import { loadLibrary, loadSettings, saveSettings } from "../lib/storage.js";
import { hostPermissionFor, normalizeServerUrl } from "../lib/settings.js";
import { buildExport, exportFileName } from "../lib/transfer.js";

const $ = (id) => document.getElementById(id);

function showStatus(id, message, kind) {
  const status = $(id);
  status.textContent = message;
  status.className = `status ${kind}`;
  status.hidden = false;
}

async function showIdentity() {
  const { installationId } = await loadLibrary();
  $("installation-id").textContent = installationId || "(not generated yet; reopen this page)";
  $("extension-origin").textContent = new URL(chrome.runtime.getURL("")).origin;
}

async function showSettings() {
  const { serverUrl, adminToken } = await loadSettings();
  $("server-url").value = serverUrl;
  $("admin-token").value = adminToken;
}

$("settings-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  let serverUrl;
  try {
    serverUrl = normalizeServerUrl($("server-url").value);
  } catch (error) {
    showStatus("settings-status", error.message, "error");
    return;
  }

  // Requested inside the submit handler: Chrome only shows a permission prompt in
  // response to a user gesture.
  const pattern = hostPermissionFor(serverUrl);
  let granted = false;
  try {
    granted = await chrome.permissions.request({ origins: [pattern] });
  } catch (error) {
    showStatus("settings-status", `Could not ask for access to ${pattern}: ${error.message}`, "error");
    return;
  }

  await saveSettings({ serverUrl, adminToken: $("admin-token").value.trim() });
  $("server-url").value = serverUrl;
  if (granted) {
    showStatus("settings-status", "Saved.", "success");
  } else {
    showStatus(
      "settings-status",
      `Saved, but access to ${pattern} was not granted. Sync then depends on liotek allowing this ` +
        "extension's origin in extension_origins.",
      "error"
    );
  }
});

$("export").addEventListener("click", async () => {
  const { installationId, highlights } = await loadLibrary();
  if (!installationId) {
    showStatus("transfer-status", "This installation has no ID yet; reopen this page and try again.", "error");
    return;
  }
  const exportedAt = new Date().toISOString();
  const file = buildExport({ installationId, highlights, exportedAt });
  const blob = new Blob([JSON.stringify(file, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);

  const link = document.createElement("a");
  link.href = url;
  link.download = exportFileName(installationId, exportedAt);
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
  showStatus("transfer-status", `Exported to ${link.download}.`, "success");
});

$("import-file").addEventListener("change", async (event) => {
  const input = event.target;
  const [file] = input.files;
  input.value = ""; // choosing the same file again still fires a change
  if (!file) return;

  try {
    const result = await commands.importLibrary(await file.text());
    const notes = result.commentsAdded ? ` and ${result.commentsAdded} notes` : "";
    const identity = result.adopted ? ` This extension now uses installation ${result.installationId}.` : "";
    showStatus("transfer-status", `Imported ${result.added} highlights${notes}.${identity}`, "success");
    showIdentity();
  } catch (error) {
    showStatus("transfer-status", `Nothing was imported. ${error.message}`, "error");
  }
});

showIdentity();
showSettings();
