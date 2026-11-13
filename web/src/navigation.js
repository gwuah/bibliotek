export const NAVIGATED = "popstate";

export function navigate(path) {
  if (window.location.pathname === path) return;
  window.history.pushState({ from: window.location.pathname }, "", path);
  window.dispatchEvent(new PopStateEvent(NAVIGATED));
}

export const cameFrom = () => window.history.state?.from ?? null;

export const documentPath = (id) => `/documents/${id}`;

export const isPlainClick = (event) =>
  event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
