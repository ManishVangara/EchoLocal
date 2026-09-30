import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type AiSettings,
  type DownloadProgress,
  type ModelId,
  type ModelUnload,
  type ModelView,
  type PostProcessing,
  type Settings,
  type Snapshot,
} from "../api";
import { acceleratorFromEvent, formatAccelerator } from "../shortcut";

export default function App() {
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
    // Permissions are granted in System Settings; re-check on return.
    window.addEventListener("focus", refresh);
    return () => {
      unlisten.forEach((p) => p.then((f) => f()));
      window.removeEventListener("focus", refresh);
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

  if (!snapshot) {
    return <main className="loading">Loading…</main>;
  }
  const { settings } = snapshot;
  const update = (patch: Partial<Settings>) => save({ ...settings, ...patch });

  return (
    <main>
      <header>
        <h1>EchoLocal</h1>
        <StatusLine snapshot={snapshot} />
      </header>

      {!snapshot.accessibility && <PermissionCard />}
      {error && (
        <div className="banner error" role="alert">
          {error}
        </div>
      )}

      <section>
        <h2>Speech Model</h2>
        <div className="models" role="radiogroup">
          {snapshot.models.map((model) => (
            <ModelRow
              key={model.id}
              model={model}
              progress={progress[model.id]}
              error={downloadErrors[model.id]}
              onSelect={() => update({ model: model.id })}
              onDownload={() => {
                setDownloadErrors((d) => ({ ...d, [model.id]: undefined }));
                api.downloadModel(model.id);
              }}
              onCancel={() => api.cancelDownload(model.id)}
              onDelete={() => api.deleteModel(model.id).catch((e) => setError(String(e)))}
            />
          ))}
        </div>
      </section>

      <section>
        <h2>Push-to-Talk Shortcut</h2>
        <ShortcutField
          value={settings.shortcut}
          error={snapshot.shortcut_error}
          onChange={(shortcut) => update({ shortcut })}
        />
        <p className="hint">Hold to dictate, release to insert. Press Esc while dictating to cancel.</p>
      </section>

      <section>
        <h2>Post-Processing</h2>
        <Segmented<PostProcessing>
          value={settings.post_processing}
          options={[
            ["off", "Off"],
            ["clean", "Clean"],
            ["rewrite", "Rewrite"],
          ]}
          onChange={(post_processing) => update({ post_processing })}
        />
        <p className="hint">
          {settings.post_processing === "off" && "Text is inserted exactly as heard."}
          {settings.post_processing === "clean" && "Fixes punctuation and removes filler words, keeping your wording."}
          {settings.post_processing === "rewrite" && "Rewrites your words following the instruction below."}
        </p>
        {settings.post_processing !== "off" && (
          <AiFields ai={settings.ai} mode={settings.post_processing} onSave={(ai) => update({ ai })} />
        )}
      </section>

      <section>
        <h2>General</h2>
        <Toggle
          label="Launch at login"
          checked={settings.launch_at_login}
          onChange={(launch_at_login) => update({ launch_at_login })}
        />
        <Toggle
          label="Trim silence"
          detail="Skip pauses before and after speaking"
          checked={settings.trim_silence}
          onChange={(trim_silence) => update({ trim_silence })}
        />
        <label className="toggle">
          <span>
            Free model memory when idle
            <span className="small block">Reloads automatically while you speak</span>
          </span>
          <select
            value={settings.unload_model}
            onChange={(e) => update({ unload_model: e.target.value as ModelUnload })}
          >
            <option value="after5_minutes">After 5 minutes</option>
            <option value="after15_minutes">After 15 minutes</option>
            <option value="after1_hour">After 1 hour</option>
            <option value="never">Never</option>
          </select>
        </label>
        <MicrophoneField value={settings.microphone} onChange={(microphone) => update({ microphone })} />
      </section>

      <footer>
        Speech never leaves this Mac.
        {settings.post_processing !== "off" && " With post-processing on, only the transcript text is sent to your AI server."}
      </footer>
    </main>
  );
}

function StatusLine({ snapshot }: { snapshot: Snapshot }) {
  const { engine, settings } = snapshot;
  const hint = `Hold ${formatAccelerator(settings.shortcut)} and speak`;
  let text: string;
  let tone = "ok";
  switch (engine.status) {
    case "ready":
      text = `Ready — ${hint}`;
      break;
    case "unloaded":
      text = `Ready — ${hint} (model reloads as you speak)`;
      break;
    case "loading":
      text = "Loading speech model…";
      tone = "busy";
      break;
    case "failed":
      text = `Couldn't load the speech model: ${engine.error}`;
      tone = "bad";
      break;
    default:
      text = "Download a speech model to start";
      tone = "bad";
  }
  return <p className={`status ${tone}`}>{text}</p>;
}

function PermissionCard() {
  return (
    <div className="banner warn">
      <strong>Allow Accessibility access</strong>
      <p>EchoLocal needs it to type the transcript into the app you're using.</p>
      <div className="row">
        <button className="primary" onClick={() => api.requestAccessibility()}>
          Allow…
        </button>
        <button onClick={() => api.openAccessibilitySettings()}>Open System Settings</button>
      </div>
    </div>
  );
}

function formatMB(bytes: number) {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function ModelRow(props: {
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
  const pct = Math.floor((have / model.total_bytes) * 100);
  return (
    <div className={`model ${model.selected ? "selected" : ""}`}>
      <label className="model-main">
        <input type="radio" name="model" checked={model.selected} onChange={props.onSelect} />
        <span>
          <span className="model-name">{model.name}</span>
          <span className="model-subtitle">{model.subtitle}</span>
        </span>
      </label>
      <div className="model-action">
        {model.downloaded ? (
          <button className="link" onClick={props.onDelete} title="Remove the downloaded model">
            Remove
          </button>
        ) : model.downloading ? (
          <>
            <progress max={100} value={pct} aria-label={`${model.name} download`} />
            <span className="small">
              {formatMB(have)} / {model.size_label}
            </span>
            <button className="link" onClick={props.onCancel}>
              Cancel
            </button>
          </>
        ) : (
          <button onClick={props.onDownload}>
            {model.progress_bytes > 0 ? "Resume" : "Download"} · {model.size_label}
          </button>
        )}
      </div>
      {props.error && <p className="model-error">{props.error}</p>}
    </div>
  );
}

function ShortcutField(props: { value: string; error: string | null; onChange: (value: string) => Promise<boolean> }) {
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape" && !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
        setRecording(false);
        setHint(null);
        return;
      }
      const result = acceleratorFromEvent(e);
      if (!result) return;
      if ("error" in result) {
        setHint(result.error);
        return;
      }
      setRecording(false);
      setHint(null);
      props.onChange(result.accelerator);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, props]);

  return (
    <div>
      <div className="row">
        <kbd className={`shortcut ${recording ? "recording" : ""}`}>
          {recording ? "Press a shortcut…" : formatAccelerator(props.value)}
        </kbd>
        <button onClick={() => setRecording(!recording)}>{recording ? "Cancel" : "Change"}</button>
      </div>
      {(hint || props.error) && <p className="field-error">{hint ?? props.error}</p>}
    </div>
  );
}

function Segmented<T extends string>(props: { value: T; options: [T, string][]; onChange: (value: T) => void }) {
  return (
    <div className="segmented" role="radiogroup">
      {props.options.map(([value, label]) => (
        <button
          key={value}
          role="radio"
          aria-checked={props.value === value}
          className={props.value === value ? "active" : ""}
          onClick={() => props.onChange(value)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function AiFields(props: { ai: AiSettings; mode: PostProcessing; onSave: (ai: AiSettings) => Promise<boolean> }) {
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
    <div className="ai">
      <label>
        Server URL
        <input type="url" placeholder="http://localhost:11434/v1" spellCheck={false} {...field("base_url")} />
      </label>
      <label>
        Model
        <input placeholder="e.g. llama3.2" spellCheck={false} {...field("model")} />
      </label>
      <label>
        API key <span className="small">(optional for local servers)</span>
        <input type="password" autoComplete="off" {...field("api_key")} />
      </label>
      {props.mode === "rewrite" && (
        <label>
          Instruction
          <textarea rows={2} {...field("rewrite_instruction")} />
        </label>
      )}
      <div className="row">
        <button onClick={runTest} disabled={testing}>
          {testing ? "Testing…" : "Test"}
        </button>
        <span className="small">Works with Ollama, LM Studio and OpenAI-compatible APIs.</span>
      </div>
      {test && <p className={test.ok ? "test-ok" : "field-error"}>{test.text}</p>}
    </div>
  );
}

function Toggle(props: { label: string; detail?: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="toggle">
      <span>
        {props.label}
        {props.detail && <span className="small block">{props.detail}</span>}
      </span>
      <input type="checkbox" role="switch" checked={props.checked} onChange={(e) => props.onChange(e.target.checked)} />
    </label>
  );
}

function MicrophoneField(props: { value: string | null; onChange: (value: string | null) => void }) {
  const [devices, setDevices] = useState<string[]>([]);
  useEffect(() => {
    api.listMicrophones().then(setDevices).catch(() => setDevices([]));
  }, []);
  const options = props.value && !devices.includes(props.value) ? [...devices, props.value] : devices;
  return (
    <label className="toggle">
      <span>Microphone</span>
      <select value={props.value ?? ""} onChange={(e) => props.onChange(e.target.value || null)}>
        <option value="">System default</option>
        {options.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}
