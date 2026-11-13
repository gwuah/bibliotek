import { HIGHLIGHT_CLASS } from "./highlighter.js";

export const RULE = "#C9A94F";
export const RED = "#E5484D";
const FONT = '"Light Barlow", "Barlow", -apple-system, BlinkMacSystemFont, sans-serif';

const LATIN =
  "U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD";
const LATIN_EXT =
  "U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C4, U+2113, U+2C60-2C7F, U+A720-A7FF";

// @font-face has no effect inside a shadow root, so the faces are declared on the
// page itself, under a family name no site will use.
function fontFace(weight, subset, range) {
  const src = chrome.runtime.getURL(`fonts/barlow-${weight}-${subset}.woff2`);
  return `
    @font-face {
      font-family: "Light Barlow"; font-style: normal; font-weight: ${weight}; font-display: swap;
      src: url("${src}") format("woff2");
      unicode-range: ${range};
    }`;
}

export function pageCss() {
  const HL = HIGHLIGHT_CLASS;
  return `
    ${fontFace(400, "latin", LATIN)}
    ${fontFace(400, "latin-ext", LATIN_EXT)}
    ${fontFace(500, "latin", LATIN)}
    ${fontFace(500, "latin-ext", LATIN_EXT)}
    .${HL}.light-has-note { text-decoration: underline dotted ${RULE}; text-decoration-thickness: 1px; text-underline-offset: 4px; }
    .${HL}.light-active { filter: brightness(0.94) saturate(1.3); }
    html.light-off .${HL} { text-decoration: none; filter: none; }
  `;
}

export const UI_CSS = `
  :host { all: initial; }
  .note {
    position: absolute; box-sizing: border-box;
    border-left: 1.5px solid ${RULE}; padding: 0 0 0 14px;
    font-family: ${FONT}; font-weight: 400; font-size: 14px; line-height: 1.45;
    letter-spacing: -0.1px; cursor: text;
  }
  .note .text { opacity: 0.6; transition: opacity 120ms ease; }
  .note.lit .text, .note.editing .text { opacity: 1; }
  .note.floating { padding: 8px 10px 8px 14px; }
  .text { outline: none; white-space: pre-wrap; overflow-wrap: break-word; min-height: 1.5em; }
  .text:empty::before { content: "Write a note"; opacity: 0.4; }
  .hint {
    display: none; margin-top: 10px;
    font-size: 12px; line-height: 1.4; opacity: 0.55; user-select: none;
  }
  .note.editing .hint { display: block; }
  .remove { all: unset; cursor: pointer; }
  .remove:hover { text-decoration: underline; }
  .remove.armed { color: ${RED}; opacity: 1; }
  .x {
    position: absolute; display: none; padding: 3px 5px;
    font-family: ${FONT}; font-weight: 500; line-height: 1;
    color: ${RED}; cursor: pointer; user-select: none;
    white-space: nowrap; border-radius: 3px;
  }
  .x.armed { font-size: 13px !important; }
`;
