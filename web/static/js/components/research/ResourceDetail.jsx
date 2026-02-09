import React, { useState, useEffect } from "react";
import ChapterEditor from "./ChapterEditor.jsx";
import ChapterSidebar from "./ChapterSidebar.jsx";

function AnnotationItem({ annotation }) {
  const [showComments, setShowComments] = useState(false);

  return (
    <div className="research-annotation">
      <div className="research-annotation-text">
        <p>{annotation.text}</p>
      </div>

      {annotation.comments && annotation.comments.length > 0 && (
        <>
          <button
            className="research-comments-toggle"
            onClick={() => setShowComments(!showComments)}
          >
            {showComments ? "▼" : "▶"} {annotation.comments.length} comment
            {annotation.comments.length > 1 ? "s" : ""}
          </button>

          {showComments && (
            <div className="research-comments">
              {annotation.comments.map((comment) => (
                <div key={comment.id} className="research-comment">
                  <div dangerouslySetInnerHTML={{ __html: comment.content }} />
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function groupAnnotationsByPage(annotations) {
  const groups = {};

  for (const ann of annotations) {
    const page = ann.boundary?.pageNumber ?? "No Page";
    if (!groups[page]) {
      groups[page] = [];
    }
    groups[page].push(ann);
  }

  const sortedPages = Object.keys(groups).sort((a, b) => {
    if (a === "No Page") return 1;
    if (b === "No Page") return -1;
    return Number(a) - Number(b);
  });

  return sortedPages.map((page) => ({
    page,
    annotations: groups[page],
  }));
}

function parseChapters(config) {
  if (!config?.chapters) return [];

  const chapters = Object.entries(config.chapters)
    .map(([key, [title, startPage]]) => ({
      key: parseInt(key, 10),
      title,
      startPage,
    }))
    .sort((a, b) => a.startPage - b.startPage);

  return chapters.map((chapter, i) => ({
    ...chapter,
    endPage: i < chapters.length - 1 ? chapters[i + 1].startPage - 1 : Infinity,
  }));
}

function getAnnotationsForChapter(annotations, chapter) {
  return annotations.filter((ann) => {
    const page = ann.boundary?.pageNumber;
    if (page == null) return false;
    return page >= chapter.startPage && page <= chapter.endPage;
  });
}

function getChapterAnnotationCounts(annotations, chapters) {
  const counts = {};
  for (const chapter of chapters) {
    counts[chapter.key] = getAnnotationsForChapter(annotations, chapter).length;
  }
  return counts;
}

function getUnchapteredAnnotations(annotations, chapters) {
  if (chapters.length === 0) return annotations;
  return annotations.filter((ann) => {
    const page = ann.boundary?.pageNumber;
    if (page == null) return true;
    return !chapters.some((ch) => page >= ch.startPage && page <= ch.endPage);
  });
}

function getChapterFromHash() {
  const hash = window.location.hash;
  if (hash && hash.startsWith("#chapter-")) {
    const val = parseInt(hash.replace("#chapter-", ""), 10);
    return isNaN(val) ? null : val;
  }
  return null;
}

export default function ResourceDetail({ resourceId, onBack }) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [savingConfig, setSavingConfig] = useState(false);
  const [editingChapters, setEditingChapters] = useState(false);
  const [selectedChapter, setSelectedChapter] = useState(getChapterFromHash);
  const [darkMode, setDarkMode] = useState(() => {
    const saved = localStorage.getItem("research-dark-mode");
    return saved === "true";
  });
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [excludeChaptered, setExcludeChaptered] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    localStorage.setItem("research-dark-mode", darkMode);
  }, [darkMode]);

  useEffect(() => {
    if (selectedChapter) {
      window.history.replaceState(null, "", `#chapter-${selectedChapter}`);
    } else {
      window.history.replaceState(null, "", window.location.pathname);
    }
  }, [selectedChapter]);

  useEffect(() => {
    loadResourceFull();
  }, [resourceId]);

  const loadResourceFull = async () => {
    try {
      setLoading(true);
      setError(null);
      const res = await fetch(`/commonplace/resources/${resourceId}/full`);
      if (res.ok) {
        const result = await res.json();
        setData(result.data);
        const chapters = parseChapters(result.data?.config);
        const hashChapter = getChapterFromHash();
        if (hashChapter && chapters.some((c) => c.key === hashChapter)) {
          setSelectedChapter(hashChapter);
        }
      } else {
        setError("Resource not found");
      }
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleSync = async () => {
    setSyncing(true);
    try {
      const res = await fetch("/research/sync", { method: "POST" });
      if (res.ok) {
        await loadResourceFull();
      }
    } catch (err) {
      console.error("Sync failed:", err);
    } finally {
      setSyncing(false);
    }
  };

  const handleSaveConfig = async (newConfig) => {
    setSavingConfig(true);
    try {
      const res = await fetch(`/commonplace/resources/${resourceId}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ config: newConfig }),
      });
      if (res.ok) {
        const result = await res.json();
        setData((prev) => ({ ...prev, config: result.data.config }));
        setEditingChapters(false);
        const chapters = parseChapters(result.data.config);
        if (chapters.length > 0) {
          setSelectedChapter(chapters[0].key);
        }
      }
    } catch (err) {
      console.error("Failed to save config:", err);
    } finally {
      setSavingConfig(false);
    }
  };

  if (loading) {
    return <div className="research-loading">Loading...</div>;
  }

  if (error) {
    return (
      <div className="research-detail">
        <button onClick={onBack} className="research-back-btn">
          ← Back to list
        </button>
        <p className="research-error">{error}</p>
      </div>
    );
  }

  const hasNotes = data?.notes && data.notes.length > 0;
  const hasAnnotations = data?.annotations && data.annotations.length > 0;
  const chapters = parseChapters(data?.config);
  const hasChapters = chapters.length > 0;
  const annotationCounts = hasAnnotations
    ? getChapterAnnotationCounts(data.annotations, chapters)
    : {};

  const selectedChapterData =
    hasChapters && selectedChapter != null
      ? chapters.find((c) => c.key === selectedChapter)
      : null;
  const allAnnotations = data?.annotations || [];
  const displayAnnotations = selectedChapterData
    ? getAnnotationsForChapter(allAnnotations, selectedChapterData)
    : excludeChaptered && hasChapters
      ? getUnchapteredAnnotations(allAnnotations, chapters)
      : allAnnotations;

  return (
    <div className="research-detail">
      <div className="research-detail-header">
        <button onClick={onBack} className="research-back-btn">
          ← Back to list
        </button>
        <span className="research-detail-header-sep">|</span>
        <h2 className="research-detail-title">{data?.title}</h2>
      </div>

      <div className="research-detail-layout">
        <div
          className={`research-toc-column ${sidebarCollapsed ? "collapsed" : ""}`}
        >
          {editingChapters ? (
            <ChapterEditor
              config={data?.config}
              onSave={handleSaveConfig}
              onCancel={() => setEditingChapters(false)}
              saving={savingConfig}
            />
          ) : (
            <ChapterSidebar
              chapters={chapters}
              annotationCounts={annotationCounts}
              totalAnnotations={allAnnotations.length}
              unchapteredCount={
                hasChapters
                  ? getUnchapteredAnnotations(allAnnotations, chapters).length
                  : allAnnotations.length
              }
              selectedChapter={selectedChapter}
              onSelectChapter={setSelectedChapter}
              excludeChaptered={excludeChaptered}
              onToggleExcludeChaptered={() =>
                setExcludeChaptered(!excludeChaptered)
              }
              onEditChapters={() => setEditingChapters(true)}
              darkMode={darkMode}
              onToggleDarkMode={() => setDarkMode(!darkMode)}
              syncing={syncing}
              onSync={handleSync}
              collapsed={sidebarCollapsed}
              onToggleCollapse={() => setSidebarCollapsed(!sidebarCollapsed)}
            />
          )}
        </div>

        <div
          className={`research-annotations-column ${darkMode ? "dark" : ""}`}
        >
          {hasAnnotations ? (
            <div className="research-section">
              <h3>
                {selectedChapterData
                  ? `${selectedChapterData.title} (${displayAnnotations.length})`
                  : `Annotations (${displayAnnotations.length})`}
              </h3>
              <div className="research-annotations">
                {groupAnnotationsByPage(displayAnnotations).map((group) => (
                  <div key={group.page} className="research-page-group">
                    <div className="research-page-header">
                      <span className="research-page-number">
                        {group.page === "No Page"
                          ? "No Page"
                          : `Page ${group.page}`}
                      </span>
                    </div>
                    {group.annotations.map((ann) => (
                      <AnnotationItem key={ann.id} annotation={ann} />
                    ))}
                  </div>
                ))}
                {displayAnnotations.length === 0 && (
                  <p className="research-empty">
                    No annotations in this chapter.
                  </p>
                )}
              </div>
            </div>
          ) : (
            <p className="research-empty">No annotations for this resource.</p>
          )}
        </div>

        <div className="research-notes-column">
          <div className="research-notes-container">
            <h3>Notes {hasNotes && `(${data.notes.length})`}</h3>
            {hasNotes ? (
              <div className="research-notes">
                {data.notes.map((note) => (
                  <div key={note.id} className="research-note">
                    <div dangerouslySetInnerHTML={{ __html: note.content }} />
                  </div>
                ))}
              </div>
            ) : (
              <p className="research-notes-empty">No notes</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
