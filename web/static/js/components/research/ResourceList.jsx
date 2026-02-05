import React from 'react'

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

export default function ResourceList({ resources, onNavigate }) {
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
