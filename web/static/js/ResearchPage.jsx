import React, { useState, useEffect } from 'react'
import ConfigPanel from './components/research/ConfigPanel.jsx'
import SyncStats from './components/research/SyncStats.jsx'
import ResourceDetail from './components/research/ResourceDetail.jsx'
import ResourceList from './components/research/ResourceList.jsx'

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
