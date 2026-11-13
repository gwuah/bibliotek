async function send(type, args = {}) {
  const reply = await chrome.runtime.sendMessage({ type, ...args });
  if (!reply) throw new Error(`The extension's background did not answer "${type}".`);
  if (!reply.ok) throw new Error(reply.error);
  return reply.value;
}

export const addHighlight = (highlight) => send("addHighlight", { highlight });

export const deleteHighlight = (groupId) => send("deleteHighlight", { groupId: String(groupId) });

export const setNote = (groupId, text) => send("setNote", { groupId: String(groupId), text });

export const importLibrary = (fileText) => send("importLibrary", { fileText });
