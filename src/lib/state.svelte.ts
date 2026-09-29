import { api, events, type UnlistenFn } from "./api";
import type { ActivityEntry, AppConfig, EngineStatus, MonitorInfo, Zone } from "./types";

const SAVE_DELAY_MS = 350;
const ACTIVITY_LIMIT = 300;

export function newId(prefix: string): string {
  const bytes = crypto.getRandomValues(new Uint8Array(6));
  return `${prefix}-${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

class AppState {
  monitors = $state<MonitorInfo[]>([]);
  config = $state<AppConfig | null>(null);
  status = $state<EngineStatus | null>(null);
  activity = $state<ActivityEntry[]>([]);
  error = $state<string | null>(null);
  saveState = $state<"idle" | "pending" | "saving" | "saved">("idle");
  selectedMonitorId = $state<string | null>(null);
  selectedZoneId = $state<string | null>(null);

  #saveTimer: ReturnType<typeof setTimeout> | null = null;
  #unlisten: UnlistenFn[] = [];

  selectedMonitor = $derived(
    this.monitors.find((m) => m.id === this.selectedMonitorId) ??
      this.monitors.find((m) => m.isPrimary) ??
      this.monitors[0] ??
      null,
  );

  selectedZone = $derived(this.config?.zones.find((z) => z.id === this.selectedZoneId) ?? this.config?.zones[0] ?? null);

  async init() {
    try {
      const [monitors, config, status, activity] = await Promise.all([
        api.getMonitors(),
        api.getConfig(),
        api.getStatus(),
        api.getActivity(),
      ]);
      this.monitors = monitors;
      this.config = config;
      this.status = status;
      this.activity = activity.reverse();
    } catch (e) {
      this.error = String(e);
    }

    this.#unlisten = await Promise.all([
      events.onActivity((entry) => {
        this.activity = [entry, ...this.activity].slice(0, ACTIVITY_LIMIT);
      }),
      events.onManagedChanged(() => void this.refreshStatus()),
      events.onMonitorsChanged((m) => (this.monitors = m)),
      events.onConfigChanged((c) => {
        // Don't clobber edits the user is still making.
        if (this.saveState !== "pending" && this.saveState !== "saving") this.config = c;
      }),
    ]);
  }

  dispose() {
    this.#unlisten.forEach((u) => u());
  }

  async refreshStatus() {
    try {
      this.status = await api.getStatus();
    } catch (e) {
      this.error = String(e);
    }
  }

  async refreshMonitors() {
    this.monitors = await api.getMonitors();
  }

  /** Call after mutating `config` in place; persists after a short debounce. */
  scheduleSave(immediate = false) {
    this.saveState = "pending";
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => void this.#save(), immediate ? 0 : SAVE_DELAY_MS);
  }

  async #save() {
    if (!this.config) return;
    this.saveState = "saving";
    try {
      this.config = await api.setConfig($state.snapshot(this.config));
      this.error = null;
      this.saveState = "saved";
    } catch (e) {
      this.error = String(e);
      this.saveState = "idle";
    }
  }

  async setEnabled(enabled: boolean) {
    try {
      this.config = await api.setEnabled(enabled);
      await this.refreshStatus();
    } catch (e) {
      this.error = String(e);
    }
  }

  addZone(): Zone | null {
    if (!this.config) return null;
    const zone: Zone = {
      id: newId("zone"),
      name: `Zone ${this.config.zones.length + 1}`,
      rect: { x: 0, y: 0, width: 0.5, height: 1 },
      monitor: null,
      reference: "monitor",
    };
    this.config.zones.push(zone);
    this.selectedZoneId = zone.id;
    this.scheduleSave(true);
    return zone;
  }

  zoneInUse(zoneId: string): boolean {
    return !!this.config?.rules.some((r) => r.actions.some((a) => a.zoneId === zoneId));
  }

  deleteZone(zoneId: string) {
    if (!this.config || this.zoneInUse(zoneId)) return;
    this.config.zones = this.config.zones.filter((z) => z.id !== zoneId);
    if (this.selectedZoneId === zoneId) this.selectedZoneId = this.config.zones[0]?.id ?? null;
    this.scheduleSave(true);
  }
}

export const app = new AppState();
