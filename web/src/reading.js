// Bibliotek listed PDFs synced from Research; the listing cannot filter by source, so
// annotated PDFs stand in for them.

import { listAllDocuments } from './api.js'

const isAnnotated = (document) => document.highlight_count > 0 || document.note_count > 0

export const listReading = async () => (await listAllDocuments({ kind: 'pdf' })).filter(isAnnotated)
