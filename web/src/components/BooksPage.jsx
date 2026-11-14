import React, { useState, useEffect, useRef, useCallback } from 'react'
import MassUploader from './MassUploader.jsx'
import BookList from './BookList.jsx'
import { getFacets, listDocuments } from '../api.js'
import { uploadRunner } from '../uploads/runner.js'

export default function BooksPage() {
  const [books, setBooks] = useState([])
  const [entities, setEntities] = useState({ authors: [], tags: [], subjects: [] })
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState('')
  const [page, setPage] = useState(1)
  const [totalPages, setTotalPages] = useState(1)
  const debounceRef = useRef(null)

  const fetchBooks = useCallback(async (query, pageNum) => {
    try {
      const data = await listDocuments({ q: query, page: pageNum, has_file: true })
      setBooks(data.documents)
      setTotalPages(Math.max(1, Math.ceil(data.total / data.page_size)))
    } catch (err) {
      console.error('Failed to fetch documents:', err)
    }
  }, [])

  const loadData = async () => {
    try {
      const [, facets] = await Promise.all([fetchBooks(searchQuery, page), getFacets()])
      setEntities({ authors: facets.authors, tags: facets.tags, subjects: facets.subjects })
    } catch (err) {
      console.error('Failed to fetch facets:', err)
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

  useEffect(() => uploadRunner.onCommitted(() => {
    setPage(1)
    setSearchQuery('')
    fetchBooks('', 1)
  }), [fetchBooks])

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

  if (loading) {
    return <div className="p-10">Loading...</div>
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col md:flex-row md:items-center gap-3 md:gap-4">
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

      <div className="flex flex-col md:flex-row gap-4 md:gap-10 md:items-start">
        <div className="md:w-[280px] md:flex-shrink-0">
          <MassUploader />
        </div>
        <div className="flex-1 min-w-0 border border-gray-300 overflow-auto">
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
