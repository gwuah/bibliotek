import React from 'react'
import BookRow from './BookRow.jsx'

export default function BookList({ books, entities, onBookUpdate, onEntitiesChange }) {
  if (!books.length) {
    return <p className="p-4 text-gray-500">No books found</p>
  }

  return (
    <table className="w-full table-fixed text-left text-sm">
      <colgroup>
        <col style={{ width: '33%' }} />
        <col style={{ width: '17%' }} />
        <col style={{ width: '17%' }} />
        <col style={{ width: '17%' }} />
        <col style={{ width: '16%' }} />
      </colgroup>
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
