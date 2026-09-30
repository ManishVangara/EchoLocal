// Feature widgets shared by onboarding and the settings pages.

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type AiSettings,
  type CaptureEvent,
  type DownloadProgress,
  type ModelView,
  type Permission,
  type PostProcessing,
} from "../../api";
import type { AppState } from "../useAppState";
import { Keycap } from "./Controls";
import { CheckIcon, HandIcon, MicIcon } from "./Icons";

function mb(bytes: number) {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function ModelCard(props: {
  model: ModelView;
  progress?: DownloadProgress;
  error?: string;
  onSelect: () => void;
  onDownload: () => void;
  onCancel: () => void;
  onDelete: () => void;
}) {
  const { model } = props;
  const have = props.progress?.downloaded ?? model.progress_bytes;
  const pct = Math.min(100, Math.floor((have / model.total_bytes) * 100));
  return (
    <div
      className={`model-card ${model.selected ? "selected" : ""}`}
      role="radio"
      aria-checked={model.selected}
      tabIndex={0}
      onClick={props.onSelect}
      onKeyDown={(e) => (e.key === " " || e.key === "Enter") && props.onSelect()}
    >
      <div className="model-radio">{model.selected && <span />}</div>
      <div className="model-text">
        <div className="model-name">{model.name}</div>
        <div className="model-subtitle">{model.subtitle}</div>
        {model.downloading && (
          <div className="model-progress">
            <div className="bar">
              <div style={{ width: `${pct}%` }} />
            </div>
            <span>
              {mb(have)} of {model.size_label}
            </span>
          </div>
        )}
        {props.error && <div className="model-error">{props.error}</div>}
      </div>
      <div className="model-action" onClick={(e) => e.stopPropagation()}>
        {model.downloaded ? (
          <>
            <span className="badge">
              <CheckIcon size={13} /> Downloaded
            </span>
            <button className="plain" onClick={props.onDelete} title="Remove the downloaded model">
              Remove
            </button>
          </>
        ) : model.downloading ? (
          <button className="plain" onClick={props.onCancel}>
            Cancel
          </button>
        ) : (
          <button className="primary small" onClick={props.onDownload}>
            {model.progress_bytes > 0 ? "Resume" : "Download"} · {model.size_label}
          </button>
        )}
      </div>
    </div>
  );
}

export function ModelPicker({ app }: { app: AppState }) {
  const { snapshot, progress, downloadErrors, save, download, setError } = app;
  if (!snapshot) return null;
  return (
    <div className="model-list" role="radiogroup" aria-label="Speech model">
      {snapshot.models.map((model) => (
        <ModelCard
          key={model.id}
          model={model}
          progress={progress[model.id]}
          error={downloadErrors[model.id]}
          onSelect={() => !model.selected && save({ ...snapshot.settings, model: model.id })}
          onDownload={() => download(model.id)}
          onCancel={() => api.cancelDownload(model.id)}
          onDelete={() => api.deleteModel(model.id).catch((e) => setError(String(e)))}
        />
      ))}
    </div>
  );
}

const PRESETS: [string, string][] = [
  ["OptRight", "Right ⌥"],
  ["CmdRight", "Right ⌘"],
  ["Fn", "fn"],
  ["Ctrl+Opt+Space", "⌃ ⌥ Space"],
];

/** Shows the current shortcut and records a new one from the real keyboard. */
export function ShortcutRecorder({ app, compact }: { app: AppState; compact?: boolean }) {
  const { snapshot, save } = app;
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<CaptureEvent>("shortcut-capture", async (e) => {
      const event = e.payload;
      if (event.state === "pending") {
        setLive(event.label);
      } else if (event.state === "invalid") {
        setMessage(event.message);
      } else {
        setRecording(false);
        setLive("");
        if (event.state === "done" && snapshot) {
          setMessage(null);
          await save({ ...snapshot.settings, shortcut: event.shortcut });
        }
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [snapshot, save]);

  // Stop listening if the view goes away mid-recording.
  useEffect(() => () => void api.stopShortcutCapture(), []);

  if (!snapshot) return null;
  const start = async () => {
    setMessage(null);
    setLive("");
    try {
      await api.startShortcutCapture();
      setRecording(true);
    } catch (e) {
      setMessage(String(e));
    }
  };
  const stop = () => {
    api.stopShortcutCapture();
    setRecording(false);
  };
  const status = snapshot.hotkey;

  return (
    <div className={`shortcut ${compact ? "compact" : ""}`}>
      <div className={`shortcut-display ${recording ? "recording" : ""}`}>
        {recording ? (
          live ? (
            <Keycap label={live} large />
          ) : (
            <span className="shortcut-prompt">Press your shortcut…</span>
          )
        ) : (
          <Keycap label={snapshot.shortcut_label} large />
        )}
      </div>
      <div className="shortcut-actions">
        {recording ? (
          <button onClick={stop}>Cancel</button>
        ) : (
          <button className="primary" onClick={start}>
            Change Shortcut
          </button>
        )}
      </div>
      <p className="hint center">
        {recording
          ? "Press a combination, or press and release a single modifier like Right ⌥. Esc cancels."
          : "Hold to dictate, release to insert. Press Esc while dictating to cancel."}
      </p>
      {message && <p className="error-text center">{message}</p>}
      {status.state === "needs_accessibility" && (
        <p className="warn-text center">The shortcut starts working once Accessibility access is allowed.</p>
      )}
      {status.state === "error" && <p className="error-text center">{status.detail}</p>}
      {!compact && !recording && (
        <div className="presets">
          <span className="hint">Quick picks:</span>
          {PRESETS.map(([value, label]) => (
            <button
              key={value}
              className={`chip ${snapshot.settings.shortcut === value ? "active" : ""}`}
              onClick={() => save({ ...snapshot.settings, shortcut: value })}
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function PermissionRow(props: {
  icon: React.ReactNode;
  title: string;
  why: string;
  granted: boolean;
  denied?: boolean;
  onGrant: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <div className={`permission ${props.granted ? "granted" : ""}`}>
      <div className="permission-icon">{props.icon}</div>
      <div className="permission-text">
        <div className="permission-title">{props.title}</div>
        <div className="permission-why">{props.why}</div>
      </div>
      {props.granted ? (
        <span className="badge success">
          <CheckIcon size={13} /> Allowed
        </span>
      ) : props.denied ? (
        <button onClick={props.onOpenSettings}>Open Settings</button>
      ) : (
        <button className="primary small" onClick={props.onGrant}>
          Allow…
        </button>
      )}
    </div>
  );
}

export function Permissions({ app }: { app: AppState }) {
  const { snapshot } = app;
  if (!snapshot) return null;
  const mic: Permission = snapshot.microphone;
  return (
    <div className="permissions">
      <PermissionRow
        icon={<MicIcon size={20} />}
        title="Microphone"
        why="To hear you while you hold the shortcut. Audio never leaves this Mac."
        granted={mic === "granted"}
        denied={mic === "denied"}
        onGrant={() => api.requestMicrophone()}
        onOpenSettings={() => api.openMicrophoneSettings()}
      />
      <PermissionRow
        icon={<HandIcon size={20} />}
        title="Accessibility"
        why="To detect the shortcut and type the text into the app you're using."
        granted={snapshot.accessibility}
        onGrant={() => api.requestAccessibility()}
        onOpenSettings={() => api.openAccessibilitySettings()}
      />
      {!snapshot.accessibility && (
        <p className="hint">
          After clicking Allow, turn on EchoLocal in the list that opens. If it's already on but not detected, turn it
          off and on again. <button className="link" onClick={() => api.openAccessibilitySettings()}>Open Accessibility settings</button>
        </p>
      )}
    </div>
  );
}

export function AiFields(props: { ai: AiSettings; mode: PostProcessing; onSave: (ai: AiSettings) => Promise<boolean> }) {
  const [draft, setDraft] = useState(props.ai);
  const [test, setTest] = useState<{ ok: boolean; text: string } | null>(null);
  const [testing, setTesting] = useState(false);
  useEffect(() => setDraft(props.ai), [props.ai]);

  const commit = () => {
    if (JSON.stringify(draft) !== JSON.stringify(props.ai)) props.onSave(draft);
  };
  const field = (key: keyof AiSettings) => ({
    value: draft[key],
    onChange: (e: { target: { value: string } }) => setDraft({ ...draft, [key]: e.target.value }),
    onBlur: commit,
  });
  const runTest = async () => {
    commit();
    setTesting(true);
    try {
      setTest({ ok: true, text: await api.testAi(draft, props.mode) });
    } catch (e) {
      setTest({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="form">
      <label>
        <span>Server URL</span>
        <input type="url" placeholder="http://localhost:11434/v1" spellCheck={false} {...field("base_url")} />
      </label>
      <label>
        <span>Model</span>
        <input placeholder="e.g. llama3.2" spellCheck={false} {...field("model")} />
      </label>
      <label>
        <span>
          API key <em>optional for local servers</em>
        </span>
        <input type="password" autoComplete="off" {...field("api_key")} />
      </label>
      {props.mode === "rewrite" && (
        <label>
          <span>Rewrite instruction</span>
          <textarea rows={2} {...field("rewrite_instruction")} />
        </label>
      )}
      <div className="form-actions">
        <button onClick={runTest} disabled={testing}>
          {testing ? "Testing…" : "Test with a sample sentence"}
        </button>
      </div>
      {test && <p className={test.ok ? "test-result" : "error-text"}>{test.text}</p>}
    </div>
  );
}

export function MicrophoneSelect(props: { value: string | null; onChange: (value: string | null) => void }) {
  const [devices, setDevices] = useState<string[]>([]);
  useEffect(() => {
    api.listMicrophones().then(setDevices).catch(() => setDevices([]));
  }, []);
  const options = props.value && !devices.includes(props.value) ? [...devices, props.value] : devices;
  return (
    <select value={props.value ?? ""} onChange={(e) => props.onChange(e.target.value || null)} aria-label="Microphone">
      <option value="">System default</option>
      {options.map((name) => (
        <option key={name} value={name}>
          {name}
        </option>
      ))}
    </select>
  );
}

