import type { MonitorInfo, NormalizedRect, Rect, Zone } from "./types";

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

const MAGNETS = [0, 0.25, 1 / 3, 0.5, 2 / 3, 0.75, 1];

/** Snaps to a 0.5% grid, pulling toward common fractions when close. */
export function snap(v: number): number {
  const near = MAGNETS.find((m) => Math.abs(m - v) < 0.012);
  if (near !== undefined) return near;
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
