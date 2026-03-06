import React, { useState, useEffect, useRef, useCallback } from 'react'
import MassUploader from './MassUploader.jsx'
import BookList from './BookList.jsx'

const PAGE_SIZE = 50

export default function BooksPage() {
  const [books, setBooks] = useState([])
  const [entities, setEntities] = useState({ authors: [], tags: [], categories: [] })
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState('')
  const [page, setPage] = useState(1)
  const [totalBooks, setTotalBooks] = useState(0)
  const debounceRef = useRef(null)

  const totalPages = Math.max(1, Math.ceil(totalBooks / PAGE_SIZE))

  const fetchBooks = useCallback(async (query, pageNum) => {
    const params = new URLSearchParams()
    params.set('page', String(pageNum))
    params.set('limit', String(PAGE_SIZE))
    if (query) params.set('q', query)

    const res = await fetch(`/books?${params}`)
    if (res.ok) {
      const data = await res.json()
      setBooks(data.books || [])
      setTotalBooks(data.total_books ?? 0)
    }
  }, [])

  const loadData = async () => {
    try {
      const [_, metadataRes] = await Promise.all([
        fetchBooks(searchQuery, page),
        fetch('/metadata')
      ])
      if (metadataRes.ok) {
        const data = await metadataRes.json()
        const metadata = data.metadata || {}
        setEntities({
          authors: (metadata.authors || []).map(a => a.author),
          tags: (metadata.tags || []).map(t => t.tag),
          categories: (metadata.categories || []).map(c => c.category)
        })
      }
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadData()
  }, [])

  useEffect(() => {
    if (!loading) {
      fetchBooks(searchQuery, page)
    }
  }, [page])

  const handleSearchChange = (e) => {
    const value = e.target.value
    setSearchQuery(value)

    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      setPage(1)
      fetchBooks(value, 1)
    }, 300)
  }

  const handleBookUpdate = (updatedBook) => {
    setBooks(prev => prev.map(b => b.id === updatedBook.id ? updatedBook : b))
  }

  const handleEntitiesChange = (type, newEntity) => {
    setEntities(prev => ({
      ...prev,
      [type]: [...prev[type], newEntity]
    }))
  }

  const handleBookCreated = (book) => {
    if (page === 1 && !searchQuery) {
      setBooks(prev => [book, ...prev])
      setTotalBooks(prev => prev + 1)
    } else {
      setPage(1)
      setSearchQuery('')
      fetchBooks('', 1)
    }
  }

  if (loading) {
    return <div className="p-10">Loading...</div>
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-4">
        <div className="flex-1">
          <input
            type="text"
            value={searchQuery}
            onChange={handleSearchChange}
            placeholder="Search books..."
            className="search-input"
          />
        </div>
        <div className="pagination">
          <button
            onClick={() => setPage(p => p - 1)}
            disabled={page <= 1}
            className="pagination-btn"
          >
            ← prev
          </button>
          <span className="pagination-info">
            page {page} of {totalPages}
          </span>
          <button
            onClick={() => setPage(p => p + 1)}
            disabled={page >= totalPages}
            className="pagination-btn"
          >
            next →
          </button>
        </div>
      </div>

      <div className="flex flex-row gap-10" style={{ alignItems: 'flex-start' }}>
        <div className="w-[280px] flex-shrink-0">
          <MassUploader onBookCreated={handleBookCreated} />
        </div>
        <div className="flex-1 border border-gray-300 overflow-auto">
          <BookList
            books={books}
            entities={entities}
            onBookUpdate={handleBookUpdate}
            onEntitiesChange={handleEntitiesChange}
          />
        </div>
      </div>

    </div>
  )
}
