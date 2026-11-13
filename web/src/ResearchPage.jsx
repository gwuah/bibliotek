import React, { useState, useEffect } from "react";
import ConfigPanel from "./components/research/ConfigPanel.jsx";
import SyncStats from "./components/research/SyncStats.jsx";
import ResourceDetail from "./components/research/ResourceDetail.jsx";
import ResourceList from "./components/research/ResourceList.jsx";
import { listReading } from "./reading.js";
import { useResearchSync } from "./components/research/useResearchSync.js";

export default function ResearchPage({ documentId }) {
  const [resources, setResources] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const research = useResearchSync(() => loadData());

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      setLoading(true);
      setError(null);
      setResources(await listReading());
    } catch (err) {
      console.error("Failed to load data:", err);
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  if (documentId) {
    return (
      <div className="research-container">
        <ResourceDetail documentId={documentId} />
      </div>
    );
  }

  if (loading) {
    return (
      <div className="research-container">
        <div className="research-loading">Loading...</div>
      </div>
    );
  }

  return (
    <div className="research-container">
      <div className="research-sidebar">
        <ConfigPanel research={research} />

        {(error || research.error) && (
          <p className="research-error">{error || research.error}</p>
        )}

        <SyncStats stats={research.stats} />
      </div>

      <div className="research-main">
        <h2 className="research-heading">Resources ({resources.length})</h2>
        <ResourceList resources={resources} />
      </div>
    </div>
  );
}
