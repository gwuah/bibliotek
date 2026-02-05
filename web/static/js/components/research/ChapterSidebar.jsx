import React from 'react'

export default function ChapterSidebar({
  chapters,
  annotationCounts,
  totalAnnotations,
  selectedChapter,
  onSelectChapter,
  onEditChapters,
  darkMode,
  onToggleDarkMode,
  syncing,
  onSync,
  collapsed,
  onToggleCollapse
}) {
  if (collapsed) {
    return (
      <div className="research-chapter-sidebar collapsed">
        <button onClick={onToggleCollapse} className="research-theme-toggle-small" title="Expand">
          ▸
        </button>
      </div>
    )
  }

  const headerButtons = (
    <div className="research-chapter-header-buttons">
      <button onClick={onToggleCollapse} className="research-theme-toggle-small" title="Collapse">
        ◂
      </button>
      <button onClick={onEditChapters} className="research-theme-toggle-small" title="Edit chapters">
        ✎
      </button>
      <button onClick={onSync} disabled={syncing} className="research-theme-toggle-small" title="Sync">
        {syncing ? '...' : '↻'}
      </button>
      <button onClick={onToggleDarkMode} className="research-theme-toggle-small" title={darkMode ? 'Light mode' : 'Dark mode'}>
        {darkMode ? '☀' : '●'}
      </button>
    </div>
  )

  if (chapters.length === 0) {
    return (
      <div className="research-chapter-sidebar">
        <div className="research-chapter-header">
          <h3>Chapters</h3>
          {headerButtons}
        </div>
        <p className="research-chapter-empty">No chapters defined</p>
      </div>
    )
  }

  return (
    <div className="research-chapter-sidebar">
      <div className="research-chapter-header">
        <h3>Chapters</h3>
        {headerButtons}
      </div>
      <div className="research-chapter-list">
        <div
          className={`research-chapter-item ${selectedChapter === null ? 'selected' : ''}`}
          onClick={() => onSelectChapter(null)}
        >
          <div className="research-chapter-item-title">
            {selectedChapter === null && <span className="research-chapter-marker">▶ </span>}
            All Annotations
          </div>
          <div className="research-chapter-item-meta">
            {totalAnnotations} total
          </div>
        </div>
        {chapters.map((chapter) => (
          <div
            key={chapter.key}
            className={`research-chapter-item ${selectedChapter === chapter.key ? 'selected' : ''}`}
            onClick={() => onSelectChapter(chapter.key)}
          >
            <div className="research-chapter-item-title">
              {selectedChapter === chapter.key && <span className="research-chapter-marker">▶ </span>}
              {chapter.title}
            </div>
            <div className="research-chapter-item-meta">
              ({chapter.startPage}-{chapter.endPage}) · {annotationCounts[chapter.key] || 0}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
