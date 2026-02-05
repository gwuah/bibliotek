import React from 'react'

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

export default function SyncStats({ stats }) {
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
