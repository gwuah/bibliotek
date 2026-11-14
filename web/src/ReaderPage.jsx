import React, { useState } from 'react'
import { shelfOf } from './shelf.js'

function edition(book) {
  return [book.translator && `tr. ${book.translator}`, book.year].filter(Boolean).join(', ')
}

function ReaderBook({ book }) {
  return (
    <li className="reader-book">
      <a href={book.url} className="reader-book-title">{book.title}</a>
      <span className="reader-book-edition">{edition(book)}</span>
      {book.highlights > 0 && (
        <span className="reader-book-highlights">
          {book.highlights} {book.highlights === 1 ? 'highlight' : 'highlights'}
        </span>
      )}
    </li>
  )
}

export default function ReaderPage({ books }) {
  const [query, setQuery] = useState('')

  if (books === undefined) return <p className="light-loading">Loading books…</p>
  if (books === null) return <p className="light-empty">The reader is not set up on this liotek.</p>

  const shelves = shelfOf(books, query)

  return (
    <div className="flex flex-col gap-4">
      <input
        type="text"
        className="search-input"
        placeholder="Search by title, author or translator..."
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {shelves.length === 0 && <p className="light-empty">No books found</p>}
      {shelves.map((shelf) => (
        <section key={shelf.author} className="reader-shelf">
          <h2 className="reader-shelf-author">{shelf.author}</h2>
          <ul>
            {shelf.books.map((book) => <ReaderBook key={book.slug} book={book} />)}
          </ul>
        </section>
      ))}
    </div>
  )
}
