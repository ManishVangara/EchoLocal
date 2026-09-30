import { api, type Snapshot } from "../../api";
import type { AppState } from "../useAppState";
import { Keycap } from "../components/Controls";
import type { Page } from "../App";

function Readiness({ snapshot, go }: { snapshot: Snapshot; go: (page: Page) => void }) {
  const model = snapshot.models.find((m) => m.selected);
  const problems: { text: string; action: string; onClick: () => void }[] = [];
  if (snapshot.microphone !== "granted") {
    problems.push({
      text: "Microphone access is needed.",
      action: snapshot.microphone === "denied" ? "Open Settings" : "Allow",
      onClick: () => (snapshot.microphone === "denied" ? api.openMicrophoneSettings() : api.requestMicrophone()),
    });
  }
  if (!snapshot.accessibility) {
    problems.push({ text: "Accessibility access is needed.", action: "Allow", onClick: () => api.requestAccessibility() });
  }
  if (!model?.downloaded) {
    problems.push({
      text: model?.downloading ? "The speech model is downloading…" : "Download a speech model to start.",
      action: "Speech Model",
      onClick: () => go("model"),
    });
  }
  if (snapshot.engine.status === "failed") {
    problems.push({ text: `The speech model couldn't load: ${snapshot.engine.error}`, action: "Speech Model", onClick: () => go("model") });
  }
  if (snapshot.hotkey.state === "error") {
    problems.push({ text: snapshot.hotkey.detail, action: "Shortcut", onClick: () => go("shortcut") });
  }

  if (problems.length > 0) {
    return (
      <div className="hero warn">
        <h2>Almost ready</h2>
        <ul className="problems">
          {problems.map((p) => (
            <li key={p.text}>
              <span>{p.text}</span>
              <button className="small" onClick={p.onClick}>
                {p.action}
              </button>
            </li>
          ))}
        </ul>
      </div>
    );
  }

  const loading = snapshot.engine.status === "loading";
  return (
    <div className="hero ready">
      <div className="hero-label">{loading ? "Loading speech model…" : "Ready to dictate"}</div>
      <div className="hero-instruction">
        Hold <Keycap label={snapshot.shortcut_label} /> and speak. Release to insert.
      </div>
      <div className="hero-meta">
        {model?.name} · {model?.subtitle}
        {snapshot.engine.status === "unloaded" && " · reloads as you speak"}
      </div>
    </div>
  );
}

export function HomePage({ app, go }: { app: AppState; go: (page: Page) => void }) {
  const { snapshot } = app;
  if (!snapshot) return null;
  const { stats } = snapshot;
  return (
    <div className="page">
      <Readiness snapshot={snapshot} go={go} />

      <section className="try">
        <h3 className="group-title">Try it here</h3>
        <textarea
          className="try-area"
          rows={4}
          placeholder={`Click here, hold ${snapshot.shortcut_label} and say something…`}
        />
      </section>

      <div className="stats">
        <div className="stat">
          <div className="stat-value">{stats.dictations}</div>
          <div className="stat-label">Dictations this session</div>
        </div>
        <div className="stat">
          <div className="stat-value">{stats.words}</div>
          <div className="stat-label">Words</div>
        </div>
        <div className="stat">
          <div className="stat-value">
            {stats.last_release_to_text_ms != null ? `${(stats.last_release_to_text_ms / 1000).toFixed(2)} s` : "–"}
          </div>
          <div className="stat-label">Last release → text</div>
        </div>
      </div>
    </div>
  );
}
