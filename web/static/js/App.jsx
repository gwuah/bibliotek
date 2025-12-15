import React, { useState, useEffect, useRef } from 'react'

function MassUploader({ onUploadComplete }) {
  const [queue, setQueue] = useState([])
  const [isUploading, setIsUploading] = useState(false)
  const fileInputRef = useRef(null)
  const queueRef = useRef([])

  useEffect(() => {
    queueRef.current = queue
  }, [queue])

  const handleFiles = (files) => {
    const pdfFiles = Array.from(files).filter(f => 
      f.type === 'application/pdf' || f.name.toLowerCase().endsWith('.pdf')
    )
    const newEntries = pdfFiles.filter(f => 
      !queue.some(q => q.signature === `${f.name}-${f.size}-${f.lastModified}`)
    ).map(f => ({
      id: crypto.randomUUID(),
      file: f,
      signature: `${f.name}-${f.size}-${f.lastModified}`,
      status: 'pending',
      progress: 0
    }))
    if (newEntries.length) setQueue(prev => [...prev, ...newEntries])
  }

  const handleDrop = (e) => {
    e.preventDefault()
    e.currentTarget.classList.remove('dragging')
    handleFiles(e.dataTransfer?.files || [])
  }

  const handleDragOver = (e) => {
    e.preventDefault()
    e.currentTarget.classList.add('dragging')
  }

  const handleDragLeave = (e) => {
    e.preventDefault()
    e.currentTarget.classList.remove('dragging')
  }

  const uploadFile = async (entry) => {
    const updateEntry = (updates) => {
      setQueue(prev => prev.map(e => e.id === entry.id ? { ...e, ...updates } : e))
    }

    updateEntry({ status: 'uploading', progress: 0 })
    const file = entry.file
    const chunkSize = 1024 * 1024

    try {
      const initForm = new FormData()
      initForm.append('file_name', file.name)
      const initRes = await fetch('/upload?state=init', { method: 'POST', body: initForm })
      if (!initRes.ok) throw new Error('Init request failed')
      const initData = await initRes.json()
      if (!initData.upload_id) throw new Error('No upload_id')

      const totalChunks = Math.max(1, Math.ceil(file.size / chunkSize))
      for (let i = 0; i < totalChunks; i++) {
        const chunk = file.slice(i * chunkSize, (i + 1) * chunkSize)
        const chunkForm = new FormData()
        chunkForm.append('chunk', chunk)
        chunkForm.append('upload_id', initData.upload_id)
        chunkForm.append('part_number', i + 1)
        const chunkRes = await fetch('/upload?state=continue', { method: 'POST', body: chunkForm })
        if (!chunkRes.ok) throw new Error('Chunk upload failed')
        updateEntry({ progress: Math.round(((i + 1) / totalChunks) * 100) })
      }

      const completeForm = new FormData()
      completeForm.append('upload_id', initData.upload_id)
      const completeRes = await fetch('/upload?state=complete', { method: 'POST', body: completeForm })
      if (!completeRes.ok) throw new Error('Complete request failed')
      updateEntry({ status: 'completed', progress: 100 })
      onUploadComplete?.()
    } catch (err) {
      updateEntry({ status: 'error' })
    }
  }

  const startUpload = async () => {
    if (isUploading) return
    setIsUploading(true)

    setQueue(prev => prev.map(e => 
      e.status === 'error' ? { ...e, status: 'pending', progress: 0 } : e
    ))

    await new Promise(r => setTimeout(r, 0))

    const maxConcurrent = 3
    const processing = new Set()
    const processed = new Set()

    const processNext = async () => {
      const current = queueRef.current
      const next = current.find(e => 
        e.status === 'pending' && !processing.has(e.id) && !processed.has(e.id)
      )

      if (!next || processing.size >= maxConcurrent) return

      processing.add(next.id)
      processed.add(next.id)
      await uploadFile(next)
      processing.delete(next.id)

      await processNext()
    }

    await Promise.all(Array(maxConcurrent).fill(null).map(() => processNext()))
    setIsUploading(false)
  }

  const hasPending = queue.some(e => e.status === 'pending' || e.status === 'error')
  const completed = queue.filter(e => e.status === 'completed').length

  return (
    <div className="uploader-panel">
      <div 
        className="file-drop-area"
        onClick={() => fileInputRef.current?.click()}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
      >
        <input
          ref={fileInputRef}
          type="file"
          className="hidden"
          multiple
          accept="application/pdf"
          onChange={(e) => handleFiles(e.target.files)}
        />
        <div className="file-drop-content">
          <svg className="w-5 h-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 20 16">
            <path stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 13h3a3 3 0 0 0 0-6h-.025A5.56 5.56 0 0 0 16 6.5 5.5 5.5 0 0 0 5.207 5.021C5.137 5.017 5.071 5 5 5a4 4 0 0 0 0 8h2.167M10 15V6m0 0L8 8m2-2 2 2"/>
          </svg>
          <div className="file-drop-text">
            <p className="text-sm font-bold">drop or click to add pdfs</p>
          </div>
        </div>
      </div>

      {queue.length > 0 && (
        <>
          <ul className="upload-queue">
            {queue.map(entry => (
              <li key={entry.id} className={`upload-item ${entry.status}`}>
                <div className="upload-item-header">
                  <span className="upload-item-name">{entry.file.name}</span>
                  <span className="upload-item-status">{entry.status}</span>
                </div>
                <div className="upload-progress-track">
                  <div 
                    className={`upload-progress-fill ${entry.status === 'pending' ? 'pending' : ''}`}
                    style={{ width: `${entry.progress}%` }}
                  />
                </div>
              </li>
            ))}
          </ul>
          <div className="upload-controls">
            <button
              onClick={startUpload}
              disabled={isUploading || !hasPending}
              className={`start-upload-button w-full border border-gray-300 text-sm py-4 cursor-pointer bg-gray-100 hover:bg-gray-200 ${isUploading || !hasPending ? 'disabled' : ''}`}
            >
              {isUploading ? 'uploading...' : 'Upload'}
            </button>
            <span className="upload-summary">{completed}/{queue.length} completed</span>
          </div>
        </>
      )}
    </div>
  )
}

function MultiSelect({ options, selected, onChange, onCreate, placeholder, entityType }) {
  const [isOpen, setIsOpen] = useState(false)
  const [search, setSearch] = useState('')
  const [creating, setCreating] = useState(false)
  const containerRef = useRef(null)

  useEffect(() => {
    const handleClickOutside = (e) => {
      if (containerRef.current && !containerRef.current.contains(e.target)) {
        setIsOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  const filtered = options.filter(o => 
    o.name.toLowerCase().includes(search.toLowerCase()) && 
    !selected.some(s => s.id === o.id)
  )

  const handleCreate = async () => {
    if (!search.trim() || creating) return
    setCreating(true)
    const created = await onCreate(search.trim())
    if (created) {
      onChange([...selected, created])
      setSearch('')
    }
    setCreating(false)
  }

  const showCreateOption = search.trim() && 
    !options.some(o => o.name.toLowerCase() === search.toLowerCase()) &&
    !selected.some(s => s.name.toLowerCase() === search.toLowerCase())

  return (
    <div ref={containerRef} className="relative">
      <div 
        className="border border-gray-300 p-1 min-h-[32px] flex flex-wrap gap-1 cursor-text"
        onClick={() => setIsOpen(true)}
      >
        {selected.map(s => (
          <span key={s.id} className="bg-amber-100 px-2 py-0.5 text-xs rounded flex items-center gap-1">
            {s.name}
            <button onClick={(e) => { e.stopPropagation(); onChange(selected.filter(x => x.id !== s.id)) }} className="hover:text-red-500">×</button>
          </span>
        ))}
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onFocus={() => setIsOpen(true)}
          placeholder={selected.length ? '' : placeholder}
          className="flex-1 min-w-[60px] outline-none text-sm bg-transparent"
        />
      </div>
      {isOpen && (filtered.length > 0 || showCreateOption) && (
        <div className="absolute z-10 w-full bg-white border border-gray-300 mt-1 max-h-40 overflow-auto shadow-lg">
          {filtered.map(o => (
            <div
              key={o.id}
              onClick={() => { onChange([...selected, o]); setSearch('') }}
              className="px-2 py-1 text-sm cursor-pointer hover:bg-gray-100"
            >
              {o.name}
            </div>
          ))}
          {showCreateOption && (
            <div
              onClick={handleCreate}
              className="px-2 py-1 text-sm cursor-pointer hover:bg-green-100 text-green-700 border-t"
            >
              {creating ? 'Creating...' : `Create "${search}"`}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

function BookRow({ book, entities, onUpdate, onEntitiesChange }) {
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

  const bookAuthors = entities.authors.filter(a => book.author_ids.includes(String(a.id)))
  const bookTags = entities.tags.filter(t => book.tag_ids.includes(String(t.id)))
  const bookCategories = entities.categories.filter(c => book.category_ids.includes(String(c.id)))

  if (!editing) {
    return (
      <tr className="border-b border-gray-200">
        <td className="py-2 px-2 font-medium">{book.title}</td>
        <td className="py-2 px-2">
          {bookAuthors.map(a => (
            <span key={a.id} className="bg-amber-100 px-2 py-0.5 text-xs rounded mr-1">{a.name}</span>
          ))}
        </td>
        <td className="py-2 px-2">
          {bookTags.map(t => (
            <span key={t.id} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{t.name}</span>
          ))}
        </td>
        <td className="py-2 px-2">
          {bookCategories.map(c => (
            <span key={c.id} className="border border-gray-400 px-2 py-0.5 text-xs rounded-full mr-1">{c.name}</span>
          ))}
        </td>
        <td className="py-2 px-2">
          <button onClick={() => setEditing(true)} className="border border-gray-400 px-3 py-1 text-sm hover:bg-gray-100">edit</button>
        </td>
      </tr>
    )
  }

  return (
    <tr className="border-b border-gray-200 bg-gray-50">
      <td className="py-2 px-2">
        <input
          value={form.title}
          onChange={(e) => setForm(f => ({ ...f, title: e.target.value }))}
          className="w-full border border-gray-300 px-2 py-1 text-sm"
        />
      </td>
      <td className="py-2 px-2">
        <MultiSelect
          options={entities.authors}
          selected={form.authors}
          onChange={(v) => setForm(f => ({ ...f, authors: v }))}
          onCreate={(name) => createEntity('authors', name)}
          placeholder="authors..."
          entityType="authors"
        />
      </td>
      <td className="py-2 px-2">
        <MultiSelect
          options={entities.tags}
          selected={form.tags}
          onChange={(v) => setForm(f => ({ ...f, tags: v }))}
          onCreate={(name) => createEntity('tags', name)}
          placeholder="tags..."
          entityType="tags"
        />
      </td>
      <td className="py-2 px-2">
        <MultiSelect
          options={entities.categories}
          selected={form.categories}
          onChange={(v) => setForm(f => ({ ...f, categories: v }))}
          onCreate={(name) => createEntity('categories', name)}
          placeholder="categories..."
          entityType="categories"
        />
      </td>
      <td className="py-2 px-2 whitespace-nowrap">
        <button onClick={handleCancel} className="border border-gray-400 px-3 py-1 text-sm hover:bg-gray-100 mr-1">cancel</button>
        <button 
          onClick={handleSave} 
          disabled={!hasChanges() || saving}
          className={`border border-gray-400 px-3 py-1 text-sm ${hasChanges() && !saving ? 'bg-green-100 hover:bg-green-200' : 'opacity-50 cursor-not-allowed'}`}
        >
          {saving ? 'saving...' : 'save'}
        </button>
      </td>
    </tr>
  )
}

function BookList({ books, entities, onBookUpdate, onEntitiesChange }) {
  if (!books.length) {
    return <p className="p-4 text-gray-500">No books found</p>
  }

  return (
    <table className="w-full text-left text-sm">
      <thead>
        <tr className="border-b-2 border-gray-300">
          <th className="py-2 px-2 font-semibold">Title</th>
          <th className="py-2 px-2 font-semibold">Authors</th>
          <th className="py-2 px-2 font-semibold">Tags</th>
          <th className="py-2 px-2 font-semibold">Categories</th>
          <th className="py-2 px-2 font-semibold"></th>
        </tr>
      </thead>
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

export default function App() {
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
    <div className="p-10">
      <div className="flex flex-row gap-10">
        <div className="w-[280px] flex-shrink-0">
          <MassUploader onUploadComplete={loadData} />
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

