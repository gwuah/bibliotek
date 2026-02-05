import React, { useState, useEffect } from 'react'

export default function ChapterEditor({ config, onSave, onCancel, saving }) {
  const [json, setJson] = useState('')
  const [pageOffset, setPageOffset] = useState(0)
  const [error, setError] = useState(null)

  useEffect(() => {
    const chapters = config?.chapters || {}
    setJson(JSON.stringify(chapters, null, 2))
    setPageOffset(config?.page_offset || 0)
  }, [config])

  const handleSave = () => {
    setError(null)
    try {
      const parsed = JSON.parse(json)
      for (const [key, value] of Object.entries(parsed)) {
        if (!Array.isArray(value) || value.length !== 3) {
          throw new Error(`Chapter "${key}" must be [title, startPage, endPage]`)
        }
        if (typeof value[0] !== 'string') {
          throw new Error(`Chapter "${key}" title must be a string`)
        }
        if (typeof value[1] !== 'number' || typeof value[2] !== 'number') {
          throw new Error(`Chapter "${key}" pages must be numbers`)
        }
      }
      onSave({ chapters: parsed, page_offset: pageOffset })
    } catch (err) {
      setError(err.message)
    }
  }

  return (
    <div className="research-chapter-editor">
      <h4>Chapter Config</h4>
      <label className="research-chapter-offset-label">
        <span>Page offset (physical - logical):</span>
        <input
          type="number"
          value={pageOffset}
          onChange={(e) => setPageOffset(parseInt(e.target.value) || 0)}
          className="research-chapter-offset-input"
        />
      </label>
      <p className="research-chapter-help">
        Chapters: {"{"}"1": ["Title", startPage, endPage], ...{"}"}
      </p>
      <textarea
        value={json}
        onChange={(e) => setJson(e.target.value)}
        className="research-chapter-textarea"
        rows={10}
        spellCheck={false}
      />
      {error && <p className="research-error">{error}</p>}
      <div className="research-chapter-actions">
        <button onClick={onCancel} className="research-btn research-btn-secondary">
          Cancel
        </button>
        <button
          onClick={handleSave}
          disabled={saving}
          className="research-btn research-btn-primary"
        >
          {saving ? 'Saving...' : 'Save'}
        </button>
      </div>
    </div>
  )
}
