import type { KnownApp, WindowInfo } from "./types";

const LEGACY_SEEN_KEY = "screenbound.seenApps";

export type SeenApp = KnownApp;

export interface AppRow {
    processName: string;
    title: string;
    running: boolean;
    applied: boolean;
    groupId: string | null;
}

/** Older builds stored the list in the webview. Read once so it can move into config.json. */
export function loadLegacySeenApps(): SeenApp[] {
    try {
        const raw = localStorage.getItem(LEGACY_SEEN_KEY);
        if (!raw) return [];
        const parsed: unknown = JSON.parse(raw);
        if (!Array.isArray(parsed)) return [];
        return parsed.flatMap((entry) => {
            if (!entry || typeof entry !== "object") return [];
            const processName = "processName" in entry && typeof entry.processName === "string" ? entry.processName.trim() : "";
            const title = "title" in entry && typeof entry.title === "string" ? entry.title.trim() : "";
            return processName ? [{ processName, title }] : [];
        });
    } catch {
        return [];
    }
}

export function clearLegacySeenApps(): void {
    localStorage.removeItem(LEGACY_SEEN_KEY);
}

/** Merges currently open windows into the remembered list. Keeps a useful title when one was seen. */
export function rememberApps(seen: SeenApp[], windows: WindowInfo[]): SeenApp[] {
    const byName = new Map<string, SeenApp>();
    for (const app of seen) byName.set(app.processName.toLowerCase(), app);
    for (const window of windows) {
        const processName = window.identity.processName.trim();
        if (!processName) continue;
        const key = processName.toLowerCase();
        const previous = byName.get(key);
        const title = window.identity.title.trim();
        byName.set(key, {
            processName,
            title: title || previous?.title || "",
            enabled: previous?.enabled ?? false,
            groupId: previous?.groupId ?? null,
        });
    }
    return [...byName.values()];
}

export function appRows(seen: SeenApp[], running: Iterable<string>): AppRow[] {
    const runningKeys = new Set([...running].map((name) => name.toLowerCase()));
    return seen
        .map((app) => ({
            processName: app.processName,
            title: app.title,
            running: runningKeys.has(app.processName.toLowerCase()),
            applied: !!app.enabled,
            groupId: app.groupId ?? null,
        }))
        .sort((a, b) => {
            if (a.running !== b.running) return a.running ? -1 : 1;
            const labelA = (a.title || a.processName).toLowerCase();
            const labelB = (b.title || b.processName).toLowerCase();
            return labelA.localeCompare(labelB);
        });
}

export function setAppsEnabled(apps: SeenApp[], processNames: string[], enabled: boolean): void {
    const wanted = new Set(processNames.map((name) => name.toLowerCase()));
    for (const app of apps) {
        if (wanted.has(app.processName.toLowerCase())) app.enabled = enabled;
    }
}
