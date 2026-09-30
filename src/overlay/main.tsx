import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

type Kind = "listening" | "transcribing" | "polishing" | "inserted" | "notice" | "error";

interface OverlayEvent {
  kind: Kind;
  message: string;
}

const MAX_CHARS = 160;

/** Keep the end of a long transcript: that's what the user just said. */
export function tail(text: string): string {
  if (text.length <= MAX_CHARS) return text;
  const cut = text.slice(text.length - MAX_CHARS);
  const space = cut.indexOf(" ");
  return "…" + (space >= 0 && space < 30 ? cut.slice(space + 1) : cut);
}

function Bars() {
  return (
    <span className="bars" aria-hidden="true">
      {[0, 1, 2, 3, 4].map((i) => (
        <span key={i} style={{ animationDelay: `${i * 0.12}s` }} />
      ))}
    </span>
  );
}

function Indicator({ kind }: { kind: Kind }) {
  switch (kind) {
    case "listening":
      return <Bars />;
    case "transcribing":
    case "polishing":
      return <span className="spinner" aria-hidden="true" />;
    case "inserted":
      return <span className="icon ok">✓</span>;
    case "error":
      return <span className="icon warn">!</span>;
    default:
      return <span className="icon info">i</span>;
  }
}

function Overlay() {
  const [state, setState] = useState<OverlayEvent>({ kind: "listening", message: "Listening" });
  const [text, setText] = useState("");

  useEffect(() => {
    const unlisten = [
      listen<OverlayEvent>("overlay", (e) => setState(e.payload)),
      listen<string>("overlay-text", (e) => setText(e.payload)),
    ];
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
    };
  }, []);

  return (
    <div className="stack">
      {text && (
        <div className="transcript" aria-live="polite">
          {tail(text)}
        </div>
      )}
      <div className={`pill ${state.kind}`} role="status" aria-live="polite">
        <Indicator kind={state.kind} />
        <span className="message">{state.message}</span>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
