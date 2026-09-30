import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type DownloadProgress, type ModelId, type Settings, type Snapshot } from "../api";

/** Live app state from Rust, refreshed on change events and window focus. */
export function useAppState() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [progress, setProgress] = useState<Partial<Record<ModelId, DownloadProgress>>>({});
  const [downloadErrors, setDownloadErrors] = useState<Partial<Record<ModelId, string>>>({});
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    api.getState().then(setSnapshot).catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = [
      listen("state-changed", refresh),
      listen<DownloadProgress>("download-progress", (e) =>
        setProgress((p) => ({ ...p, [e.payload.id]: e.payload })),
      ),
      listen<{ id: ModelId; error: string }>("download-failed", (e) =>
        setDownloadErrors((d) => ({ ...d, [e.payload.id]: e.payload.error })),
      ),
    ];
    // Permissions are granted in System Settings; re-check on return, and
    // poll while something is missing (focus events aren't always delivered).
    window.addEventListener("focus", refresh);
    const poll = window.setInterval(refresh, 2000);
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
      window.removeEventListener("focus", refresh);
      window.clearInterval(poll);
    };
  }, [refresh]);

  const save = useCallback(async (settings: Settings) => {
    try {
      setSnapshot(await api.saveSettings(settings));
      setError(null);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }, []);

  const download = useCallback((id: ModelId) => {
    setDownloadErrors((d) => ({ ...d, [id]: undefined }));
    api.downloadModel(id).catch((e) => setError(String(e)));
  }, []);

  return { snapshot, progress, downloadErrors, error, setError, save, download, refresh };
}

export type AppState = ReturnType<typeof useAppState>;
