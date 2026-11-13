import React, { useState, useEffect } from 'react'
import HighlightsPage from './HighlightsPage.jsx'
import ResearchPage from './ResearchPage.jsx'
import DuplicatesPage from './DuplicatesPage.jsx'
import BooksPage from './components/BooksPage.jsx'
import { listDocuments, listMatches } from './api.js'
import { listReading } from './reading.js'
import { NAVIGATED, navigate } from './navigation.js'

const VALID_PAGES = ['library', 'highlights', 'reading', 'duplicates']

function parseRoute() {
  const segments = window.location.pathname.split('/').filter(Boolean)

  if (segments[0] === 'documents') {
    const documentId = parseInt(segments[1], 10)
    if (!isNaN(documentId)) return { page: 'reading', documentId }
  }

  const page = VALID_PAGES.includes(segments[0]) ? segments[0] : 'library'
  return { page, documentId: null }
}

export default function App() {
  const [route, setRoute] = useState(parseRoute)
  const [counts, setCounts] = useState({ documents: 0, highlights: 0, reading: 0, duplicates: 0 })

  useEffect(() => {
    const handlePopState = () => {
      setRoute(parseRoute())
    }
    window.addEventListener(NAVIGATED, handlePopState)
    return () => window.removeEventListener(NAVIGATED, handlePopState)
  }, [])

  useEffect(() => {
    const fetchCounts = async () => {
      try {
        const [documents, websites, reading, matches] = await Promise.all([
          listDocuments(),
          listDocuments({ kind: 'web' }),
          listReading(),
          listMatches()
        ])

        setCounts({
          documents: documents.total,
          highlights: websites.total,
          reading: reading.length,
          duplicates: matches.length
        })
      } catch (err) {
        console.error('Failed to fetch counts:', err)
      }
    }
    fetchCounts()
  }, [])

  const tab = (page, label, count) => (
    <button
      className={route.page === page ? 'active' : ''}
      onClick={() => navigate(`/${page}`)}
    >
      {label} {count > 0 && `(${count})`}
    </button>
  )

  return (
    <div className="p-10">
      <nav className="app-nav">
        {tab('library', 'Library', counts.documents)}
        {tab('highlights', 'Highlights', counts.highlights)}
        {tab('reading', 'Reading', counts.reading)}
        {tab('duplicates', 'Duplicates', counts.duplicates)}
      </nav>

      {route.page === 'library' && <BooksPage />}
      {route.page === 'highlights' && <HighlightsPage />}
      {route.page === 'reading' && <ResearchPage documentId={route.documentId} />}
      {route.page === 'duplicates' && <DuplicatesPage />}
    </div>
  )
}
