import type { AppConfig, Matcher, WindowInfo, WindowRule } from "./types";

const SEEN_KEY = "screenbound.seenApps";
const APPS_RULE_ID = "rule-apps";

export interface SeenApp {
    processName: string;
    title: string;
}

export interface AppRow {
    processName: string;
    title: string;
    running: boolean;
    applied: boolean;
}

function sameProcess(a: string, b: string): boolean {
    return a.toLowerCase() === b.toLowerCase();
}

export function processNames(matcher: Matcher): string[] {
    if (matcher.type === "processName") return matcher.value.trim() ? [matcher.value.trim()] : [];
    if (matcher.type === "any" || matcher.type === "all") return matcher.value.flatMap(processNames);
    return [];
}

export function mentionsProcess(matcher: Matcher, processName: string): boolean {
    return processNames(matcher).some((name) => sameProcess(name, processName));
}

export function processApplied(rules: WindowRule[], processName: string): boolean {
    return rules.some((rule) => rule.enabled && mentionsProcess(rule.matcher, processName));
}

function stripProcess(matcher: Matcher, processName: string): Matcher | null {
    if (matcher.type === "processName") return sameProcess(matcher.value, processName) ? null : matcher;
    if (matcher.type === "any" || matcher.type === "all") {
        const value = matcher.value
            .map((child) => stripProcess(child, processName))
            .filter((child): child is Matcher => child !== null);
        if (value.length === 0) return null;
        return { type: matcher.type, value };
    }
    return matcher;
}

/** Removes `processName` from every rule. An empty rule matches nothing. */
export function disableProcess(rules: WindowRule[], processName: string): void {
    for (const rule of rules) {
        rule.matcher = stripProcess(rule.matcher, processName) ?? { type: "any", value: [] };
    }
}

function zoneForNewRule(config: AppConfig): string | null {
    for (const rule of config.rules) {
        const zone = rule.actions.find((action) => action.type === "fullscreenZone");
        if (zone) return zone.zoneId;
    }
    return config.zones[0]?.id ?? null;
}

function addProcessName(rule: WindowRule, processName: string): void {
    if (mentionsProcess(rule.matcher, processName)) return;
    if (rule.matcher.type === "any") {
        rule.matcher.value.push({ type: "processName", value: processName });
        return;
    }
    if (rule.matcher.type === "processName") {
        rule.matcher = { type: "any", value: [rule.matcher, { type: "processName", value: processName }] };
        return;
    }
    rule.matcher = { type: "any", value: [rule.matcher, { type: "processName", value: processName }] };
}

/** Adds `processName` to the Apps rule, creating that rule on the current zone when needed. */
export function enableProcess(config: AppConfig, processName: string): void {
    if (processApplied(config.rules, processName)) return;
    const zoneId = zoneForNewRule(config);
    if (!zoneId) return;
    let rule = config.rules.find((candidate) => candidate.id === APPS_RULE_ID);
    if (!rule) {
        rule = {
            id: APPS_RULE_ID,
            name: "Apps",
            enabled: true,
            matcher: { type: "any", value: [] },
            actions: [{ type: "fullscreenZone", zoneId, chrome: "keep" }],
        };
        config.rules.push(rule);
    }
    rule.enabled = true;
    addProcessName(rule, processName);
}

export function loadSeenApps(): SeenApp[] {
    try {
        const raw = localStorage.getItem(SEEN_KEY);
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

export function saveSeenApps(apps: SeenApp[]): void {
    localStorage.setItem(SEEN_KEY, JSON.stringify(apps));
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
        byName.set(key, { processName, title: title || previous?.title || "" });
    }
    return [...byName.values()];
}

export function appRows(seen: SeenApp[], rules: WindowRule[], running: Iterable<string>): AppRow[] {
    const runningKeys = new Set([...running].map((name) => name.toLowerCase()));
    const byName = new Map<string, AppRow>();
    const remember = (processName: string, title: string) => {
        const key = processName.toLowerCase();
        const existing = byName.get(key);
        if (!existing) {
            byName.set(key, { processName, title, running: runningKeys.has(key), applied: processApplied(rules, processName) });
            return;
        }
        if (!existing.title && title) existing.title = title;
    };
    for (const app of seen) remember(app.processName, app.title);
    for (const rule of rules) {
        for (const name of processNames(rule.matcher)) remember(name, "");
    }
    return [...byName.values()].sort((a, b) => {
        if (a.running !== b.running) return a.running ? -1 : 1;
        const labelA = (a.title || a.processName).toLowerCase();
        const labelB = (b.title || b.processName).toLowerCase();
        return labelA.localeCompare(labelB);
    });
}
