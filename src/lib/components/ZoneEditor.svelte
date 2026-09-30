<script lang="ts">
  import type { NormalizedRect, Rect, ZoneUnit } from "$lib/types";
  import { rectH, rectW } from "$lib/types";
  import { clamp, resolve, sanitize, snap } from "$lib/zones";

  type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

  let {
    rect,
    reference,
    unit = "percent",
    others = [],
    onchange,
    oncommit,
  }: {
    rect: NormalizedRect;
    /** Physical pixel rect the zone is relative to (monitor bounds or work area). */
    reference: Rect;
    unit?: ZoneUnit;
    others?: { name: string; rect: NormalizedRect }[];
    onchange: (r: NormalizedRect) => void;
    oncommit: () => void;
  } = $props();

  let surface: HTMLDivElement;
  let drag: { handle: Handle; startX: number; startY: number; start: NormalizedRect } | null = null;

  const px = $derived(resolve(rect, reference));
  const pct = (v: number) => `${v * 100}%`;

  function begin(e: PointerEvent, handle: Handle) {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { handle, startX: e.clientX, startY: e.clientY, start: { ...rect } };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const box = surface.getBoundingClientRect();
    const dx = (e.clientX - drag.startX) / box.width;
    const dy = (e.clientY - drag.startY) / box.height;
    const s = drag.start;
    const free = e.shiftKey;
    const pixels = unit === "pixels";
    const snX = (v: number) => (free ? v : snap(v, pixels ? rectW(reference) : undefined));
    const snY = (v: number) => (free ? v : snap(v, pixels ? rectH(reference) : undefined));
    let left = s.x;
    let top = s.y;
    let right = s.x + s.width;
    let bottom = s.y + s.height;

    if (drag.handle === "move") {
      const x = clamp(snX(s.x + dx), 0, 1 - s.width);
      const y = clamp(snY(s.y + dy), 0, 1 - s.height);
      onchange({ x, y, width: s.width, height: s.height });
      return;
    }
    if (drag.handle.includes("w")) left = clamp(snX(s.x + dx), 0, right - 0.02);
    if (drag.handle.includes("e")) right = clamp(snX(right + dx), left + 0.02, 1);
    if (drag.handle.includes("n")) top = clamp(snY(s.y + dy), 0, bottom - 0.02);
    if (drag.handle.includes("s")) bottom = clamp(snY(bottom + dy), top + 0.02, 1);
    onchange(sanitize({ x: left, y: top, width: right - left, height: bottom - top }));
  }

  function end() {
    if (!drag) return;
    drag = null;
    oncommit();
  }

  const handles: Handle[] = ["n", "s", "e", "w", "ne", "nw", "se", "sw"];
</script>

<div class="editor">
  <div
    class="surface"
    bind:this={surface}
    style:aspect-ratio="{Math.max(1, rectW(reference))} / {Math.max(1, rectH(reference))}"
    style:width="min(100%, calc(46vh * {Math.max(1, rectW(reference)) / Math.max(1, rectH(reference))}))"
  >
    <div class="grid"></div>
    {#each others as o, i (i)}
      <div
        class="ghost"
        style:left={pct(o.rect.x)}
        style:top={pct(o.rect.y)}
        style:width={pct(o.rect.width)}
        style:height={pct(o.rect.height)}
      >
        <span>{o.name}</span>
      </div>
    {/each}
    <div
      class="zone"
      role="slider"
      tabindex="0"
      aria-valuenow={Math.round(rect.width * 100)}
      aria-label="Zone"
      style:left={pct(rect.x)}
      style:top={pct(rect.y)}
      style:width={pct(rect.width)}
      style:height={pct(rect.height)}
      onpointerdown={(e) => begin(e, "move")}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={end}
    >
      <div class="label">
        {#if unit === "pixels"}
          <strong>{rectW(px)} × {rectH(px)} px</strong>
          <span>{Math.round(rect.width * 1000) / 10}% × {Math.round(rect.height * 1000) / 10}%</span>
        {:else}
          <strong>{Math.round(rect.width * 1000) / 10}% × {Math.round(rect.height * 1000) / 10}%</strong>
          <span>{rectW(px)} × {rectH(px)} px</span>
        {/if}
      </div>
      {#each handles as h (h)}
        <!-- Move/up events bubble to the zone element while captured. -->
        <div class="handle {h}" role="presentation" onpointerdown={(e) => begin(e, h)}></div>
      {/each}
    </div>
  </div>
  <p class="hint">
    Drag to move, drag edges/corners to resize. Snaps to {unit === "pixels" ? "8 px" : "0.5%"} and common splits; hold
    Shift for free resize.
  </p>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .surface {
    position: relative;
    margin: 0 auto;
    background: var(--surface-0);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    user-select: none;
    touch-action: none;
  }
  .grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(to right, var(--grid) 1px, transparent 1px),
      linear-gradient(to bottom, var(--grid) 1px, transparent 1px);
    background-size: 25% 25%;
    pointer-events: none;
  }
  .ghost {
    position: absolute;
    box-sizing: border-box;
    border: 1px dashed var(--border-strong);
    color: var(--text-dim);
    font-size: 11px;
    padding: 4px;
    pointer-events: none;
  }
  .zone {
    position: absolute;
    box-sizing: border-box;
    background: var(--accent-soft);
    border: 2px solid var(--accent);
    cursor: move;
    display: flex;
    align-items: center;
    justify-content: center;
    outline: none;
  }
  .zone:focus-visible {
    box-shadow: 0 0 0 2px var(--text);
  }
  .label {
    display: flex;
    flex-direction: column;
    align-items: center;
    font-size: 12px;
    pointer-events: none;
    text-align: center;
  }
  .label span {
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .handle {
    position: absolute;
    width: 12px;
    height: 12px;
    background: var(--accent);
    border-radius: 2px;
  }
  .handle.n,
  .handle.s {
    left: calc(50% - 6px);
    cursor: ns-resize;
  }
  .handle.e,
  .handle.w {
    top: calc(50% - 6px);
    cursor: ew-resize;
  }
  .handle.n,
  .handle.ne,
  .handle.nw {
    top: -7px;
  }
  .handle.s,
  .handle.se,
  .handle.sw {
    bottom: -7px;
  }
  .handle.e,
  .handle.ne,
  .handle.se {
    right: -7px;
  }
  .handle.w,
  .handle.nw,
  .handle.sw {
    left: -7px;
  }
  .handle.ne,
  .handle.sw {
    cursor: nesw-resize;
  }
  .handle.nw,
  .handle.se {
    cursor: nwse-resize;
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-dim);
  }
</style>
