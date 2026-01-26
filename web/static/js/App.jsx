import React, { useState, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import HighlightsPage from './HighlightsPage.jsx'
import ResearchPage from './ResearchPage.jsx'

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

// Compute SHA-256 signature for a file
async function computeSignature(file) {
  const input = `${file.name}:${file.size}:${file.lastModified}`
  const encoder = new TextEncoder()
  const data = encoder.encode(input)
  const hashBuffer = await crypto.subtle.digest('SHA-256', data)
  const hashArray = Array.from(new Uint8Array(hashBuffer))
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('')
  return hashHex.substring(0, 16) // First 16 hex chars
}

// Extract PDF metadata using pdf.js
async function extractPdfMetadata(file) {
  try {
    const arrayBuffer = await file.arrayBuffer()
    const pdf = await pdfjsLib.getDocument({ data: arrayBuffer }).promise
    const metadata = await pdf.getMetadata()

    return {
      title: metadata.info?.Title || null,
      author: metadata.info?.Author || null,
      subject: metadata.info?.Subject || null,
      keywords: metadata.info?.Keywords || null,
    }
  } catch (e) {
    console.warn('Failed to extract PDF metadata:', e)
    return { title: null, author: null, subject: null, keywords: null }
  }
}

// Format bytes to human readable
function formatBytes(bytes) {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

function MassUploader({ onBookCreated }) {
  const [queue, setQueue] = useState([])
  const [isUploading, setIsUploading] = useState(false)
  const [loadingPending, setLoadingPending] = useState(true)
  const fileInputRef = useRef(null)
  const queueRef = useRef([])

  useEffect(() => {
    queueRef.current = queue
  }, [queue])

  // Fetch pending uploads on mount
  useEffect(() => {
    const fetchPending = async () => {
      try {
        const res = await fetch('/upload/pending')
        if (res.ok) {
          const data = await res.json()
          const pendingEntries = (data.uploads || []).map(u => ({
            id: u.file_signature,
            file_name: u.file_name,
            file_signature: u.file_signature,
            status: 'pending_server', // Pending on server, waiting for file
            bytes_uploaded: u.bytes_uploaded,
            completed_chunks: u.completed_chunks,
            upload_id: u.upload_id,
            file: null,
            key: u.key,
            chunk_size: null,
            total_chunks: null,
            progress: 0,
          }))
          if (pendingEntries.length > 0) {
            setQueue(pendingEntries)
          }
        }
      } catch (e) {
        console.error('Failed to fetch pending uploads:', e)
      } finally {
        setLoadingPending(false)
      }
    }
    fetchPending()
  }, [])

  const handleFiles = async (files) => {
    const pdfFiles = Array.from(files).filter(f =>
      f.type === 'application/pdf' || f.name.toLowerCase().endsWith('.pdf')
    )

    for (const file of pdfFiles) {
      const signature = await computeSignature(file)

      // Check if this file matches a pending upload from server
      const existingEntry = queueRef.current.find(
        e => e.file_signature === signature && e.status === 'pending_server'
      )

      if (existingEntry) {
        // Attach file to existing pending entry and extract metadata
        const metadata = await extractPdfMetadata(file)
        setQueue(prev => prev.map(e =>
          e.id === existingEntry.id
            ? {
                ...e,
                file,
                metadata,
                status: 'pending',
                progress: e.bytes_uploaded > 0 ? Math.round((e.bytes_uploaded / file.size) * 100) : 0
              }
            : e
        ))
      } else {
        // Check if we already have this file in queue
        if (queueRef.current.some(q => q.file_signature === signature)) {
          continue
        }

        // Extract metadata from PDF
        const metadata = await extractPdfMetadata(file)

        // Add new entry
        setQueue(prev => [...prev, {
          id: signature,
          file_name: file.name,
          file_signature: signature,
          status: 'pending',
          bytes_uploaded: 0,
          completed_chunks: 0,
          file,
          metadata,
          upload_id: null,
          key: null,
          chunk_size: null,
          total_chunks: null,
          progress: 0,
        }])
      }
    }
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

    if (!entry.file) {
      console.error('No file attached to entry')
      return
    }

    updateEntry({ status: 'uploading', progress: entry.progress || 0 })
    const file = entry.file

    try {
      // 1. Init (server detects if this is a resume)
      const initForm = new FormData()
      initForm.append('file_name', file.name)
      initForm.append('file_size', file.size)
      initForm.append('file_signature', entry.file_signature)

      const initRes = await fetch('/upload?state=init', { method: 'POST', body: initForm })
      if (!initRes.ok) throw new Error('Init request failed')
      const initData = await initRes.json()

      if (!initData.upload_id || !initData.key) {
        throw new Error('No upload_id or key returned')
      }

      const { upload_id, key, chunk_size, total_chunks, completed_chunks } = initData

      updateEntry({
        upload_id,
        key,
        chunk_size,
        total_chunks,
        progress: completed_chunks > 0 ? Math.round((completed_chunks / total_chunks) * 100) : 0
      })

      if (completed_chunks > 0) {
        console.log(`Resuming upload from chunk ${completed_chunks}/${total_chunks}`)
      }

      for (let i = completed_chunks; i < total_chunks; i++) {
        const start = i * chunk_size
        const end = Math.min(start + chunk_size, file.size)
        const chunk = file.slice(start, end)

        const chunkForm = new FormData()
        chunkForm.append('chunk', chunk)
        chunkForm.append('upload_id', upload_id)
        chunkForm.append('key', key)
        chunkForm.append('part_number', i + 1)

        const chunkRes = await fetch('/upload?state=continue', { method: 'POST', body: chunkForm })
        if (!chunkRes.ok) throw new Error('Chunk upload failed')

        updateEntry({
          progress: Math.round(((i + 1) / total_chunks) * 100),
          bytes_uploaded: end,
        })
      }

      // 3. Complete - send metadata with request
      const completeForm = new FormData()
      completeForm.append('upload_id', upload_id)
      completeForm.append('key', key)
      // Send extracted metadata
      if (entry.metadata) {
        completeForm.append('pdf_title', entry.metadata.title || '')
        completeForm.append('pdf_author', entry.metadata.author || '')
        completeForm.append('pdf_subject', entry.metadata.subject || '')
        completeForm.append('pdf_keywords', entry.metadata.keywords || '')
      }

      const completeRes = await fetch('/upload?state=complete', { method: 'POST', body: completeForm })
      if (!completeRes.ok) throw new Error('Complete request failed')

      const completeData = await completeRes.json()
      updateEntry({ status: 'completed', progress: 100 })

      if (completeData.books?.[0]) {
        onBookCreated?.(completeData.books[0])
      }
    } catch (err) {
      console.error('Upload failed:', err)
      updateEntry({ status: 'error' })
    }
  }

  const cancelUpload = async (entry) => {
    if (entry.upload_id && entry.key) {
      try {
        const form = new FormData()
        form.append('upload_id', entry.upload_id)
        form.append('key', entry.key)
        await fetch('/upload/abort', { method: 'POST', body: form })
      } catch (e) {
        console.error('Failed to abort upload:', e)
      }
    }
    setQueue(prev => prev.filter(e => e.id !== entry.id))
  }

  const startUpload = async () => {
    if (isUploading) return
    setIsUploading(true)

    // Reset error entries to pending (only those with files)
    setQueue(prev => prev.map(e =>
      e.status === 'error' && e.file ? { ...e, status: 'pending', progress: 0 } : e
    ))

    await new Promise(r => setTimeout(r, 0))

    const maxConcurrent = 3
    const processing = new Set()
    const processed = new Set()

    const processNext = async () => {
      const current = queueRef.current
      const next = current.find(e =>
        e.status === 'pending' && e.file && !processing.has(e.id) && !processed.has(e.id)
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

  const hasPendingWithFile = queue.some(e => (e.status === 'pending' || e.status === 'error') && e.file)
  const hasErrors = queue.some(e => e.status === 'error')
  const hasPendingServer = queue.some(e => e.status === 'pending_server')

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

      {loadingPending && (
        <div className="text-xs text-gray-500 mt-2">Loading pending uploads...</div>
      )}

      {queue.length > 0 && (
        <>
          <ul className="upload-queue">
            {queue.map(entry => (
              <li key={entry.id} className={`upload-item ${entry.status}`}>
                <div className="upload-item-header">
                  <span className="upload-item-name">{entry.file_name}</span>
                  <div className="upload-item-actions">
                    {entry.status === 'pending_server' && (
                      <span className="upload-item-hint text-xs text-amber-600">
                        {formatBytes(entry.bytes_uploaded)} - select file
                      </span>
                    )}
                    {entry.status !== 'completed' && (
                      <button
                        onClick={(e) => { e.stopPropagation(); cancelUpload(entry) }}
                        className="upload-cancel-btn text-gray-400 hover:text-red-500 ml-2"
                        title="Cancel"
                      >
                        ×
                      </button>
                    )}
                  </div>
                </div>
                <div className="upload-progress-track">
                  <div
                    className={`upload-progress-fill ${entry.status === 'pending' || entry.status === 'pending_server' ? 'pending' : ''}`}
                    style={{ width: `${entry.progress}%` }}
                  />
                </div>
                {entry.status === 'error' && (
                  <div className="text-xs text-red-500 mt-1">Upload failed</div>
                )}
              </li>
            ))}
          </ul>
          <div className="upload-controls">
            {hasPendingServer && !hasPendingWithFile && (
              <div className="text-xs text-amber-600 mb-2 text-center">
                Select files to resume incomplete uploads
              </div>
            )}
            <button
              onClick={startUpload}
              disabled={isUploading || !hasPendingWithFile}
              className={`start-upload-button w-full border border-gray-300 text-sm py-4 cursor-pointer bg-gray-100 hover:bg-gray-200 ${isUploading || !hasPendingWithFile ? 'disabled' : ''}`}
            >
              {isUploading ? 'uploading...' : 'Upload'}
            </button>
            {hasErrors && !isUploading && (
              <button
                onClick={startUpload}
                className="w-full border border-red-300 text-sm py-2 mt-2 cursor-pointer bg-red-50 hover:bg-red-100 text-red-700"
              >
                Retry Failed
              </button>
            )}
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
  const [dropdownStyle, setDropdownStyle] = useState(null)
  const containerRef = useRef(null)
  const dropdownRef = useRef(null)

  const updateDropdownPosition = () => {
    if (containerRef.current) {
      const rect = containerRef.current.getBoundingClientRect()
      setDropdownStyle({
        position: 'fixed',
        top: rect.bottom + 4,
        left: rect.left,
        width: rect.width,
        zIndex: 9999
      })
    }
  }

  const openDropdown = () => {
    updateDropdownPosition()
    setIsOpen(true)
  }

  useEffect(() => {
    const handleClickOutside = (e) => {
      if (containerRef.current && !containerRef.current.contains(e.target) &&
          dropdownRef.current && !dropdownRef.current.contains(e.target)) {
        setIsOpen(false)
        setDropdownStyle(null)
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

  const showDropdown = isOpen && dropdownStyle && (filtered.length > 0 || showCreateOption)

  return (
    <div ref={containerRef} className="relative">
      <div 
        className="border border-gray-300 px-1 min-h-[24px] flex flex-wrap gap-1 cursor-text items-center"
        onClick={openDropdown}
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
          onFocus={openDropdown}
          placeholder={selected.length ? '' : placeholder}
          className="flex-1 min-w-[60px] outline-none text-sm px-2 bg-transparent"
        />
      </div>
      {showDropdown && createPortal(
        <div 
          ref={dropdownRef}
          style={dropdownStyle}
          className="bg-white border border-gray-300 max-h-40 overflow-auto shadow-lg"
        >
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
        </div>,
        document.body
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

  const handleView = async () => {
    try {
      // Extract key from download_url (everything after the bucket domain)
      const url = new URL(book.download_url)
      const key = url.pathname.slice(1) // Remove leading slash
      const res = await fetch(`/download?key=${encodeURIComponent(key)}`)
      if (res.ok) {
        const data = await res.json()
        window.open(data.url, '_blank')
      } else {
        console.error('Failed to get download URL')
        // Fallback to direct URL
        window.open(book.download_url, '_blank')
      }
    } catch (e) {
      console.error('Failed to get download URL:', e)
      // Fallback to direct URL
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

function BookList({ books, entities, onBookUpdate, onEntitiesChange }) {
  if (!books.length) {
    return <p className="p-4 text-gray-500">No books found</p>
  }

  return (
    <table className="w-full text-left text-sm">
      {/* <thead>
        <tr className="border-b-2 border-gray-300">
          <th className="px-2 font-semibold">Title</th>
          <th className="px-2 font-semibold">Authors</th>
          <th className="px-2 font-semibold">Tags</th>
          <th className="px-2 font-semibold">Categories</th>
          <th className="px-2 font-semibold"></th>
        </tr>
      </thead> */}
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

function BooksPage() {
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

const VALID_PAGES = ['books', 'highlights', 'research']

function parseRoute() {
  const path = window.location.pathname.slice(1)
  const segments = path.split('/').filter(Boolean)

  if (segments.length === 0) {
    return { page: 'books', resourceId: null }
  }

  const page = VALID_PAGES.includes(segments[0]) ? segments[0] : 'books'
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
          className={route.page === 'books' ? 'active' : ''}
          onClick={() => navigateTo('books')}
        >
          Books {counts.books > 0 && `(${counts.books})`}
        </button>
        <button
          className={route.page === 'highlights' ? 'active' : ''}
          onClick={() => navigateTo('highlights')}
        >
          Highlights {counts.highlights > 0 && `(${counts.highlights})`}
        </button>
        <button
          className={route.page === 'research' ? 'active' : ''}
          onClick={() => navigateTo('research')}
        >
          Research {counts.research > 0 && `(${counts.research})`}
        </button>
      </nav>

      {route.page === 'books' && <BooksPage />}
      {route.page === 'highlights' && <HighlightsPage />}
      {route.page === 'research' && (
        <ResearchPage
          resourceId={route.resourceId}
          onNavigate={(id) => navigateTo('research', id)}
        />
      )}
    </div>
  )
}

