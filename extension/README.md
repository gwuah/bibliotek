# Light for liotek

The Light highlighter, syncing to liotek. Select text on any page and press Ctrl+A (or
right-click › Highlight Text), write a note in the margin, and press Sync in the popup to
send every highlight to your liotek library.

It is a separate extension from the old Light, with its own name and storage, so the two
can be installed side by side.

## Install

1. Open `chrome://extensions` and turn on Developer mode.
2. Load unpacked › choose this `extension/` directory.
3. Open the extension's Settings (the gear in the popup):
   - **Server URL**: `http://127.0.0.1:5678` by default. Saving asks Chrome for access to
     that host, and only that host.
   - **Admin token**: the daemon's `admin_token`.
   - Copy the **extension origin** shown there (`chrome-extension://<id>`) into
     `extension_origins` in liotek's config file and restart the daemon.

## Sync

Sync sends this installation's complete library to
`PUT /api/imports/light/{installation}` and shows what liotek changed. liotek removes
whatever this installation sent before and no longer holds, so a deletion here reaches
liotek on the next sync. If every highlight here has been deleted, liotek refuses the
empty snapshot; the popup asks before sending it again with `confirm_empty=true`.

A sync only reads local storage. Whatever the server answers, the highlights here stay as
they were.

## Page protocol

Any page can talk to Light through two small, public hooks. Nothing in the extension is
specific to the pages that use them.

- **`light:url`** declares the address a page's highlights are saved under, whatever
  address the page was opened at:

  ```html
  <meta
    name="light:url"
    content="http://liotek.test/reader/croce-logic/book/book.html"
  />
  ```

  A page that declares one shows exactly the highlights saved under it. A page without one
  is saved under its own address, without its fragment, and shows only the highlights saved
  there. `http` and `https`, and a leading `www.`, count as the same page; a different query
  string is a different page. Single-page apps that change the address in place are followed.

- **`light:repaint`** asks Light to paint the page's highlights again, after the page has
  rewritten its own text (and with it the highlight marks):

  ```js
  document.dispatchEvent(new Event("light:repaint"));
  ```

  Several within one frame cause one repaint. The event carries no data.

A highlight's address never keeps a place in the page (`#section-2`), so one page stays one
liotek document. A fragment that is a route (`#/inbox`, `#!/thread`) is kept.

## Layout

```text
manifest.json
background.js         context menu; the one writer of the library (a queue of commands)
lib/
  storage.js          the only code touching chrome.storage; every key is `liotek:…`
  library.js          the highlights object and every change to it, as pure functions
  page.js             the address a highlight is saved under
  commands.js         asks the background to change the library
  sync.js             the sync request, and the server's answer or error
  transfer.js         the export file: building, validating, and what importing does
  installation.js     installation IDs (bare, or the old `light_` spelling)
  settings.js         the server URL and the host permission it needs
content/
  loader.js           a classic script that loads main.js as a module
  main.js             makes highlights, paints them back, answers the popup
  highlighter.js      selection capture and painting text ranges
  notes.js            margin notes and the delete button, in a shadow root
  notes-style.js      their CSS and bundled fonts
popup/                the list of highlights, toggle, sync
options/              settings, installation identity, export and import
fonts/                Barlow, bundled so nothing loads from the network
scripts/
  import-reader-highlights.js   the one-off move of iconoclast reader highlights
tests/                node --test, for the pure modules and the background worker
```

There is no build step. `npm test` (or `node --test tests/*.test.js`) runs the tests;
`package.json` exists only so Node reads the sources as ES modules.
