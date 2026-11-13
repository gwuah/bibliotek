import React, { useEffect, useRef, useSyncExternalStore } from 'react'
import { uploadRunner } from '../uploads/runner.js'
import { canQueue, needsFile, progressOf } from '../uploads/queue.js'
import { formatBytes } from '../text.js'

function statusOf(entry) {
  if (needsFile(entry)) return 'pending_server'
  if (entry.phase === 'active') return 'uploading'
  if (entry.phase === 'done') return 'completed'
  if (entry.phase === 'failed' || (entry.phase === 'paused' && entry.error)) return 'error'
  return 'pending'
}

function viewOf(entry) {
  const status = statusOf(entry)
  const done = progressOf(entry)
  return {
    key: entry.key,
    file_name: entry.filename,
    status,
    bytes_uploaded: Math.round(done * entry.size),
    progress: status === 'pending_server' ? 0 : Math.round(done * 100),
  }
}

function useQueue() {
  return useSyncExternalStore(uploadRunner.subscribe, uploadRunner.getSnapshot)
}

export default function MassUploader() {
  const { entries, recovery } = useQueue()
  const fileInputRef = useRef(null)

  useEffect(() => {
    uploadRunner.recover()
  }, [])

  const listed = entries.filter(e => !e.reading)
  const queue = listed.map(viewOf)

  const handleDrop = (e) => {
    e.preventDefault()
    e.currentTarget.classList.remove('dragging')
    uploadRunner.add(e.dataTransfer?.files || [])
  }

  const handleDragOver = (e) => {
    e.preventDefault()
    e.currentTarget.classList.add('dragging')
  }

  const handleDragLeave = (e) => {
    e.preventDefault()
    e.currentTarget.classList.remove('dragging')
  }

  const isUploading = entries.some(e => e.phase === 'queued' || e.phase === 'active')
  const hasPendingWithFile = listed.some(canQueue)
  const hasErrors = queue.some(e => e.status === 'error')
  const hasPendingServer = queue.some(e => e.status === 'pending_server')

  return (
    <div className="uploader-panel">
      <div
        className="file-drop-area"
        onClick={() => fileInputRef.current?.click()}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
      >
        <input
          ref={fileInputRef}
          type="file"
          className="hidden"
          multiple
          accept="application/pdf"
          onChange={(e) => uploadRunner.add(e.target.files)}
        />
        <div className="file-drop-content">
          <svg className="w-5 h-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 20 16">
            <path stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 13h3a3 3 0 0 0 0-6h-.025A5.56 5.56 0 0 0 16 6.5 5.5 5.5 0 0 0 5.207 5.021C5.137 5.017 5.071 5 5 5a4 4 0 0 0 0 8h2.167M10 15V6m0 0L8 8m2-2 2 2" />
          </svg>
          <div className="file-drop-text">
            <p className="text-sm font-bold">drop or click to add pdfs</p>
          </div>
        </div>
      </div>

      {recovery === 'loading' && (
        <div className="text-xs text-gray-500 mt-2">Loading pending uploads...</div>
      )}

      {queue.length > 0 && (
        <>
          <ul className="upload-queue">
            {queue.map(entry => (
              <li key={entry.key} className={`upload-item ${entry.status}`}>
                <div className="upload-item-header">
                  <span className="upload-item-name">{entry.file_name}</span>
                  <div className="upload-item-actions">
                    {entry.status === 'pending_server' && (
                      <span className="upload-item-hint text-xs text-amber-600">
                        {formatBytes(entry.bytes_uploaded)} - select file
                      </span>
                    )}
                    {entry.status !== 'completed' && (
                      <button
                        onClick={(e) => { e.stopPropagation(); uploadRunner.remove(entry.key) }}
                        className="upload-cancel-btn text-gray-400 hover:text-red-500 ml-2"
                        title="Cancel"
                      >
                        ×
                      </button>
                    )}
                  </div>
                </div>
                <div className="upload-progress-track">
                  <div
                    className={`upload-progress-fill ${entry.status === 'pending' || entry.status === 'pending_server' ? 'pending' : ''}`}
                    style={{ width: `${entry.progress}%` }}
                  />
                </div>
                {entry.status === 'error' && (
                  <div className="text-xs text-red-500 mt-1">Upload failed</div>
                )}
              </li>
            ))}
          </ul>
          <div className="upload-controls">
            {hasPendingServer && !hasPendingWithFile && (
              <div className="text-xs text-amber-600 mb-2 text-center">
                Select files to resume incomplete uploads
              </div>
            )}
            <button
              onClick={() => uploadRunner.start()}
              disabled={isUploading || !hasPendingWithFile}
              className={`start-upload-button w-full border border-gray-300 text-sm py-4 cursor-pointer bg-gray-100 hover:bg-gray-200 ${isUploading || !hasPendingWithFile ? 'disabled' : ''}`}
            >
              {isUploading ? 'uploading...' : 'Upload'}
            </button>
            {hasErrors && !isUploading && (
              <button
                onClick={() => uploadRunner.start()}
                className="w-full border border-red-300 text-sm py-2 mt-2 cursor-pointer bg-red-50 hover:bg-red-100 text-red-700"
              >
                Retry Failed
              </button>
            )}
          </div>
        </>
      )}
    </div>
  )
}
