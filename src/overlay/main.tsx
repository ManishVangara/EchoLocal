import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

type Kind = "listening" | "transcribing" | "polishing" | "inserted" | "notice" | "error";

interface OverlayEvent {
  kind: Kind;
  message: string;
}

const ICONS: Partial<Record<Kind, string>> = { inserted: "✓", notice: "i", error: "!" };

function Overlay() {
  const [state, setState] = useState<OverlayEvent>({ kind: "listening", message: "Listening" });

  useEffect(() => {
    const unlisten = listen<OverlayEvent>("overlay", (e) => setState(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const icon = ICONS[state.kind];
  return (
    <div className={`pill ${state.kind}`} role="status" aria-live="polite">
      {icon ? <span className="icon">{icon}</span> : <span className="indicator" />}
      <span className="message">{state.message}</span>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
