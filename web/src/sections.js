// Sections are ranges of printed pages, but a highlight records its PDF page:
// PDF page = printed page + page_offset.
function pdfPageOf(highlight) {
  const page = highlight.location?.payload?.page_number;
  return Number.isInteger(page) ? page : null;
}

export function printedPageOf(highlight, pageOffset = 0) {
  const page = pdfPageOf(highlight);
  return page === null ? null : page - pageOffset;
}

export const chaptersOf = (sections = []) =>
  sections.map((section, index) => ({
    key: index + 1,
    title: section.title,
    startPage: section.start_page,
    endPage: section.end_page,
  }));

function inChapter(highlight, chapter, pageOffset) {
  const page = printedPageOf(highlight, pageOffset);
  return page !== null && page >= chapter.startPage && page <= chapter.endPage;
}

export const highlightsInChapter = (highlights, chapter, pageOffset = 0) =>
  highlights.filter((highlight) => inChapter(highlight, chapter, pageOffset));

export const unchapteredHighlights = (highlights, chapters, pageOffset = 0) =>
  highlights.filter((highlight) => !chapters.some((chapter) => inChapter(highlight, chapter, pageOffset)));

export function groupByPage(highlights, pageOffset = 0) {
  const pages = new Map();
  for (const highlight of highlights) {
    const page = printedPageOf(highlight, pageOffset);
    if (!pages.has(page)) pages.set(page, []);
    pages.get(page).push(highlight);
  }
  return [...pages.entries()]
    .sort(([a], [b]) => (a === null) - (b === null) || a - b)
    .map(([page, grouped]) => ({ page, highlights: grouped }));
}
