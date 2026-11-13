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

## Moving over from the old Light

A new extension starts with empty storage. Carry each old installation over with its
installation ID, so liotek matches the highlights it already has (for example from the
Bibliotek import) instead of storing them twice.

1. In the browser with the old Light, right-click its toolbar icon › **Inspect popup**,
   open the **Console**, and run:

   ```js
   chrome.storage.local.get({ highlights: {}, sourceId: null }, ({ highlights, sourceId }) => {
     if (!sourceId) return console.error("This installation never generated a sourceId.");
     const file = {
       format: "liotek-light-export",
       version: 1,
       installation: sourceId,
       exported_at: new Date().toISOString(),
       highlights,
     };
     const link = document.createElement("a");
     link.href = URL.createObjectURL(new Blob([JSON.stringify(file, null, 2)], { type: "application/json" }));
     link.download = `light-${sourceId}.json`;
     link.click();
   });
   ```

   It only reads the old extension's storage (`highlights` and `sourceId`) and downloads
   `light-<id>.json`.
2. Install Light for liotek in the same browser, open Settings › **Import from file**, and
   choose that file. The extension adopts the old installation ID.
3. Sync. Every highlight should come back `unchanged` or `updated`, not `new`.
4. Once that sync looks right, remove the old extension.

One copy of Light for liotek holds one installation. Importing into a copy that already
has highlights works only for a file from the same installation (the two are merged);
a file from a different installation is refused, because one ID cannot speak for two sets
of highlights. With several old copies, import each into the new extension in the browser
or profile where that old copy lived.

**Export to file** in Settings writes the same format, for backups or moving to another
browser.

## Layout

```text
manifest.json
background.js         context menu; the one writer of the library (a queue of commands)
lib/
  storage.js          the only code touching chrome.storage; every key is `liotek:…`
  library.js          the highlights object and every change to it, as pure functions
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
tests/                node --test, for the pure modules and the background worker
```

There is no build step. `npm test` (or `node --test tests/*.test.js`) runs the tests;
`package.json` exists only so Node reads the sources as ES modules.
