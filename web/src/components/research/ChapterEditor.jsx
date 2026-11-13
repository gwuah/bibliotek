import React, { useState } from "react";

const blankRow = () => ({ title: "", start_page: "", end_page: "" });

const toRows = (sections) =>
  sections.map((s) => ({ title: s.title, start_page: String(s.start_page), end_page: String(s.end_page) }));

function toSections(rows) {
  return rows.map((row, i) => {
    const start = Number(row.start_page);
    const end = Number(row.end_page);
    if (!Number.isInteger(start) || !Number.isInteger(end) || row.start_page === "" || row.end_page === "") {
      throw new Error(`Chapter ${i + 1} needs whole start and end pages`);
    }
    return { title: row.title.trim(), start_page: start, end_page: end };
  });
}

export default function ChapterEditor({ document, onSave, onCancel, saving, saveError }) {
  const [rows, setRows] = useState(() => toRows(document.sections));
  const [offset, setOffset] = useState(String(document.page_offset));
  const [error, setError] = useState(null);

  const setRow = (index, field, value) =>
    setRows((prev) => prev.map((row, i) => (i === index ? { ...row, [field]: value } : row)));

  const handleSave = () => {
    setError(null);
    try {
      const pageOffset = Number(offset);
      if (offset === "" || !Number.isInteger(pageOffset)) {
        throw new Error("The page offset must be a whole number");
      }
      onSave({ sections: toSections(rows), page_offset: pageOffset });
    } catch (err) {
      setError(err.message);
    }
  };

  return (
    <div className="research-chapter-editor">
      <h4>Chapter Config</h4>
      <p className="research-chapter-help">PDF page = printed page + offset</p>
      <label className="research-chapter-offset-label">
        <span>Page offset</span>
        <input
          type="number"
          value={offset}
          onChange={(e) => setOffset(e.target.value)}
          className="research-chapter-offset-input"
        />
      </label>
      <div className="research-chapter-rows">
        {rows.map((row, i) => (
          <div key={i} className="research-chapter-row">
            <input
              value={row.title}
              onChange={(e) => setRow(i, "title", e.target.value)}
              placeholder="title"
              className="research-chapter-row-title"
            />
            <input
              type="number"
              value={row.start_page}
              onChange={(e) => setRow(i, "start_page", e.target.value)}
              placeholder="from"
              className="research-chapter-offset-input"
            />
            <input
              type="number"
              value={row.end_page}
              onChange={(e) => setRow(i, "end_page", e.target.value)}
              placeholder="to"
              className="research-chapter-offset-input"
            />
            <button
              onClick={() => setRows((prev) => prev.filter((_, j) => j !== i))}
              className="research-theme-toggle-small"
              title="Remove chapter"
            >
              ×
            </button>
          </div>
        ))}
      </div>
      <button
        onClick={() => setRows((prev) => [...prev, blankRow()])}
        className="research-btn research-btn-secondary research-chapter-edit-btn"
      >
        + Add chapter
      </button>
      {(error || saveError) && <p className="research-error">{error || saveError}</p>}
      <div className="research-chapter-actions">
        <button onClick={onCancel} className="research-btn research-btn-secondary">
          Cancel
        </button>
        <button onClick={handleSave} disabled={saving} className="research-btn research-btn-primary">
          {saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  );
}
