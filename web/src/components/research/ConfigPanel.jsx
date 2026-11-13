import React from 'react'

export default function ConfigPanel({ research }) {
  return (
    <div className="research-config-panel">
      <div className="research-config-form">
        <label>
          <span>Database Path:</span>
          <input
            type="text"
            value=""
            readOnly
            placeholder="research_database in the config file"
            className="research-input"
          />
        </label>

        <div className="research-config-actions">
          <button
            onClick={research.sync}
            disabled={research.syncing}
            className="research-btn research-btn-primary"
          >
            {research.syncing ? 'Syncing...' : 'Sync Now'}
          </button>
        </div>
      </div>
    </div>
  )
}
