import React, { useState, useEffect } from 'react'
import MassUploader from './MassUploader.jsx'
import BookList from './BookList.jsx'

export default function BooksPage() {
  const [books, setBooks] = useState([])
  const [entities, setEntities] = useState({ authors: [], tags: [], categories: [] })
  const [loading, setLoading] = useState(true)

  const loadData = async () => {
    try {
      const [booksRes, metadataRes] = await Promise.all([
        fetch('/books'),
        fetch('/metadata')
      ])
      if (booksRes.ok) {
        const data = await booksRes.json()
        setBooks(data.books || [])
      }
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
    <div className="flex flex-row gap-10" style={{ alignItems: 'flex-start' }}>
      <div className="w-[280px] flex-shrink-0">
        <MassUploader onBookCreated={(book) => setBooks(prev => [book, ...prev])} />
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
  )
}
