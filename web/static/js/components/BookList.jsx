import React from 'react'
import BookRow from './BookRow.jsx'

export default function BookList({ books, entities, onBookUpdate, onEntitiesChange }) {
  if (!books.length) {
    return <p className="p-4 text-gray-500">No books found</p>
  }

  return (
    <table className="w-full text-left text-sm">
      <tbody>
        {books.map(book => (
          <BookRow
            key={book.id}
            book={book}
            entities={entities}
            onUpdate={onBookUpdate}
            onEntitiesChange={onEntitiesChange}
          />
        ))}
      </tbody>
    </table>
  )
}
