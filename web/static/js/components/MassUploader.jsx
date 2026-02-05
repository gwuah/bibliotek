import React, { useState, useEffect, useRef } from 'react'

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

export default function MassUploader({ onBookCreated }) {
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
            <path stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 13h3a3 3 0 0 0 0-6h-.025A5.56 5.56 0 0 0 16 6.5 5.5 5.5 0 0 0 5.207 5.021C5.137 5.017 5.071 5 5 5a4 4 0 0 0 0 8h2.167M10 15V6m0 0L8 8m2-2 2 2" />
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
