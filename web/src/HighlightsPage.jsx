import React, { useState, useEffect } from 'react'
import { getDocument, listAllDocuments } from './api.js'
import { formatDate } from './text.js'

function cleanUrl(url) {
  try {
    const urlObj = new URL(url)
    const searchParams = new URLSearchParams(urlObj.search)

    const utmParams = [
      'utm_source',
      'utm_medium',
      'utm_campaign',
      'utm_term',
      'utm_content',
    ]
    utmParams.forEach((param) => searchParams.delete(param))

    const cleanedSearch = searchParams.toString()
    return urlObj.pathname + (cleanedSearch ? '?' + cleanedSearch : '')
  } catch (e) {
    return url
  }
}

function getLatestTimestamp(highlights) {
  return Math.max(...highlights.map((h) => new Date(h.date).getTime()))
}

function transformDocumentsToLight(documents) {
  const highlightsByUrl = {}

  documents.forEach((document) => {
    if (document.highlights.length > 0) {
      const url = document.canonical_url ?? document.title

      highlightsByUrl[url] = document.highlights.map((highlight) => {
        const payload = highlight.location?.payload || {}

        return {
          chunks: payload.chunks || [highlight.text],
          date: highlight.created_at,
          groupID: highlight.id,
          repr: highlight.text,
          url: payload.url || url,
        }
      })
    }
  })

  return highlightsByUrl
}

function HighlightText({ highlight }) {
  return (
    <div className="light-highlight-text">
      <p>{highlight.repr}</p>
    </div>
  )
}

function HighlightsForUrl({ highlights, isExpanded }) {
  if (!isExpanded) return null

  return (
    <div className={`light-highlights-for-url ${isExpanded ? 'expanded' : ''}`}>
      {highlights.map((highlight) => (
        <HighlightText key={highlight.groupID} highlight={highlight} />
      ))}
    </div>
  )
}

function PathItem({ url, highlights }) {
  const [isExpanded, setIsExpanded] = useState(false)
  const cleanPath = cleanUrl(url)

  return (
    <>
      <div
        className="light-path-item"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {cleanPath}
      </div>
      <HighlightsForUrl highlights={highlights} isExpanded={isExpanded} />
    </>
  )
}

function DomainItem({ domain, urlsInDomain }) {
  const [isExpanded, setIsExpanded] = useState(false)

  const latestTimestamp = Math.max(
    ...Object.values(urlsInDomain)
      .flat()
      .map((h) => new Date(h.date).getTime())
  )
  const formattedDate = formatDate(latestTimestamp)

  const sortedUrls = Object.keys(urlsInDomain).sort((a, b) => {
    const latestA = getLatestTimestamp(urlsInDomain[a])
    const latestB = getLatestTimestamp(urlsInDomain[b])
    return latestB - latestA
  })

  return (
    <>
      <div
        className="light-url-item"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <span>{domain}</span>
        <span className="light-url-item-date">{formattedDate}</span>
      </div>
      <div className={`light-domain-content ${isExpanded ? 'expanded' : ''}`}>
        {sortedUrls.map((url) => (
          <PathItem
            key={url}
            url={url}
            highlights={urlsInDomain[url]}
          />
        ))}
      </div>
    </>
  )
}

function HighlightsList({ highlightsByUrl }) {
  const domainGroups = {}
  Object.keys(highlightsByUrl).forEach((url) => {
    try {
      const hostname = new URL(url).hostname
      if (!domainGroups[hostname]) {
        domainGroups[hostname] = {}
      }
      domainGroups[hostname][url] = highlightsByUrl[url]
    } catch (e) {}
  })

  const sortedDomains = Object.keys(domainGroups).sort((a, b) => {
    const latestA = Math.max(
      ...Object.values(domainGroups[a])
        .flat()
        .map((h) => new Date(h.date).getTime())
    )
    const latestB = Math.max(
      ...Object.values(domainGroups[b])
        .flat()
        .map((h) => new Date(h.date).getTime())
    )
    return latestB - latestA
  })

  if (sortedDomains.length === 0) {
    return <div className="light-empty">No highlights yet</div>
  }

  return (
    <div className="light-highlights-list">
      {sortedDomains.map((domain) => (
        <DomainItem
          key={domain}
          domain={domain}
          urlsInDomain={domainGroups[domain]}
        />
      ))}
    </div>
  )
}

function Stats({ highlightsByUrl }) {
  const domainGroups = {}
  Object.keys(highlightsByUrl).forEach((url) => {
    try {
      const hostname = new URL(url).hostname
      if (!domainGroups[hostname]) {
        domainGroups[hostname] = {}
      }
      domainGroups[hostname][url] = highlightsByUrl[url]
    } catch (e) {}
  })

  const totalWebsites = Object.keys(domainGroups).length
  const totalHighlights = Object.values(highlightsByUrl).reduce(
    (sum, highlights) => sum + highlights.length,
    0
  )

  return (
    <div className="light-stats">
      <div className="light-stat">
        <p className="light-stat-desc">WEBSITES</p>
        <p className="light-stat-val">{totalWebsites}</p>
      </div>
      <div className="light-stat">
        <p className="light-stat-desc">HIGHLIGHTS</p>
        <p className="light-stat-val">{totalHighlights}</p>
      </div>
    </div>
  )
}

export default function HighlightsPage() {
  const [highlightsByUrl, setHighlightsByUrl] = useState({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(null)

  useEffect(() => {
    loadHighlights()
  }, [])

  const loadHighlights = async () => {
    try {
      setLoading(true)
      setError(null)

      const pages = await listAllDocuments({ kind: 'web' })
      const documents = await Promise.all(pages.map((page) => getDocument(page.id)))

      setHighlightsByUrl(transformDocumentsToLight(documents))
    } catch (err) {
      console.error('Failed to load highlights:', err)
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  if (loading) {
    return (
      <div className="light-container">
        <div className="light-loading">Loading highlights...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="light-container">
        <div className="light-empty">Error loading highlights: {error}</div>
      </div>
    )
  }

  return (
    <div className="light-container">
      <Stats highlightsByUrl={highlightsByUrl} />
      <HighlightsList highlightsByUrl={highlightsByUrl} />
    </div>
  )
}

