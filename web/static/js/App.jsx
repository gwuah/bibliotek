import React, { useState, useEffect } from 'react'
import HighlightsPage from './HighlightsPage.jsx'
import ResearchPage from './ResearchPage.jsx'
import BooksPage from './components/BooksPage.jsx'

const VALID_PAGES = ['library', 'highlights', 'reading']

function parseRoute() {
  const path = window.location.pathname.slice(1)
  const segments = path.split('/').filter(Boolean)

  if (segments.length === 0) {
    return { page: 'library', resourceId: null }
  }

  const page = VALID_PAGES.includes(segments[0]) ? segments[0] : 'library'
  const resourceId = segments[1] ? parseInt(segments[1], 10) : null

  return { page, resourceId: isNaN(resourceId) ? null : resourceId }
}

export default function App() {
  const [route, setRoute] = useState(parseRoute)
  const [counts, setCounts] = useState({ books: 0, highlights: 0, research: 0 })

  useEffect(() => {
    const handlePopState = () => {
      setRoute(parseRoute())
    }
    window.addEventListener('popstate', handlePopState)
    return () => window.removeEventListener('popstate', handlePopState)
  }, [])

  useEffect(() => {
    const fetchCounts = async () => {
      try {
        const [booksRes, highlightsRes, researchRes] = await Promise.all([
          fetch('/books'),
          fetch('/commonplace/resources?limit=1000&type=website'),
          fetch('/commonplace/resources?limit=1000&type=pdf')
        ])

        const booksData = booksRes.ok ? await booksRes.json() : { books: [] }
        const highlightsData = highlightsRes.ok ? await highlightsRes.json() : { data: [] }
        const researchData = researchRes.ok ? await researchRes.json() : { data: [] }

        setCounts({
          books: booksData.books?.length || 0,
          highlights: highlightsData.data?.length || 0,
          research: researchData.data?.length || 0
        })
      } catch (err) {
        console.error('Failed to fetch counts:', err)
      }
    }
    fetchCounts()
  }, [])

  const navigateTo = (page, resourceId = null) => {
    const newPath = resourceId ? `/${page}/${resourceId}` : `/${page}`
    if (window.location.pathname !== newPath) {
      window.history.pushState({}, '', newPath)
      setRoute({ page, resourceId })
    }
  }

  return (
    <div className="p-10">
      <nav className="app-nav">
        <button
          className={route.page === 'library' ? 'active' : ''}
          onClick={() => navigateTo('library')}
        >
          Library {counts.books > 0 && `(${counts.books})`}
        </button>
        <button
          className={route.page === 'highlights' ? 'active' : ''}
          onClick={() => navigateTo('highlights')}
        >
          Highlights {counts.highlights > 0 && `(${counts.highlights})`}
        </button>
        <button
          className={route.page === 'reading' ? 'active' : ''}
          onClick={() => navigateTo('reading')}
        >
          Reading {counts.research > 0 && `(${counts.research})`}
        </button>
      </nav>

      {route.page === 'library' && <BooksPage />}
      {route.page === 'highlights' && <HighlightsPage />}
      {route.page === 'reading' && (
        <ResearchPage
          resourceId={route.resourceId}
          onNavigate={(id) => navigateTo('reading', id)}
        />
      )}
    </div>
  )
}
