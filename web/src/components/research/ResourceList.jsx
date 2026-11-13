import React from 'react'
import DocumentLink from '../DocumentLink.jsx'
import { formatDate } from '../../text.js'

function trimTitle(title, maxLength = 60) {
  if (!title) return title
  if (title.length <= maxLength) return title
  return title.substring(0, maxLength).trim() + '...'
}

export default function ResourceList({ resources }) {
  if (!resources.length) {
    return <p className="research-empty">No resources synced yet. Set the database path in the config file and sync.</p>
  }

  return (
    <div className="research-list">
      {resources.map((resource) => (
        <DocumentLink key={resource.id} id={resource.id} className="research-list-item">
          <span className="research-list-title">{trimTitle(resource.title)}</span>
          <span className="research-list-date">{formatDate(resource.created_at)}</span>
        </DocumentLink>
      ))}
    </div>
  )
}
