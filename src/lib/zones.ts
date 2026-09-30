import type { MonitorInfo, NormalizedRect, PixelRect, Rect, Zone } from "./types";

const MIN = 0.02;

export function sanitize(r: NormalizedRect): NormalizedRect {
    const width = clamp(r.width, MIN, 1);
    const height = clamp(r.height, MIN, 1);
    return { x: clamp(r.x, 0, 1 - width), y: clamp(r.y, 0, 1 - height), width, height };
}

export function clamp(v: number, lo: number, hi: number) {
    return Math.min(hi, Math.max(lo, v));
}

/** Same edge rounding as NormalizedRect::resolve in Rust. */
export function resolve(r: NormalizedRect, ref: Rect): Rect {
    const w = ref.right - ref.left;
    const h = ref.bottom - ref.top;
    return {
        left: ref.left + Math.round(w * r.x),
        top: ref.top + Math.round(h * r.y),
        right: ref.left + Math.round(w * (r.x + r.width)),
        bottom: ref.top + Math.round(h * (r.y + r.height)),
    };
}

export function referenceRect(zone: Zone, monitor: MonitorInfo): Rect {
    return zone.reference === "workArea" ? monitor.workArea : monitor.bounds;
}

export const MIN_PX = 32;

/** Same shrink-then-shift fitting as PixelRect::resolve in Rust. */
export function fitPixels(p: PixelRect, ref: Rect): PixelRect {
    const refW = Math.max(1, ref.right - ref.left);
    const refH = Math.max(1, ref.bottom - ref.top);
    const width = Math.min(Math.max(MIN_PX, Math.round(p.width)), refW);
    const height = Math.min(Math.max(MIN_PX, Math.round(p.height)), refH);
    const x = Math.min(Math.max(0, Math.round(p.x)), refW - width);
    const y = Math.min(Math.max(0, Math.round(p.y)), refH - height);
    return { x, y, width, height };
}

export function toPixels(r: NormalizedRect, ref: Rect): PixelRect {
    const abs = resolve(r, ref);
    return { x: abs.left - ref.left, y: abs.top - ref.top, width: abs.right - abs.left, height: abs.bottom - abs.top };
}

export function toNormalized(p: PixelRect, ref: Rect): NormalizedRect {
    const f = fitPixels(p, ref);
    const w = Math.max(1, ref.right - ref.left);
    const h = Math.max(1, ref.bottom - ref.top);
    return { x: f.x / w, y: f.y / h, width: f.width / w, height: f.height / h };
}

/** What the zone covers on `ref`, as fractions — the editor always works in this space. */
export function effectiveRect(zone: Zone, ref: Rect): NormalizedRect {
    return zone.unit === "pixels" && zone.pixels ? toNormalized(zone.pixels, ref) : zone.rect;
}

const MAGNETS = [0, 0.25, 1 / 3, 0.5, 2 / 3, 0.75, 1];

/** Snaps to a 0.5% grid (or `pxGrid` pixels of `span`), pulling toward common fractions when close. */
export function snap(v: number, span?: number, pxGrid = 8): number {
    const near = MAGNETS.find((m) => Math.abs(m - v) < 0.012);
    if (near !== undefined) return near;
    if (span && span > 0) return (Math.round((v * span) / pxGrid) * pxGrid) / span;
    return Math.round(v * 200) / 200;
}

export interface Preset {
    label: string;
    rect: (monitor: MonitorInfo | null) => NormalizedRect;
}

const r = (x: number, y: number, width: number, height: number): NormalizedRect => ({ x, y, width, height });

export const PRESETS: Preset[] = [
    { label: "Left 75%", rect: () => r(0, 0, 0.75, 1) },
    { label: "Right 25%", rect: () => r(0.75, 0, 0.25, 1) },
    { label: "Right 75%", rect: () => r(0.25, 0, 0.75, 1) },
    { label: "Left 25%", rect: () => r(0, 0, 0.25, 1) },
    { label: "Left 50%", rect: () => r(0, 0, 0.5, 1) },
    { label: "Right 50%", rect: () => r(0.5, 0, 0.5, 1) },
    { label: "Top half", rect: () => r(0, 0, 1, 0.5) },
    { label: "Bottom half", rect: () => r(0, 0.5, 1, 0.5) },
    {
        label: "Centered 16:9",
        rect: (m) => {
            if (!m) return r(0.125, 0, 0.75, 1);
            const w = m.bounds.right - m.bounds.left;
            const h = m.bounds.bottom - m.bounds.top;
            const width = Math.min(1, (h * 16) / 9 / w);
            const height = Math.min(1, (w * width * 9) / 16 / h);
            return r((1 - width) / 2, (1 - height) / 2, width, height);
        },
    },
    { label: "Full", rect: () => r(0, 0, 1, 1) },
];
