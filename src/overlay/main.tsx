import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./overlay.css";

type Kind = "listening" | "transcribing" | "polishing" | "inserted" | "notice" | "error";
type Size = "pill" | "card" | "large";
type Mode = "off" | "clean" | "rewrite";

interface StatusEvent {
  kind: Kind;
  message: string;
}

interface Context {
  size: Size;
  mode: Mode;
  ai_configured: boolean;
  app_name: string | null;
  app_icon: string | null;
}

const MAX_CHARS: Record<Size, number> = { pill: 0, card: 260, large: 560 };
const BARS: Record<Size, number> = { pill: 9, card: 13, large: 17 };

/** Keep the end of a long transcript: that's what the user just said. */
export function tail(text: string, max: number): string {
  if (text.length <= max) return text;
  const cut = text.slice(text.length - max);
  const space = cut.indexOf(" ");
  return "…" + (space >= 0 && space < 30 ? cut.slice(space + 1) : cut);
}

/** A bar per recent level sample, newest in the middle, fading outwards. */
function Waveform({ levels, active }: { levels: number[]; active: boolean }) {
  const n = levels.length;
  const mid = (n - 1) / 2;
  return (
    <div className={`waveform ${active ? "" : "idle"}`} aria-hidden="true">
      {levels.map((_, i) => {
        // Mirror the history around the centre so speech "pulses" outward.
        const distance = Math.abs(i - mid);
        const level = levels[n - 1 - Math.round(distance)] ?? 0;
        const falloff = 1 - (distance / (mid + 1)) * 0.45;
        const height = active ? Math.max(0.12, level * falloff) : 0.12;
        return <span key={i} style={{ transform: `scaleY(${height})` }} />;
      })}
    </div>
  );
}

function StatusIcon({ kind }: { kind: Kind }) {
  switch (kind) {
    case "transcribing":
    case "polishing":
      return <span className="spinner" aria-hidden="true" />;
    case "inserted":
      return <span className="icon ok">✓</span>;
    case "error":
      return <span className="icon warn">!</span>;
    case "notice":
      return <span className="icon info">i</span>;
    default:
      return <span className="rec-dot" aria-hidden="true" />;
  }
}

const MODES: [Mode, string][] = [
  ["off", "Raw"],
  ["clean", "Clean"],
  ["rewrite", "Rewrite"],
];

function Overlay() {
  const [status, setStatus] = useState<StatusEvent>({ kind: "listening", message: "Listening" });
  const [text, setText] = useState("");
  const [context, setContext] = useState<Context>({
    size: "card",
    mode: "off",
    ai_configured: false,
    app_name: null,
    app_icon: null,
  });
  const [levels, setLevels] = useState<number[]>(() => new Array(BARS.card).fill(0));
  const size = useRef<Size>("card");

  useEffect(() => {
    const unlisten = [
      listen<StatusEvent>("overlay", (e) => setStatus(e.payload)),
      listen<string>("overlay-text", (e) => setText(e.payload)),
      listen<Context>("overlay-context", (e) => {
        size.current = e.payload.size;
        setContext(e.payload);
        setLevels(new Array(BARS[e.payload.size]).fill(0));
      }),
      listen<number>("overlay-level", (e) =>
        setLevels((prev) => [...prev.slice(1), e.payload]),
      ),
    ];
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
    };
  }, []);

  const recording = status.kind === "listening";
  const busy = recording || status.kind === "transcribing" || status.kind === "polishing";
  const setMode = (mode: Mode) => {
    setContext((c) => ({ ...c, mode }));
    invoke("overlay_set_mode", { mode }).catch(() => undefined);
  };

  if (context.size === "pill") {
    return (
      <div className="frame">
        <div className={`pill ${status.kind}`} role="status" aria-live="polite">
          {recording ? <Waveform levels={levels} active /> : <StatusIcon kind={status.kind} />}
          <span className="message">{status.message}</span>
        </div>
      </div>
    );
  }

  const shown = tail(text, MAX_CHARS[context.size]);
  return (
    <div className="frame">
      <div className={`card ${context.size} ${status.kind}`} role="status" aria-live="polite">
        <div className="card-top">
          <div className="target">
            {context.app_icon ? (
              <img src={context.app_icon} alt="" width={18} height={18} />
            ) : (
              <span className="target-dot" />
            )}
            <span className="target-name">{context.app_name ?? "Dictation"}</span>
          </div>
          <div className="controls">
            {context.ai_configured && busy && (
              <div className="modes" role="radiogroup" aria-label="Writing mode">
                {MODES.map(([mode, label]) => (
                  <button
                    key={mode}
                    role="radio"
                    aria-checked={context.mode === mode}
                    className={context.mode === mode ? "active" : ""}
                    onClick={() => setMode(mode)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
            <button className="icon-button" title="Settings" aria-label="Settings" onClick={() => invoke("overlay_open_settings")}>
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                <circle cx="12" cy="12" r="3" />
                <path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
              </svg>
            </button>
            {busy && (
              <button className="icon-button" title="Cancel (Esc)" aria-label="Cancel" onClick={() => invoke("overlay_cancel")}>
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round">
                  <path d="M6 6l12 12M18 6 6 18" />
                </svg>
              </button>
            )}
          </div>
        </div>

        <div className={`transcript ${shown ? "" : "empty"}`}>
          <span>{shown || (recording ? "Start speaking…" : "")}</span>
        </div>

        <div className="card-bottom">
          <Waveform levels={levels} active={recording} />
          <div className="status">
            {!recording && <StatusIcon kind={status.kind} />}
            <span className="message">{status.message}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
