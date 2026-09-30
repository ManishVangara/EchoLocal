// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";

export type ModelId = "parakeet-tdt-v2" | "parakeet-tdt-v3";
export type PostProcessing = "off" | "clean" | "rewrite";
export type Phase = "idle" | "preparing" | "recording" | "transcribing" | "post_processing" | "inserting";

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
  | { status: "failed"; model: ModelId; error: string };

export interface Snapshot {
  settings: Settings;
  models: ModelView[];
  engine: EngineStatus;
  phase: Phase;
  accessibility: boolean;
  shortcut_error: string | null;
}

export interface DownloadProgress {
  id: ModelId;
  downloaded: number;
  total: number;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  saveSettings: (settings: Settings) => invoke<Snapshot>("save_settings", { settings }),
  downloadModel: (id: ModelId) => invoke<void>("download_model", { id }),
  cancelDownload: (id: ModelId) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: ModelId) => invoke<void>("delete_model", { id }),
  requestAccessibility: () => invoke<boolean>("request_accessibility"),
  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),
  openMicrophoneSettings: () => invoke<void>("open_microphone_settings"),
  testAi: (ai: AiSettings, mode: PostProcessing) => invoke<string>("test_ai", { ai, mode }),
};
