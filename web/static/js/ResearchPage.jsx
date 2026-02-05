import React, { useState, useEffect } from 'react'

function formatDate(dateString) {
  if (!dateString) return ''
  const date = new Date(dateString)
  const day = String(date.getDate()).padStart(2, '0')
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const year = String(date.getFullYear()).slice(-2)
  return `${day}/${month}/${year}`
}

function trimTitle(title, maxLength = 60) {
  if (!title) return title
  if (title.length <= maxLength) return title
  return title.substring(0, maxLength).trim() + '...'
}

function ConfigPanel({ config, onConfigChange, onSync, syncing }) {
  const [dbPath, setDbPath] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (config?.db_path) {
      setDbPath(config.db_path)
    }
  }, [config])

  const handleSave = async () => {
    if (!dbPath.trim()) return
    
    setSaving(true)
    setError(null)
    
    try {
      const res = await fetch('/research/config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ db_path: dbPath.trim() })
      })
      
      if (!res.ok) {
        const data = await res.json()
        throw new Error(data.error || 'Failed to save configuration')
      }
      
      const data = await res.json()
      onConfigChange(data.data)
    } catch (err) {
      setError(err.message)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="research-config-panel">
      {/* <h3>Research Database</h3> */}
      
      <div className="research-config-form">
        <label>
          <span>Database Path:</span>
          <input
            type="text"
            value={dbPath}
            onChange={(e) => setDbPath(e.target.value)}
            placeholder="/path/to/data.db"
            className="research-input"
          />
        </label>
        
        <div className="research-config-actions">
          <button
            onClick={handleSave}
            disabled={saving || !dbPath.trim()}
            className="research-btn research-btn-secondary"
          >
            {saving ? 'Saving...' : 'Save Path'}
          </button>
          
          <button
            onClick={onSync}
            disabled={syncing || !config?.db_path}
            className="research-btn research-btn-primary"
          >
            {syncing ? 'Syncing...' : 'Sync Now'}
          </button>
        </div>
        
        {error && <p className="research-error">{error}</p>}
        
        {/* {config?.last_sync_at && (
          <p className="research-last-sync">
            Last synced: {formatDate(config.last_sync_at)}
          </p>
        )} */}
      </div>
    </div>
  )
}

function StatValue({ created, updated, deleted, unchanged }) {
  const hasChanges = created > 0 || updated > 0 || deleted > 0
  
  if (!hasChanges) {
    return <span className="research-stat-value">no change</span>
  }

  const parts = []
  if (created > 0) {
    parts.push(<span key="created" style={{ color: '#10b981' }}>{created} created</span>)
  }
  if (updated > 0) {
    parts.push(<span key="updated" style={{ color: '#f59e0b' }}>{updated} updated</span>)
  }
  if (deleted > 0) {
    parts.push(<span key="deleted" style={{ color: '#ef4444' }}>{deleted} deleted</span>)
  }

  return (
    <span className="research-stat-value">
      {parts.reduce((acc, part, idx) => {
        if (idx > 0) acc.push(', ')
        acc.push(part)
        return acc
      }, [])}
    </span>
  )
}

function SyncStats({ stats }) {
  if (!stats) return null

  return (
    <div className="research-sync-stats">
      <div className="research-stats-grid">
        <div className="research-stat-item">
          <span className="research-stat-label">Resources</span>
          <StatValue
            created={stats.resources_created}
            updated={stats.resources_updated}
            deleted={stats.resources_deleted}
            unchanged={stats.resources_unchanged}
          />
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Annotations</span>
          <StatValue
            created={stats.annotations_created}
            updated={stats.annotations_updated}
            deleted={stats.annotations_deleted}
            unchanged={stats.annotations_unchanged}
          />
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Comments</span>
          <StatValue
            created={stats.comments_created}
            updated={stats.comments_updated}
            deleted={stats.comments_deleted}
            unchanged={stats.comments_unchanged}
          />
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Notes</span>
          <StatValue
            created={stats.notes_created}
            updated={stats.notes_updated}
            deleted={stats.notes_deleted}
            unchanged={stats.notes_unchanged}
          />
        </div>
      </div>
    </div>
  )
}

function AnnotationItem({ annotation }) {
  const [showComments, setShowComments] = useState(false)

  return (
    <div className="research-annotation">
      <div className="research-annotation-text">
        <p>{annotation.text}</p>
      </div>
      
      {annotation.comments && annotation.comments.length > 0 && (
        <>
          <button 
            className="research-comments-toggle"
            onClick={() => setShowComments(!showComments)}
          >
            {showComments ? '▼' : '▶'} {annotation.comments.length} comment{annotation.comments.length > 1 ? 's' : ''}
          </button>
          
          {showComments && (
            <div className="research-comments">
              {annotation.comments.map((comment) => (
                <div key={comment.id} className="research-comment">
                  <div dangerouslySetInnerHTML={{ __html: comment.content }} />
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  )
}

function groupAnnotationsByPage(annotations) {
  const groups = {}

  for (const ann of annotations) {
    const page = ann.boundary?.pageNumber ?? 'No Page'
    if (!groups[page]) {
      groups[page] = []
    }
    groups[page].push(ann)
  }

  const sortedPages = Object.keys(groups).sort((a, b) => {
    if (a === 'No Page') return 1
    if (b === 'No Page') return -1
    return Number(a) - Number(b)
  })

  return sortedPages.map(page => ({
    page,
    annotations: groups[page]
  }))
}

// Parse chapters from config and sort by chapter number
function parseChapters(config) {
  if (!config?.chapters) return []

  return Object.entries(config.chapters)
    .map(([key, [title, startPage, endPage]]) => ({
      key,
      title,
      startPage,
      endPage
    }))
    .sort((a, b) => Number(a.key) - Number(b.key))
}

// Get annotations for a specific chapter (by page range)
function getAnnotationsForChapter(annotations, chapter) {
  return annotations.filter(ann => {
    const page = ann.boundary?.pageNumber
    if (page == null) return false
    return page >= chapter.startPage && page <= chapter.endPage
  })
}

// Count annotations per chapter
function getChapterAnnotationCounts(annotations, chapters) {
  const counts = {}
  for (const chapter of chapters) {
    counts[chapter.key] = getAnnotationsForChapter(annotations, chapter).length
  }
  return counts
}

function ChapterEditor({ config, onSave, onCancel, saving }) {
  const [json, setJson] = useState('')
  const [error, setError] = useState(null)

  useEffect(() => {
    // Initialize with current chapters or empty object
    const chapters = config?.chapters || {}
    setJson(JSON.stringify(chapters, null, 2))
  }, [config])

  const handleSave = () => {
    setError(null)
    try {
      const parsed = JSON.parse(json)
      // Validate structure
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
      onSave({ chapters: parsed })
    } catch (err) {
      setError(err.message)
    }
  }

  return (
    <div className="research-chapter-editor">
      <h4>Chapter Boundaries</h4>
      <p className="research-chapter-help">
        Format: {"{"}"1": ["Title", startPage, endPage], ...{"}"}
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

function ChapterSidebar({ chapters, annotationCounts, totalAnnotations, selectedChapter, onSelectChapter, onEditChapters }) {
  if (chapters.length === 0) {
    return (
      <div className="research-chapter-sidebar">
        <h3>Chapters</h3>
        <p className="research-chapter-empty">No chapters defined</p>
        <button onClick={onEditChapters} className="research-btn research-btn-secondary">
          + Add Chapters
        </button>
      </div>
    )
  }

  return (
    <div className="research-chapter-sidebar">
      <h3>Chapters</h3>
      <div className="research-chapter-list">
        {/* "All" option to show all annotations */}
        <div
          className={`research-chapter-item ${selectedChapter === null ? 'selected' : ''}`}
          onClick={() => onSelectChapter(null)}
        >
          <div className="research-chapter-item-title">
            {selectedChapter === null && <span className="research-chapter-marker">▶ </span>}
            All Annotations
          </div>
          <div className="research-chapter-item-meta">
            {totalAnnotations} total
          </div>
        </div>
        {chapters.map((chapter) => (
          <div
            key={chapter.key}
            className={`research-chapter-item ${selectedChapter === chapter.key ? 'selected' : ''}`}
            onClick={() => onSelectChapter(chapter.key)}
          >
            <div className="research-chapter-item-title">
              {selectedChapter === chapter.key && <span className="research-chapter-marker">▶ </span>}
              {chapter.title}
            </div>
            <div className="research-chapter-item-meta">
              ({chapter.startPage}-{chapter.endPage}) · {annotationCounts[chapter.key] || 0}
            </div>
          </div>
        ))}
      </div>
      <button onClick={onEditChapters} className="research-btn research-btn-secondary research-chapter-edit-btn">
        Edit Chapters
      </button>
    </div>
  )
}

// Read chapter from URL hash
function getChapterFromHash() {
  const hash = window.location.hash
  if (hash && hash.startsWith('#chapter-')) {
    return hash.replace('#chapter-', '')
  }
  return null
}

function ResourceDetail({ resourceId, onBack }) {
  const [data, setData] = useState(null)
  const [loading, setLoading] = useState(true)
  const [syncing, setSyncing] = useState(false)
  const [savingConfig, setSavingConfig] = useState(false)
  const [editingChapters, setEditingChapters] = useState(false)
  const [selectedChapter, setSelectedChapter] = useState(getChapterFromHash)
  const [showNotes, setShowNotes] = useState(true)
  const [error, setError] = useState(null)

  // Update URL hash when chapter changes
  useEffect(() => {
    if (selectedChapter) {
      window.history.replaceState(null, '', `#chapter-${selectedChapter}`)
    } else {
      // Clear hash when "All" is selected
      window.history.replaceState(null, '', window.location.pathname)
    }
  }, [selectedChapter])

  useEffect(() => {
    loadResourceFull()
  }, [resourceId])

  const loadResourceFull = async () => {
    try {
      setLoading(true)
      setError(null)
      const res = await fetch(`/commonplace/resources/${resourceId}/full`)
      if (res.ok) {
        const result = await res.json()
        setData(result.data)
        // Select chapter from URL hash if present (otherwise stay on "All")
        const chapters = parseChapters(result.data?.config)
        const hashChapter = getChapterFromHash()
        if (hashChapter && chapters.some(c => c.key === hashChapter)) {
          setSelectedChapter(hashChapter)
        }
      } else {
        setError('Resource not found')
      }
    } catch (err) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  const handleSync = async () => {
    setSyncing(true)
    try {
      const res = await fetch('/research/sync', { method: 'POST' })
      if (res.ok) {
        await loadResourceFull()
      }
    } catch (err) {
      console.error('Sync failed:', err)
    } finally {
      setSyncing(false)
    }
  }

  const handleSaveConfig = async (newConfig) => {
    setSavingConfig(true)
    try {
      const res = await fetch(`/commonplace/resources/${resourceId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ config: newConfig })
      })
      if (res.ok) {
        const result = await res.json()
        setData(prev => ({ ...prev, config: result.data.config }))
        setEditingChapters(false)
        // Select first chapter if we just added chapters
        const chapters = parseChapters(result.data.config)
        if (chapters.length > 0) {
          setSelectedChapter(chapters[0].key)
        }
      }
    } catch (err) {
      console.error('Failed to save config:', err)
    } finally {
      setSavingConfig(false)
    }
  }

  if (loading) {
    return <div className="research-loading">Loading...</div>
  }

  if (error) {
    return (
      <div className="research-detail">
        <button onClick={onBack} className="research-back-btn">
          ← Back to list
        </button>
        <p className="research-error">{error}</p>
      </div>
    )
  }

  const hasNotes = data?.notes && data.notes.length > 0
  const hasAnnotations = data?.annotations && data.annotations.length > 0
  const chapters = parseChapters(data?.config)
  const hasChapters = chapters.length > 0
  const annotationCounts = hasAnnotations ? getChapterAnnotationCounts(data.annotations, chapters) : {}

  // Get annotations to display (filtered by chapter if selected)
  const selectedChapterData = hasChapters && selectedChapter
    ? chapters.find(c => c.key === selectedChapter)
    : null
  const displayAnnotations = selectedChapterData
    ? getAnnotationsForChapter(data?.annotations || [], selectedChapterData)
    : data?.annotations || []

  return (
    <div className="research-detail">
      <div className="research-detail-header">
        <button onClick={onBack} className="research-back-btn">
          ← Back to list
        </button>
        <div className="research-detail-header-right">
          <h2 className="research-detail-title">{data?.title}</h2>
          <button
            onClick={handleSync}
            disabled={syncing}
            className="research-btn research-btn-secondary"
          >
            {syncing ? 'Syncing...' : 'Sync'}
          </button>
        </div>
      </div>

      {editingChapters && (
        <ChapterEditor
          config={data?.config}
          onSave={handleSaveConfig}
          onCancel={() => setEditingChapters(false)}
          saving={savingConfig}
        />
      )}

      {!editingChapters && (
        <div className="research-detail-layout">
          {/* Left: Chapter Sidebar */}
          <div className="research-toc-column">
            <ChapterSidebar
              chapters={chapters}
              annotationCounts={annotationCounts}
              totalAnnotations={data?.annotations?.length || 0}
              selectedChapter={selectedChapter}
              onSelectChapter={setSelectedChapter}
              onEditChapters={() => setEditingChapters(true)}
            />
          </div>

          {/* Center: Annotations */}
          <div className="research-annotations-column">
            {hasAnnotations ? (
              <div className="research-section">
                <h3>
                  {selectedChapterData
                    ? `${selectedChapterData.title} (${displayAnnotations.length})`
                    : `Annotations (${data.annotations.length})`
                  }
                </h3>
                <div className="research-annotations">
                  {groupAnnotationsByPage(displayAnnotations).map((group) => (
                    <div key={group.page} className="research-page-group">
                      <div className="research-page-header">
                        <span className="research-page-number">
                          {group.page === 'No Page' ? 'No Page' : `Page ${group.page}`}
                        </span>
                      </div>
                      {group.annotations.map((ann) => (
                        <AnnotationItem key={ann.id} annotation={ann} />
                      ))}
                    </div>
                  ))}
                  {displayAnnotations.length === 0 && (
                    <p className="research-empty">No annotations in this chapter.</p>
                  )}
                </div>
              </div>
            ) : (
              <p className="research-empty">No annotations for this resource.</p>
            )}
          </div>

          {/* Right: Notes (collapsible) */}
          <div className={`research-notes-column ${showNotes ? 'expanded' : 'collapsed'}`}>
            <button
              className="research-notes-toggle"
              onClick={() => setShowNotes(!showNotes)}
              title={showNotes ? 'Hide notes' : 'Show notes'}
            >
              {showNotes ? '▶' : '◀'} Notes {hasNotes && `(${data.notes.length})`}
            </button>
            {showNotes && hasNotes && (
              <div className="research-notes">
                {data.notes.map((note) => (
                  <div key={note.id} className="research-note">
                    <div dangerouslySetInnerHTML={{ __html: note.content }} />
                  </div>
                ))}
              </div>
            )}
            {showNotes && !hasNotes && (
              <p className="research-notes-empty">No notes</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

function ResourceList({ resources, onNavigate }) {
  if (!resources.length) {
    return <p className="research-empty">No resources synced yet. Configure the database path and sync.</p>
  }

  return (
    <div className="research-list">
      {resources.map((resource) => (
        <div 
          key={resource.id} 
          className="research-list-item"
          onClick={() => onNavigate(resource.id)}
        >
          <span className="research-list-title">{trimTitle(resource.title)}</span>
          <span className="research-list-date">{formatDate(resource.created_at)}</span>
        </div>
      ))}
    </div>
  )
}

export default function ResearchPage({ resourceId, onNavigate }) {
  const [config, setConfig] = useState(null)
  const [resources, setResources] = useState([])
  const [loading, setLoading] = useState(true)
  const [syncing, setSyncing] = useState(false)
  const [syncStats, setSyncStats] = useState(null)
  const [error, setError] = useState(null)

  useEffect(() => {
    loadData()
  }, [])

  const loadData = async () => {
    try {
      setLoading(true)
      setError(null)

      const [configRes, resourcesRes] = await Promise.all([
        fetch('/research/config'),
        fetch('/commonplace/resources?limit=100&type=pdf')
      ])

      if (configRes.ok) {
        const configData = await configRes.json()
        setConfig(configData.data)
      }

      if (resourcesRes.ok) {
        const resourcesData = await resourcesRes.json()
        setResources(resourcesData.data || [])
      }
    } catch (err) {
      console.error('Failed to load data:', err)
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  const handleSync = async () => {
    setSyncing(true)
    setSyncStats(null)
    setError(null)

    try {
      const res = await fetch('/research/sync', { method: 'POST' })
      const data = await res.json()

      if (!res.ok) {
        throw new Error(data.error || 'Sync failed')
      }

      setSyncStats(data.data)
      await loadData()
    } catch (err) {
      setError(err.message)
    } finally {
      setSyncing(false)
    }
  }

  if (loading) {
    return (
      <div className="research-container">
        <div className="research-loading">Loading...</div>
      </div>
    )
  }

  if (resourceId) {
    return (
      <div className="research-container">
        <ResourceDetail 
          resourceId={resourceId} 
          onBack={() => onNavigate(null)} 
        />
      </div>
    )
  }

  return (
    <div className="research-container">
      <div className="research-sidebar">
        <ConfigPanel
          config={config}
          onConfigChange={setConfig}
          onSync={handleSync}
          syncing={syncing}
        />
        
        {error && <p className="research-error">{error}</p>}
        
        <SyncStats stats={syncStats} />
      </div>
      
      <div className="research-main">
        <h2 className="research-heading">Research Papers ({resources.length})</h2>
        <ResourceList 
          resources={resources} 
          onNavigate={onNavigate} 
        />
      </div>
    </div>
  )
}
