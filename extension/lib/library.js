import { withoutFragment } from "./page.js";

export function addHighlight(library, highlight) {
  const page = library[highlight.url] || [];
  return { ...library, [highlight.url]: [...page, highlight] };
}

// Matched on groupID alone: a highlight is painted on every page of its site, so the
// page showing it may not be its key.
export function removeHighlight(library, groupId) {
  const id = String(groupId);
  const next = {};
  for (const [url, highlights] of Object.entries(library)) {
    const kept = highlights.filter((h) => String(h.groupID) !== id);
    if (kept.length) next[url] = kept;
  }
  return next;
}

export function findHighlight(library, groupId) {
  const id = String(groupId);
  for (const highlights of Object.values(library)) {
    const found = highlights.find((h) => String(h.groupID) === id);
    if (found) return found;
  }
  return null;
}

export function noteOf(highlight) {
  return (highlight?.comments || []).map((c) => c.text).join("\n\n");
}

export function withNote(highlight, text, clock) {
  const value = text.trim();
  if (value === noteOf(highlight)) return highlight;
  if (!value) return { ...highlight, comments: [] };

  const existing = (highlight.comments || [])[0];
  const comment = {
    id: existing?.id || clock.newId(),
    text: value,
    created: existing?.created || clock.now,
    updated: clock.now,
  };
  return { ...highlight, comments: [comment] };
}

export function setNote(library, groupId, text, clock) {
  const id = String(groupId);
  const next = {};
  for (const [url, highlights] of Object.entries(library)) {
    next[url] = highlights.map((h) => (String(h.groupID) === id ? withNote(h, text, clock) : h));
  }
  return next;
}

export function systemClock() {
  return {
    now: new Date().toISOString(),
    newId: () => `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`,
  };
}

export function mergeLibraries(existing, incoming) {
  const library = { ...existing };
  let added = 0;
  let commentsAdded = 0;

  for (const [url, highlights] of Object.entries(incoming)) {
    const page = [...(library[url] || [])];
    for (const highlight of highlights) {
      const index = page.findIndex((h) => String(h.groupID) === String(highlight.groupID));
      if (index === -1) {
        page.push(highlight);
        added++;
        continue;
      }
      const current = page[index];
      const have = new Set((current.comments || []).map((c) => c.id));
      const missing = (highlight.comments || []).filter((c) => !have.has(c.id));
      if (missing.length) {
        page[index] = { ...current, comments: [...(current.comments || []), ...missing] };
        commentsAdded += missing.length;
      }
    }
    library[url] = page;
  }
  return { library, added, commentsAdded };
}

export function countHighlights(library) {
  return Object.values(library).reduce((sum, highlights) => sum + highlights.length, 0);
}

export function hostnameOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return null;
  }
}

export function highlightsForPage(library, pageUrl, declaredUrl) {
  if (!declaredUrl) return highlightsForSite(library, pageUrl);
  return library[withoutFragment(declaredUrl)] ?? [];
}

// Matched by hostname because URLs drift between visits (fragments, query strings,
// redirects).
export function highlightsForSite(library, pageUrl) {
  const host = hostnameOf(pageUrl);
  if (!host) return [];
  return Object.entries(library)
    .filter(([url]) => hostnameOf(url) === host)
    .flatMap(([, highlights]) => highlights);
}

function timeOf(highlight) {
  const time = new Date(highlight.date).getTime();
  return Number.isNaN(time) ? 0 : time;
}

function latest(highlights) {
  return Math.max(0, ...highlights.map(timeOf));
}

export function groupBySite(library) {
  const sites = new Map();
  for (const [url, highlights] of Object.entries(library)) {
    const site = hostnameOf(url) || url;
    if (!sites.has(site)) sites.set(site, []);
    sites.get(site).push({ url, latest: latest(highlights), highlights });
  }
  return [...sites.entries()]
    .map(([site, pages]) => ({
      site,
      latest: Math.max(...pages.map((p) => p.latest)),
      pages: pages.sort((a, b) => b.latest - a.latest),
    }))
    .sort((a, b) => b.latest - a.latest);
}
