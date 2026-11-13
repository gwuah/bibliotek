import React, { useState, useEffect } from 'react'
import MultiSelect from './MultiSelect.jsx'
import DocumentLink from './DocumentLink.jsx'
import { documentFileUrl, updateDocument } from '../api.js'
import { metadataChanges, metadataOf } from '../metadata.js'

function capitalizeTitle(title) {
  if (!title) return title
  return title
    .toLowerCase()
    .split(' ')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

function trimTitle(book, maxLength = 80) {
  const title = book.kind === 'web' ? book.title : capitalizeTitle(book.title)
  if (!title || title.length <= maxLength) return title
  return title.substring(0, maxLength).trim() + '...'
}

const newOption = (name) => ({ id: `new:${name}`, name })
const optionsFor = (names, options) => names.map(name => options.find(o => o.name === name) ?? newOption(name))
const namesOf = (options) => options.map(o => o.name)

const LABELS = [
  { field: 'authors', placeholder: 'authors...' },
  { field: 'tags', placeholder: 'tags...' },
  { field: 'subjects', placeholder: 'subjects...' }
]

export default function BookRow({ book, entities, onUpdate, onEntitiesChange }) {
  const [editing, setEditing] = useState(false)
  const [saving, setSaving] = useState(false)
  const [form, setForm] = useState({ title: book.title, authors: [], tags: [], subjects: [] })

  const formFor = () => ({
    title: book.title,
    authors: optionsFor(book.authors, entities.authors),
    tags: optionsFor(book.tags, entities.tags),
    subjects: optionsFor(book.subjects, entities.subjects)
  })

  useEffect(() => {
    if (editing) {
      setForm(formFor())
    }
  }, [editing, book, entities])

  const changes = () => {
    const before = metadataOf(book)
    return metadataChanges(before, {
      ...before,
      title: form.title,
      authors: namesOf(form.authors),
      tags: namesOf(form.tags),
      subjects: namesOf(form.subjects)
    })
  }
  const hasChanges = () => Object.keys(changes()).length > 0

  const handleSave = async () => {
    setSaving(true)
    try {
      const document = await updateDocument(book.id, changes())
      onUpdate({ ...book, ...metadataOf(document) })
      setEditing(false)
    } catch (err) {
      console.error('Failed to save document:', err)
    } finally {
      setSaving(false)
    }
  }

  const handleCancel = () => {
    setEditing(false)
    setForm(formFor())
  }

  const createEntity = async (type, name) => {
    const entity = newOption(name)
    onEntitiesChange(type, entity)
    return entity
  }

  const handleView = () => {
    window.open(documentFileUrl(book.id), '_blank')
  }

  if (!editing) {
    return (
      <tr className="border-b border-gray-200">
        <td className="px-2 font-medium" title={book.title}>
          <DocumentLink id={book.id} className="book-title-link">{trimTitle(book)}</DocumentLink>
        </td>
        <td className="px-2">
          {book.authors.map(name => (
            <span key={name} className="bg-amber-100 px-2 py-0.5 text-xs rounded mr-1">{name}</span>
          ))}
        </td>
        <td className="px-2">
          {book.tags.map(name => (
            <span key={name} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{name}</span>
          ))}
        </td>
        <td className="px-2">
          {book.subjects.map(name => (
            <span key={name} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{name}</span>
          ))}
        </td>
        <td className="px-2 whitespace-nowrap">
          <button onClick={() => setEditing(true)} className="border border-gray-400 px-3 text-sm hover:bg-gray-100 mr-1">edit</button>
          <button
            onClick={handleView}
            disabled={!book.has_file}
            title={book.has_file ? undefined : 'no file is stored for this document'}
            className={`border border-gray-400 px-3 text-sm ml-1 ${book.has_file ? 'hover:bg-gray-100' : 'opacity-50 cursor-not-allowed'}`}
          >
            view
          </button>
        </td>
      </tr>
    )
  }

  return (
    <tr className="border-b border-gray-200 bg-gray-50">
      <td className="px-2">
        <input
          value={form.title}
          onChange={(e) => setForm(f => ({ ...f, title: e.target.value }))}
          className="w-full border border-gray-300 px-2 text-sm"
          placeholder="title..."
        />
      </td>
      {LABELS.map(({ field, placeholder }) => (
        <td key={field} className="px-2">
          <MultiSelect
            options={entities[field]}
            selected={form[field]}
            onChange={(v) => setForm(f => ({ ...f, [field]: v }))}
            onCreate={(name) => createEntity(field, name)}
            placeholder={placeholder}
          />
        </td>
      ))}
      <td className="px-2 whitespace-nowrap">
        <button onClick={handleCancel} className="border border-gray-400 px-3 text-sm hover:bg-gray-100 mr-1">cancel</button>
        <button
          onClick={handleSave}
          disabled={!hasChanges() || saving}
          className={`border border-gray-400 px-3 text-sm ${hasChanges() && !saving ? 'bg-green-100 hover:bg-green-200' : 'opacity-50 cursor-not-allowed'}`}
        >
          {saving ? 'saving...' : 'save'}
        </button>
      </td>
    </tr>
  )
}
