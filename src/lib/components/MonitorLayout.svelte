<script lang="ts">
  import type { MonitorInfo } from "$lib/types";
  import { rectH, rectW } from "$lib/types";

  let {
    monitors,
    selectedId,
    onselect,
  }: { monitors: MonitorInfo[]; selectedId: string | null; onselect: (id: string) => void } = $props();

  // Desktop coordinates may be negative; normalize to the union of all monitors.
  const union = $derived.by(() => {
    if (monitors.length === 0) return { left: 0, top: 0, width: 1, height: 1 };
    const left = Math.min(...monitors.map((m) => m.bounds.left));
    const top = Math.min(...monitors.map((m) => m.bounds.top));
    const right = Math.max(...monitors.map((m) => m.bounds.right));
    const bottom = Math.max(...monitors.map((m) => m.bounds.bottom));
    return { left, top, width: right - left, height: bottom - top };
  });

  const pct = (v: number, of: number) => `${(v / of) * 100}%`;
</script>

<div class="layout" style:aspect-ratio="{union.width} / {union.height}">
  {#each monitors as m (m.id)}
    <button
      class="monitor"
      class:selected={m.id === selectedId}
      style:left={pct(m.bounds.left - union.left, union.width)}
      style:top={pct(m.bounds.top - union.top, union.height)}
      style:width={pct(rectW(m.bounds), union.width)}
      style:height={pct(rectH(m.bounds), union.height)}
      onclick={() => onselect(m.id)}
      title={`${m.deviceName}\n${m.id}`}
    >
      <span class="num">{m.number || "?"}{m.isPrimary ? " ★" : ""}</span>
      <span class="name">{m.friendlyName}</span>
      <span class="meta">{rectW(m.bounds)}×{rectH(m.bounds)} · {Math.round((m.dpi / 96) * 100)}%</span>
    </button>
  {/each}
</div>

<style>
  .layout {
    position: relative;
    width: 100%;
    max-height: 140px;
    margin: 0 auto;
  }
  .monitor {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    color: var(--text);
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    overflow: hidden;
    font-size: 11px;
    padding: 2px;
    cursor: pointer;
  }
  .monitor:hover {
    border-color: var(--accent);
  }
  .monitor.selected {
    border: 2px solid var(--accent);
    background: var(--accent-soft);
  }
  .num {
    font-weight: 700;
    font-size: 14px;
  }
  .name,
  .meta {
    white-space: nowrap;
    color: var(--text-dim);
  }
</style>
