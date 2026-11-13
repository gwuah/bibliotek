import React from 'react'

function StatValue({ created, updated, deleted }) {
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

  const { documents } = stats
  const rows = [
    ['Resources', {
      created: documents.created,
      updated: documents.linked,
      deleted: documents.deleted + documents.unlinked,
    }],
    ['Annotations', stats.highlights],
    ['Comments', stats.comments],
    ['Notes', stats.notes],
  ]

  return (
    <div className="research-sync-stats">
      <div className="research-stats-grid">
        {rows.map(([label, counts]) => (
          <div key={label} className="research-stat-item">
            <span className="research-stat-label">{label}</span>
            <StatValue created={counts.created} updated={counts.updated} deleted={counts.deleted} />
          </div>
        ))}
      </div>
    </div>
  )
}
