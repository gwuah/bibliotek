import React, { useState, useEffect } from "react";
import ChapterEditor from "./ChapterEditor.jsx";
import ChapterSidebar from "./ChapterSidebar.jsx";
import Paragraphs from "./Paragraphs.jsx";
import { useResearchSync } from "./useResearchSync.js";
import { getDocument, updateDocument } from "../../api.js";
import { cameFrom, navigate } from "../../navigation.js";
import {
  chaptersOf,
  groupByPage,
  highlightsInChapter,
  unchapteredHighlights,
} from "../../sections.js";

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
                  <Paragraphs text={comment.body} />
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function getChapterAnnotationCounts(annotations, chapters, pageOffset) {
  const counts = {};
  for (const chapter of chapters) {
    counts[chapter.key] = highlightsInChapter(annotations, chapter, pageOffset).length;
  }
  return counts;
}

function getChapterFromHash() {
  const hash = window.location.hash;
  if (hash && hash.startsWith("#chapter-")) {
    const val = parseInt(hash.replace("#chapter-", ""), 10);
    return isNaN(val) ? null : val;
  }
  return null;
}

const backToList = () => navigate(cameFrom() ?? "/reading");

export default function ResourceDetail({ documentId }) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [savingConfig, setSavingConfig] = useState(false);
  const [configError, setConfigError] = useState(null);
  const [editingChapters, setEditingChapters] = useState(false);
  const [selectedChapter, setSelectedChapter] = useState(getChapterFromHash);
  const [darkMode, setDarkMode] = useState(() => {
    try {
      return localStorage.getItem("research-dark-mode") === "true";
    } catch {
      return false;
    }
  });
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [excludeChaptered, setExcludeChaptered] = useState(false);
  const [error, setError] = useState(null);

  const research = useResearchSync(() => loadResourceFull());

  useEffect(() => {
    try {
      localStorage.setItem("research-dark-mode", darkMode);
    } catch {}
  }, [darkMode]);

  useEffect(() => {
    window.history.replaceState(
      window.history.state,
      "",
      selectedChapter ? `#chapter-${selectedChapter}` : window.location.pathname,
    );
  }, [selectedChapter]);

  useEffect(() => {
    loadResourceFull();
  }, [documentId]);

  const loadResourceFull = async () => {
    try {
      setLoading(true);
      setError(null);
      const document = await getDocument(documentId);
      setData(document);
      const chapters = chaptersOf(document.sections);
      const hashChapter = getChapterFromHash();
      if (hashChapter && chapters.some((c) => c.key === hashChapter)) {
        setSelectedChapter(hashChapter);
      }
    } catch (err) {
      setError(err.status === 404 ? "Resource not found" : err.message);
    } finally {
      setLoading(false);
    }
  };

  const handleSaveConfig = async (changes) => {
    setSavingConfig(true);
    setConfigError(null);
    try {
      const document = await updateDocument(documentId, changes);
      setData(document);
      setEditingChapters(false);
      const chapters = chaptersOf(document.sections);
      setSelectedChapter(chapters.length > 0 ? chapters[0].key : null);
    } catch (err) {
      setConfigError(err.message);
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
        <button onClick={backToList} className="research-back-btn">
          ← Back to list
        </button>
        <p className="research-error">{error}</p>
      </div>
    );
  }

  const pageOffset = data.page_offset;
  const hasNotes = data.notes.length > 0;
  const hasAnnotations = data.highlights.length > 0;
  const chapters = chaptersOf(data.sections);
  const hasChapters = chapters.length > 0;
  const annotationCounts = hasAnnotations
    ? getChapterAnnotationCounts(data.highlights, chapters, pageOffset)
    : {};

  const selectedChapterData =
    hasChapters && selectedChapter != null
      ? chapters.find((c) => c.key === selectedChapter)
      : null;
  const allAnnotations = data.highlights;
  const displayAnnotations = selectedChapterData
    ? highlightsInChapter(allAnnotations, selectedChapterData, pageOffset)
    : excludeChaptered && hasChapters
      ? unchapteredHighlights(allAnnotations, chapters, pageOffset)
      : allAnnotations;

  return (
    <div className="research-detail">
      <div className="research-detail-header">
        <button onClick={backToList} className="research-back-btn">
          ← Back to list
        </button>
        <span className="research-detail-header-sep">|</span>
        <h2 className="research-detail-title">{data.title}</h2>
      </div>

      <div className="research-detail-layout">
        <div
          className={`research-toc-column ${sidebarCollapsed ? "collapsed" : ""}`}
        >
          {editingChapters ? (
            <ChapterEditor
              document={data}
              onSave={handleSaveConfig}
              onCancel={() => {
                setEditingChapters(false);
                setConfigError(null);
              }}
              saving={savingConfig}
              saveError={configError}
            />
          ) : (
            <ChapterSidebar
              chapters={chapters}
              annotationCounts={annotationCounts}
              totalAnnotations={allAnnotations.length}
              unchapteredCount={
                hasChapters
                  ? unchapteredHighlights(allAnnotations, chapters, pageOffset).length
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
              syncing={research.syncing}
              onSync={research.sync}
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
                {groupByPage(displayAnnotations, pageOffset).map((group) => (
                  <div key={group.page ?? "none"} className="research-page-group">
                    <div className="research-page-header">
                      <span className="research-page-number">
                        {group.page === null ? "No Page" : `Page ${group.page}`}
                      </span>
                    </div>
                    {group.highlights.map((ann) => (
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
                    <Paragraphs text={note.body} />
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
