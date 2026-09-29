import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ActivityEntry, AppConfig, EngineStatus, MonitorInfo, WindowInfo } from "./types";

export const api = {
  getMonitors: () => invoke<MonitorInfo[]>("get_monitors"),
  getConfig: () => invoke<AppConfig>("get_config"),
  setConfig: (config: AppConfig) => invoke<AppConfig>("set_config", { config }),
  setEnabled: (enabled: boolean) => invoke<AppConfig>("set_enabled", { enabled }),
  getStatus: () => invoke<EngineStatus>("get_status"),
  listWindows: () => invoke<WindowInfo[]>("list_windows"),
  getActivity: () => invoke<ActivityEntry[]>("get_activity"),
  releaseWindow: (id: number) => invoke<void>("release_window", { id }),
  releaseAll: () => invoke<void>("release_all"),
};

export const events = {
  onActivity: (cb: (e: ActivityEntry) => void) => listen<ActivityEntry>("sb://activity", (e) => cb(e.payload)),
  onManagedChanged: (cb: () => void) => listen("sb://managed-changed", () => cb()),
  onMonitorsChanged: (cb: (m: MonitorInfo[]) => void) =>
    listen<MonitorInfo[]>("sb://monitors-changed", (e) => cb(e.payload)),
  onConfigChanged: (cb: (c: AppConfig) => void) => listen<AppConfig>("sb://config-changed", (e) => cb(e.payload)),
};

export type { UnlistenFn };
