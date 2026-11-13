// Built with textContent only, so stored text is never parsed as HTML.

import { countHighlights, groupBySite, noteOf } from "../lib/library.js";

const TRACKING_PARAMS = ["utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content"];

export function displayPath(url) {
  try {
    const parsed = new URL(url);
    for (const param of TRACKING_PARAMS) parsed.searchParams.delete(param);
    return parsed.pathname + parsed.search;
  } catch {
    return url;
  }
}

export function formatDate(time) {
  const date = new Date(time);
  const day = String(date.getDate()).padStart(2, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  return `${day}/${month}/${String(date.getFullYear()).slice(-2)}`;
}

export function renderList(container, library, { open, onShow, onDelete }) {
  container.replaceChildren();
  for (const site of groupBySite(library)) {
    const pages = div("domain-content");
    for (const page of site.pages) {
      const highlights = div("highlights-for-url");
      for (const highlight of page.highlights) {
        highlights.appendChild(renderHighlight(highlight, { onShow, onDelete }));
      }
      const pageRow = div("url-item page-item", displayPath(page.url));
      pages.append(pageRow, highlights);
      collapsible(pageRow, highlights, `page:${page.url}`, open);
    }

    const siteRow = div("url-item site-item");
    siteRow.append(span("site-name", site.site), span("site-date", formatDate(site.latest)));
    container.append(siteRow, pages);
    collapsible(siteRow, pages, `site:${site.site}`, open);
  }
}

export function libraryStats(library) {
  return { websites: groupBySite(library).length, highlights: countHighlights(library) };
}

function renderHighlight(highlight, { onShow, onDelete }) {
  const item = div("highlight-text");

  const text = document.createElement("p");
  text.textContent = highlight.repr;
  text.title = "Double-click to scroll to it on the page";
  text.addEventListener("dblclick", () => onShow(highlight));

  const remove = span("delete", " x");
  remove.setAttribute("role", "button");
  remove.setAttribute("aria-label", "Delete highlight");
  remove.addEventListener("click", (event) => {
    event.stopPropagation();
    onDelete(highlight);
  });
  text.appendChild(remove);
  item.appendChild(text);

  const note = noteOf(highlight);
  if (note) {
    const noteText = document.createElement("p");
    noteText.className = "note-text";
    noteText.textContent = note;
    item.appendChild(noteText);
  }
  return item;
}

function collapsible(row, content, key, open) {
  content.hidden = !open.has(key);
  row.addEventListener("click", () => {
    content.hidden = !content.hidden;
    if (content.hidden) open.delete(key);
    else open.add(key);
  });
}

function div(className, text) {
  const el = document.createElement("div");
  el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}
