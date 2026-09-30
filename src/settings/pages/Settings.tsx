// The settings pages reachable from the sidebar.

import type { ModelUnload, OverlayPosition, OverlaySize, PostProcessing, Settings } from "../../api";
import type { AppState } from "../useAppState";
import { Group, PageHeader, Row, Segmented, Switch } from "../components/Controls";
import { AiFields, MicrophoneSelect, ModelPicker, Permissions, ShortcutRecorder } from "../components/Features";
import { LockIcon } from "../components/Icons";
import { useEffect, useState } from "react";

/** Saves when released, not on every step while dragging. */
function OffsetSlider(props: { value: number; onCommit: (value: number) => void }) {
  const [value, setValue] = useState(props.value);
  useEffect(() => setValue(props.value), [props.value]);
  const commit = () => value !== props.value && props.onCommit(value);
  return (
    <div className="slider">
      <input
        type="range"
        min={0}
        max={240}
        step={4}
        value={value}
        onChange={(e) => setValue(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        aria-label="Distance from edge"
      />
      <span>{value} pt</span>
    </div>
  );
}

function useUpdate(app: AppState) {
  return async (patch: Partial<Settings>) =>
    app.snapshot ? app.save({ ...app.snapshot.settings, ...patch }) : false;
}

export function ModelPage({ app }: { app: AppState }) {
  const update = useUpdate(app);
  if (!app.snapshot) return null;
  return (
    <div className="page">
      <PageHeader title="Speech Model" subtitle="Speech recognition runs entirely on this Mac." />
      <ModelPicker app={app} />
      <Group footer="The model reloads automatically while you speak, so freeing memory rarely costs any speed.">
        <Row label="Free memory when idle" detail="Unload the model after a while without dictating">
          <select
            value={app.snapshot.settings.unload_model}
            onChange={(e) => update({ unload_model: e.target.value as ModelUnload })}
            aria-label="Free memory when idle"
          >
            <option value="after5_minutes">After 5 minutes</option>
            <option value="after15_minutes">After 15 minutes</option>
            <option value="after1_hour">After 1 hour</option>
            <option value="never">Never</option>
          </select>
        </Row>
      </Group>
    </div>
  );
}

export function ShortcutPage({ app }: { app: AppState }) {
  return (
    <div className="page">
      <PageHeader title="Shortcut" subtitle="Hold it to dictate. Holding a single modifier like Right ⌥ works well." />
      <div className="panel">
        <ShortcutRecorder app={app} />
      </div>
    </div>
  );
}

const MODE_DETAIL: Record<PostProcessing, string> = {
  off: "Text is inserted exactly as heard.",
  clean: "Fixes punctuation and removes filler words, keeping your wording.",
  rewrite: "Rewrites what you said following your instruction.",
};

export function WritingPage({ app }: { app: AppState }) {
  const update = useUpdate(app);
  if (!app.snapshot) return null;
  const { settings } = app.snapshot;
  return (
    <div className="page">
      <PageHeader title="Writing" subtitle="Optionally polish the transcript with an AI model before it's inserted." />
      <Group>
        <Row label="Post-processing" detail={MODE_DETAIL[settings.post_processing]}>
          <Segmented<PostProcessing>
            label="Post-processing"
            value={settings.post_processing}
            options={[
              ["off", "Off"],
              ["clean", "Clean"],
              ["rewrite", "Rewrite"],
            ]}
            onChange={(post_processing) => update({ post_processing })}
          />
        </Row>
      </Group>
      {settings.post_processing !== "off" && (
        <Group
          title="AI server"
          footer={
            <>
              <LockIcon size={12} /> Only the transcript text is sent, never audio. Works with Ollama, LM Studio and
              OpenAI-compatible APIs.
            </>
          }
        >
          <div className="group-body">
            <AiFields ai={settings.ai} mode={settings.post_processing} onSave={(ai) => update({ ai })} />
          </div>
        </Group>
      )}
    </div>
  );
}

export function GeneralPage({ app }: { app: AppState }) {
  const update = useUpdate(app);
  if (!app.snapshot) return null;
  const { settings } = app.snapshot;
  return (
    <div className="page">
      <PageHeader title="General" />
      <Group>
        <Row label="Launch at login">
          <Switch
            label="Launch at login"
            checked={settings.launch_at_login}
            onChange={(launch_at_login) => update({ launch_at_login })}
          />
        </Row>
        <Row label="Show live transcript" detail="See the words while you speak">
          <Switch
            label="Show live transcript"
            checked={settings.live_preview}
            onChange={(live_preview) => update({ live_preview })}
          />
        </Row>
        <Row label="Trim silence" detail="Skip pauses before and after speaking; needed for fast long dictations">
          <Switch
            label="Trim silence"
            checked={settings.trim_silence}
            onChange={(trim_silence) => update({ trim_silence })}
          />
        </Row>
        <Row label="Microphone">
          <MicrophoneSelect value={settings.microphone} onChange={(microphone) => update({ microphone })} />
        </Row>
      </Group>
      <Group title="Overlay" footer="The card shows the app you're dictating into, the live transcript and quick controls.">
        <Row label="Size">
          <Segmented<OverlaySize>
            label="Overlay size"
            value={settings.overlay_size}
            options={[
              ["pill", "Pill"],
              ["card", "Card"],
              ["large", "Large"],
            ]}
            onChange={(overlay_size) => update({ overlay_size })}
          />
        </Row>
        <Row label="Position">
          <Segmented<OverlayPosition>
            label="Overlay position"
            value={settings.overlay_position}
            options={[
              ["bottom", "Bottom"],
              ["top", "Top"],
            ]}
            onChange={(overlay_position) => update({ overlay_position })}
          />
        </Row>
        <Row
          label="Distance from edge"
          detail={settings.overlay_position === "bottom" ? "Above the Dock" : "Below the menu bar"}
        >
          <OffsetSlider value={settings.overlay_offset} onCommit={(overlay_offset) => update({ overlay_offset })} />
        </Row>
      </Group>
      <Group title="Permissions">
        <div className="group-body">
          <Permissions app={app} />
        </div>
      </Group>
    </div>
  );
}

export function AboutPage({ app }: { app: AppState }) {
  if (!app.snapshot) return null;
  return (
    <div className="page about">
      <img className="about-icon" src="/app-icon.png" alt="" width={72} height={72} />
      <h1 className="wordmark">EchoLocal</h1>
      <p className="hint">Version {app.snapshot.version}</p>
      <p>
        Local push-to-talk dictation with NVIDIA Parakeet. Speech recognition runs on this Mac; no account, no cloud
        audio.
      </p>
      <p className="hint">
        Logs: ~/Library/Logs/app.echolocal.desktop · Built on ideas from Handy (MIT). Parakeet models by NVIDIA (CC-BY-4.0).
      </p>
    </div>
  );
}
