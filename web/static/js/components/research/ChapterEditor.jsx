import React, { useState, useEffect } from "react";

export default function ChapterEditor({ config, onSave, onCancel, saving }) {
  const [json, setJson] = useState("");
  const [error, setError] = useState(null);

  useEffect(() => {
    const chapters = config?.chapters || {};
    setJson(JSON.stringify(chapters, null, 2));
  }, [config]);

  const handleSave = () => {
    setError(null);
    try {
      const parsed = JSON.parse(json);
      for (const [key, value] of Object.entries(parsed)) {
        if (isNaN(parseInt(key, 10))) {
          throw new Error(`Chapter key "${key}" must be an integer`);
        }
        if (!Array.isArray(value) || value.length !== 2) {
          throw new Error(`Chapter "${key}" must be [title, startPage]`);
        }
        if (typeof value[0] !== "string") {
          throw new Error(`Chapter "${key}" title must be a string`);
        }
        if (typeof value[1] !== "number") {
          throw new Error(`Chapter "${key}" startPage must be a number`);
        }
      }
      onSave({ chapters: parsed });
    } catch (err) {
      setError(err.message);
    }
  };

  return (
    <div className="research-chapter-editor">
      <h4>Chapter Config</h4>
      <p className="research-chapter-help">
        Chapters: {"{"}"1": ["Title", startPage], ...{"}"}
      </p>
      <textarea
        value={json}
        onChange={(e) => setJson(e.target.value)}
        className="research-chapter-textarea"
        rows={10}
        spellCheck={false}
      />
      {error && <p className="research-error">{error}</p>}
      <div className="research-chapter-actions">
        <button
          onClick={onCancel}
          className="research-btn research-btn-secondary"
        >
          Cancel
        </button>
        <button
          onClick={handleSave}
          disabled={saving}
          className="research-btn research-btn-primary"
        >
          {saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  );
}
