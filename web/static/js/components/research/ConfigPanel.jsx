import React, { useState, useEffect } from 'react'

export default function ConfigPanel({ config, onConfigChange, onSync, syncing }) {
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
      </div>
    </div>
  )
}
