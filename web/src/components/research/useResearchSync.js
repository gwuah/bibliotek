import { useState } from "react";
import { importResearch } from "../../api.js";

const CONFIRM_EMPTY =
  "The Research database holds nothing now. Importing it will remove every Research highlight, comment and note from the library. Import anyway?";

export function useResearchSync(onSynced) {
  const [syncing, setSyncing] = useState(false);
  const [stats, setStats] = useState(null);
  const [error, setError] = useState(null);

  const run = async (confirmEmpty) => {
    try {
      return await importResearch({ confirmEmpty });
    } catch (err) {
      if (err.code === "empty_snapshot" && !confirmEmpty && window.confirm(CONFIRM_EMPTY)) {
        return run(true);
      }
      throw err;
    }
  };

  const sync = async () => {
    setSyncing(true);
    setStats(null);
    setError(null);
    try {
      setStats(await run(false));
      await onSynced();
    } catch (err) {
      setError(err.message);
    } finally {
      setSyncing(false);
    }
  };

  return { syncing, stats, error, sync };
}
