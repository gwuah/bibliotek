import React, { useState, useEffect } from 'react'

/**
 * ResearchPage - Displays synced research annotations from the Research app
 * Allows configuring the Research database path and syncing data
 */

// ============================================================================
// Utility Functions
// ============================================================================

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

// ============================================================================
// Components
// ============================================================================

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
      <h3>Research Database</h3>
      
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
        
        {config?.last_sync_at && (
          <p className="research-last-sync">
            Last synced: {formatDate(config.last_sync_at)}
          </p>
        )}
      </div>
    </div>
  )
}

function SyncStats({ stats }) {
  if (!stats) return null

  return (
    <div className="research-sync-stats">
      <h4>Sync Results</h4>
      <div className="research-stats-grid">
        <div className="research-stat-item">
          <span className="research-stat-label">Resources</span>
          <span className="research-stat-value">
            +{stats.resources_created} / ={stats.resources_skipped}
          </span>
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Annotations</span>
          <span className="research-stat-value">
            +{stats.annotations_created} / ={stats.annotations_skipped}
          </span>
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Comments</span>
          <span className="research-stat-value">
            +{stats.comments_created} / ={stats.comments_skipped}
          </span>
        </div>
        <div className="research-stat-item">
          <span className="research-stat-label">Notes</span>
          <span className="research-stat-value">
            +{stats.notes_created} / ={stats.notes_skipped}
          </span>
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
        {annotation.boundary?.pageNumber && (
          <span className="research-page-number">Page {annotation.boundary.pageNumber}</span>
        )}
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

function ResourceDetail({ resource, onBack }) {
  const [data, setData] = useState(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    loadResourceFull()
  }, [resource.id])

  const loadResourceFull = async () => {
    try {
      setLoading(true)
      const res = await fetch(`/commonplace/resources/${resource.id}/full`)
      if (res.ok) {
        const result = await res.json()
        setData(result.data)
      }
    } finally {
      setLoading(false)
    }
  }

  if (loading) {
    return <div className="research-loading">Loading...</div>
  }

  return (
    <div className="research-detail">
      <button onClick={onBack} className="research-back-btn">
        ← Back to list
      </button>
      
      <h2 className="research-detail-title">{resource.title}</h2>
      
      {/* Notes Section */}
      {data?.notes && data.notes.length > 0 && (
        <div className="research-section">
          <h3>Notes ({data.notes.length})</h3>
          <div className="research-notes">
            {data.notes.map((note) => (
              <div key={note.id} className="research-note">
                <div dangerouslySetInnerHTML={{ __html: note.content }} />
              </div>
            ))}
          </div>
        </div>
      )}
      
      {/* Annotations Section */}
      {data?.annotations && data.annotations.length > 0 && (
        <div className="research-section">
          <h3>Annotations ({data.annotations.length})</h3>
          <div className="research-annotations">
            {data.annotations.map((ann) => (
              <AnnotationItem key={ann.id} annotation={ann} />
            ))}
          </div>
        </div>
      )}
      
      {(!data?.annotations?.length && !data?.notes?.length) && (
        <p className="research-empty">No annotations or notes for this resource.</p>
      )}
    </div>
  )
}

function ResourceList({ resources, onSelect }) {
  if (!resources.length) {
    return <p className="research-empty">No resources synced yet. Configure the database path and sync.</p>
  }

  return (
    <div className="research-list">
      {resources.map((resource) => (
        <div 
          key={resource.id} 
          className="research-list-item"
          onClick={() => onSelect(resource)}
        >
          <span className="research-list-title">{trimTitle(resource.title)}</span>
          <span className="research-list-date">{formatDate(resource.created_at)}</span>
        </div>
      ))}
    </div>
  )
}

// ============================================================================
// Main Component
// ============================================================================

export default function ResearchPage() {
  const [config, setConfig] = useState(null)
  const [resources, setResources] = useState([])
  const [selectedResource, setSelectedResource] = useState(null)
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

      // Load config and PDF resources in parallel
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
      
      // Reload resources after sync
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
        {selectedResource ? (
          <ResourceDetail 
            resource={selectedResource} 
            onBack={() => setSelectedResource(null)} 
          />
        ) : (
          <>
            <h2 className="research-heading">Research Papers ({resources.length})</h2>
            <ResourceList 
              resources={resources} 
              onSelect={setSelectedResource} 
            />
          </>
        )}
      </div>
    </div>
  )
}

