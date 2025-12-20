import React, { useState, useEffect } from 'react'

/**
 * HighlightsPage - A 1:1 replica of the Light browser extension UI
 * Fetches highlights from the Commonplace API and displays them
 * in the exact same format as the Light extension popup.
 * 
 * @see https://github.com/gwuah/light
 */

// ============================================================================
// Utility Functions (from Light extension's popup.js)
// ============================================================================

function cleanUrl(url) {
  try {
    const urlObj = new URL(url)
    const searchParams = new URLSearchParams(urlObj.search)

    // Remove UTM parameters
    const utmParams = [
      'utm_source',
      'utm_medium',
      'utm_campaign',
      'utm_term',
      'utm_content',
    ]
    utmParams.forEach((param) => searchParams.delete(param))

    // Reconstruct the URL with cleaned parameters
    const cleanedSearch = searchParams.toString()
    return urlObj.pathname + (cleanedSearch ? '?' + cleanedSearch : '')
  } catch (e) {
    return url
  }
}

function formatDate(timestamp) {
  const date = new Date(timestamp)
  const day = String(date.getDate()).padStart(2, '0')
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const year = String(date.getFullYear()).slice(-2)
  return `${day}/${month}/${year}`
}

function getLatestTimestamp(highlights) {
  return Math.max(...highlights.map((h) => new Date(h.date).getTime()))
}

// ============================================================================
// Transform Commonplace data to Light format
// ============================================================================

function transformCommonplaceToLight(resources) {
  const highlightsByUrl = {}

  resources.forEach((resource) => {
    if (resource.annotations && resource.annotations.length > 0) {
      const url = resource.title // In Light sync, URL is stored as title

      highlightsByUrl[url] = resource.annotations.map((annotation) => {
        // Extract Light metadata from boundary if available
        const boundary = annotation.boundary || {}

        return {
          chunks: boundary.chunks || [annotation.text],
          date: boundary.date || annotation.created_at,
          groupID: boundary.groupID || annotation.id,
          repr: annotation.text,
          url: boundary.url || url,
        }
      })
    }
  })

  return highlightsByUrl
}

// ============================================================================
// Components
// ============================================================================

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

  // Get latest timestamp for this domain
  const latestTimestamp = Math.max(
    ...Object.values(urlsInDomain)
      .flat()
      .map((h) => new Date(h.date).getTime())
  )
  const formattedDate = formatDate(latestTimestamp)

  // Sort URLs within domain by latest highlight timestamp
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
  // Group URLs by domain
  const domainGroups = {}
  Object.keys(highlightsByUrl).forEach((url) => {
    try {
      const hostname = new URL(url).hostname
      if (!domainGroups[hostname]) {
        domainGroups[hostname] = {}
      }
      domainGroups[hostname][url] = highlightsByUrl[url]
    } catch (e) {
      // Invalid URL, skip
    }
  })

  // Sort domains by latest highlight timestamp
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
  // Calculate stats
  const domainGroups = {}
  Object.keys(highlightsByUrl).forEach((url) => {
    try {
      const hostname = new URL(url).hostname
      if (!domainGroups[hostname]) {
        domainGroups[hostname] = {}
      }
      domainGroups[hostname][url] = highlightsByUrl[url]
    } catch (e) {
      // Invalid URL, skip
    }
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


// ============================================================================
// Main Component
// ============================================================================

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

      // Fetch all resources with their annotations from Commonplace API
      const response = await fetch('/commonplace/resources?limit=100')
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`)
      }

      const result = await response.json()
      const resources = result.data || []

      // For each resource, fetch its full data including annotations
      const fullResources = await Promise.all(
        resources.map(async (resource) => {
          const fullResponse = await fetch(`/commonplace/resources/${resource.id}/full`)
          if (fullResponse.ok) {
            const fullResult = await fullResponse.json()
            return fullResult.data
          }
          return resource
        })
      )

      // Transform to Light format
      const transformed = transformCommonplaceToLight(fullResources)
      setHighlightsByUrl(transformed)
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

