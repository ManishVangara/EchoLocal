import { useState } from "react";
import type { AppState } from "./useAppState";
import { Keycap } from "./components/Controls";
import { ModelPicker, Permissions, ShortcutRecorder } from "./components/Features";

const STEPS = ["Welcome", "Permissions", "Speech model", "Shortcut", "Try it"] as const;

export function Onboarding({ app, onDone }: { app: AppState; onDone: () => void }) {
  const [step, setStep] = useState(0);
  const { snapshot } = app;
  if (!snapshot) return null;

  const model = snapshot.models.find((m) => m.selected);
  const canContinue = [
    true,
    snapshot.microphone === "granted" && snapshot.accessibility,
    !!model?.downloaded,
    true,
    true,
  ][step];
  const next = () => (step === STEPS.length - 1 ? onDone() : setStep(step + 1));

  return (
    <div className="onboarding">
      <div className="drag-strip" data-tauri-drag-region />
      <ol className="steps" aria-label="Setup progress">
        {STEPS.map((name, i) => (
          <li key={name} className={i === step ? "current" : i < step ? "done" : ""}>
            <span className="dot" />
            {name}
          </li>
        ))}
      </ol>

      <div className="onboarding-body">
        {step === 0 && (
          <div className="welcome">
            <img src="/app-icon.png" alt="" width={120} height={120} />
            <h1>
              Welcome to <span className="wordmark">EchoLocal</span>
            </h1>
            <p>
              Hold a key, speak, release — your words appear wherever you're typing. Everything is transcribed on this
              Mac, privately.
            </p>
          </div>
        )}
        {step === 1 && (
          <>
            <h1>Two permissions</h1>
            <p className="lead">EchoLocal needs to hear you and to type for you.</p>
            <Permissions app={app} />
          </>
        )}
        {step === 2 && (
          <>
            <h1>Choose a speech model</h1>
            <p className="lead">It downloads once and then works offline. You can switch later.</p>
            <ModelPicker app={app} />
          </>
        )}
        {step === 3 && (
          <>
            <h1>Your dictation shortcut</h1>
            <p className="lead">Hold it while you speak. Most people like holding Right ⌥.</p>
            <ShortcutRecorder app={app} />
          </>
        )}
        {step === 4 && (
          <>
            <h1>Try it</h1>
            <p className="lead">
              Click in the box, hold <Keycap label={snapshot.shortcut_label} />, say a sentence and let go.
            </p>
            <textarea className="try-area" rows={5} placeholder="Your words will appear here…" autoFocus />
            {snapshot.stats.dictations > 0 && <p className="success-text">It works. You're all set.</p>}
          </>
        )}
      </div>

      <footer className="onboarding-footer">
        {step > 0 ? (
          <button onClick={() => setStep(step - 1)}>Back</button>
        ) : (
          <span />
        )}
        <div className="footer-right">
          {!canContinue && step === 2 && model?.downloading && <span className="hint">Downloading…</span>}
          <button className="primary" disabled={!canContinue} onClick={next}>
            {step === 0 ? "Get Started" : step === STEPS.length - 1 ? "Done" : "Continue"}
          </button>
        </div>
      </footer>
    </div>
  );
}
