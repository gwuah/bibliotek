import React, { useState, useEffect } from 'react'
import MultiSelect from './MultiSelect.jsx'

function capitalizeTitle(title) {
  if (!title) return title
  return title
    .toLowerCase()
    .split(' ')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

function trimTitle(title, maxLength = 80) {
  if (!title) return title
  const capitalized = capitalizeTitle(title)
  if (capitalized.length <= maxLength) return capitalized
  return capitalized.substring(0, maxLength).trim() + '...'
}

export default function BookRow({ book, entities, onUpdate, onEntitiesChange }) {
  const [editing, setEditing] = useState(false)
  const [saving, setSaving] = useState(false)
  const [form, setForm] = useState({
    title: book.title,
    authors: [],
    tags: [],
    categories: []
  })

  useEffect(() => {
    if (editing) {
      setForm({
        title: book.title,
        authors: entities.authors.filter(a => book.author_ids.includes(String(a.id))),
        tags: entities.tags.filter(t => book.tag_ids.includes(String(t.id))),
        categories: entities.categories.filter(c => book.category_ids.includes(String(c.id)))
      })
    }
  }, [editing, book, entities])

  const hasChanges = () => {
    if (form.title !== book.title) return true
    const currentAuthorIds = form.authors.map(a => String(a.id)).sort()
    const currentTagIds = form.tags.map(t => String(t.id)).sort()
    const currentCatIds = form.categories.map(c => String(c.id)).sort()
    if (JSON.stringify(currentAuthorIds) !== JSON.stringify([...book.author_ids].sort())) return true
    if (JSON.stringify(currentTagIds) !== JSON.stringify([...book.tag_ids].sort())) return true
    if (JSON.stringify(currentCatIds) !== JSON.stringify([...book.category_ids].sort())) return true
    return false
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      const res = await fetch(`/books/${book.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          title: form.title,
          author_ids: form.authors.map(a => a.id),
          tag_ids: form.tags.map(t => t.id),
          category_ids: form.categories.map(c => c.id)
        })
      })
      if (res.ok) {
        const data = await res.json()
        if (data.books?.[0]) onUpdate(data.books[0])
        setEditing(false)
      }
    } finally {
      setSaving(false)
    }
  }

  const handleCancel = () => {
    setEditing(false)
    setForm({
      title: book.title,
      authors: entities.authors.filter(a => book.author_ids.includes(String(a.id))),
      tags: entities.tags.filter(t => book.tag_ids.includes(String(t.id))),
      categories: entities.categories.filter(c => book.category_ids.includes(String(c.id)))
    })
  }

  const createEntity = async (type, name) => {
    const res = await fetch(`/${type}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name })
    })
    if (res.ok) {
      const data = await res.json()
      onEntitiesChange(type, data.entity)
      return data.entity
    }
    return null
  }

  const handleView = async () => {
    try {
      const url = new URL(book.download_url)
      const key = url.pathname.slice(1)
      const res = await fetch(`/download?key=${encodeURIComponent(key)}`)
      if (res.ok) {
        const data = await res.json()
        window.open(data.url, '_blank')
      } else {
        console.error('Failed to get download URL')
        window.open(book.download_url, '_blank')
      }
    } catch (e) {
      console.error('Failed to get download URL:', e)
      window.open(book.download_url, '_blank')
    }
  }

  const bookAuthors = entities.authors.filter(a => book.author_ids.includes(String(a.id)))
  const bookTags = entities.tags.filter(t => book.tag_ids.includes(String(t.id)))
  const bookCategories = entities.categories.filter(c => book.category_ids.includes(String(c.id)))

  if (!editing) {
    return (
      <tr className="border-b border-gray-200">
        <td className=" px-2 font-medium" title={book.title}>{trimTitle(book.title)}</td>
        <td className="px-2">
          {bookAuthors.map(a => (
            <span key={a.id} className="bg-amber-100 px-2 py-0.5 text-xs rounded mr-1">{a.name}</span>
          ))}
        </td>
        <td className="px-2">
          {bookTags.map(t => (
            <span key={t.id} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{t.name}</span>
          ))}
        </td>
        <td className="px-2">
          {bookCategories.map(c => (
            <span key={c.id} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{c.name}</span>
          ))}
        </td>
        <td className="px-2">
          <button onClick={() => setEditing(true)} className="border border-gray-400 px-3 text-sm hover:bg-gray-100 mr-1">edit</button>
          <button
            onClick={handleView}
            className="border border-gray-400 px-3 text-sm hover:bg-gray-100 ml-1"
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
      <td className="px-2">
        <MultiSelect
          options={entities.authors}
          selected={form.authors}
          onChange={(v) => setForm(f => ({ ...f, authors: v }))}
          onCreate={(name) => createEntity('authors', name)}
          placeholder="authors..."
          entityType="authors"
        />
      </td>
      <td className="px-2">
        <MultiSelect
          options={entities.tags}
          selected={form.tags}
          onChange={(v) => setForm(f => ({ ...f, tags: v }))}
          onCreate={(name) => createEntity('tags', name)}
          placeholder="tags..."
          entityType="tags"
        />
      </td>
      <td className="px-2">
        <MultiSelect
          options={entities.categories}
          selected={form.categories}
          onChange={(v) => setForm(f => ({ ...f, categories: v }))}
          onCreate={(name) => createEntity('categories', name)}
          placeholder="categories..."
          entityType="categories"
        />
      </td>
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
