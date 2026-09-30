// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";

export type ModelId = "parakeet-tdt-v2" | "parakeet-tdt-v3";
export type PostProcessing = "off" | "clean" | "rewrite";
export type ModelUnload = "never" | "after5_minutes" | "after15_minutes" | "after1_hour";
export type Phase = "idle" | "preparing" | "recording" | "transcribing" | "post_processing" | "inserting";
export type OverlaySize = "pill" | "card" | "large";
export type OverlayPosition = "bottom" | "top";
export type Permission = "granted" | "denied" | "not_determined" | "unknown";

export interface AiSettings {
  base_url: string;
  api_key: string;
  model: string;
  rewrite_instruction: string;
}

export interface Settings {
  model: ModelId;
  shortcut: string;
  post_processing: PostProcessing;
  ai: AiSettings;
  launch_at_login: boolean;
  trim_silence: boolean;
  live_preview: boolean;
  overlay_size: OverlaySize;
  overlay_position: OverlayPosition;
  overlay_offset: number;
  unload_model: ModelUnload;
  microphone: string | null;
}

export interface ModelView {
  id: ModelId;
  name: string;
  subtitle: string;
  size_label: string;
  downloaded: boolean;
  downloading: boolean;
  progress_bytes: number;
  total_bytes: number;
  selected: boolean;
}

export type EngineStatus =
  | { status: "no_model" }
  | { status: "loading"; model: ModelId }
  | { status: "ready"; model: ModelId }
  | { status: "unloaded"; model: ModelId }
  | { status: "failed"; model: ModelId; error: string };

export type HotkeyStatus =
  | { state: "active" }
  | { state: "needs_accessibility" }
  | { state: "error"; detail: string };

export interface SessionStats {
  dictations: number;
  words: number;
  last_release_to_text_ms: number | null;
  last_audio_ms: number | null;
}

export interface Snapshot {
  settings: Settings;
  models: ModelView[];
  engine: EngineStatus;
  phase: Phase;
  accessibility: boolean;
  microphone: Permission;
  shortcut_label: string;
  hotkey: HotkeyStatus;
  stats: SessionStats;
  version: string;
}

export interface DownloadProgress {
  id: ModelId;
  downloaded: number;
  total: number;
}

export type CaptureEvent =
  | { state: "pending"; label: string }
  | { state: "done"; shortcut: string; label: string }
  | { state: "invalid"; message: string }
  | { state: "cancelled" };

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  saveSettings: (settings: Settings) => invoke<Snapshot>("save_settings", { settings }),
  downloadModel: (id: ModelId) => invoke<void>("download_model", { id }),
  cancelDownload: (id: ModelId) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: ModelId) => invoke<void>("delete_model", { id }),
  requestAccessibility: () => invoke<boolean>("request_accessibility"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  requestMicrophone: () => invoke<void>("request_microphone"),
  openMicrophoneSettings: () => invoke<void>("open_microphone_settings"),
  startShortcutCapture: () => invoke<void>("start_shortcut_capture"),
  stopShortcutCapture: () => invoke<void>("stop_shortcut_capture"),
  testAi: (ai: AiSettings, mode: PostProcessing) => invoke<string>("test_ai", { ai, mode }),
};

/** Settings are ready for daily use: a model, both permissions. */
export function setupComplete(s: Snapshot): boolean {
  return s.models.some((m) => m.selected && m.downloaded) && s.accessibility && s.microphone === "granted";
}
