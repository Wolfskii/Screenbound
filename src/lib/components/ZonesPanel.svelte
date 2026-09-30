<script lang="ts">
  import { app } from "$lib/state.svelte";
  import type { NormalizedRect, PixelRect, Zone, ZoneUnit } from "$lib/types";
  import { MIN_PX, PRESETS, effectiveRect, fitPixels, referenceRect, sanitize, toNormalized, toPixels } from "$lib/zones";
  import MonitorLayout from "./MonitorLayout.svelte";
  import ZoneEditor from "./ZoneEditor.svelte";

  const FIELDS = [
    ["x", "X"],
    ["y", "Y"],
    ["width", "W"],
    ["height", "H"],
  ] as const;

  const zone = $derived(app.selectedZone);
  const previewMonitor = $derived(
    (zone?.monitor && app.monitors.find((m) => m.id === zone.monitor)) || app.selectedMonitor,
  );
  const previewRef = $derived(zone && previewMonitor ? referenceRect(zone, previewMonitor) : null);
  const pinnedMissing = $derived(!!zone?.monitor && !app.monitors.some((m) => m.id === zone.monitor));
  const others = $derived(
    (app.config?.zones ?? [])
      .filter((z) => z.id !== zone?.id && (!z.monitor || z.monitor === previewMonitor?.id))
      .map((z) => ({
        name: z.name,
        rect: previewMonitor ? effectiveRect(z, referenceRect(z, previewMonitor)) : z.rect,
      })),
  );
  const editRect = $derived(zone && previewRef ? effectiveRect(zone, previewRef) : zone?.rect);
  const pixelValues = $derived<PixelRect | null>(
    zone ? (zone.pixels ?? (previewRef ? toPixels(zone.rect, previewRef) : null)) : null,
  );

  /** Takes fractions of the preview reference; in pixel mode stores exact pixels too. */
  function setRect(r: NormalizedRect) {
    if (!zone) return;
    const clean = sanitize(r);
    zone.rect = clean;
    if (zone.unit === "pixels" && previewRef) zone.pixels = toPixels(clean, previewRef);
  }

  function setPercent(key: keyof NormalizedRect, value: number) {
    if (!zone || !Number.isFinite(value)) return;
    setRect({ ...zone.rect, [key]: value / 100 });
    app.scheduleSave();
  }

  function setPixel(key: keyof PixelRect, value: number) {
    if (!zone || !Number.isFinite(value) || !pixelValues) return;
    const next = { ...pixelValues, [key]: Math.round(value) };
    zone.pixels = previewRef
      ? fitPixels(next, previewRef)
      : { ...next, width: Math.max(MIN_PX, next.width), height: Math.max(MIN_PX, next.height) };
    if (previewRef) zone.rect = sanitize(toNormalized(zone.pixels, previewRef));
    app.scheduleSave();
  }

  function setUnit(unit: ZoneUnit) {
    if (!zone || zone.unit === unit) return;
    if (unit === "pixels" && previewRef) zone.pixels = toPixels(zone.rect, previewRef);
    if (unit === "percent" && zone.pixels && previewRef) zone.rect = sanitize(toNormalized(zone.pixels, previewRef));
    zone.unit = unit;
    app.scheduleSave(true);
  }

  function sizeLabel(z: Zone) {
    if (z.unit === "pixels" && z.pixels) return `${z.pixels.width} × ${z.pixels.height} px`;
    return `${round1(z.rect.width)}% × ${round1(z.rect.height)}%`;
  }

  const round1 = (v: number) => Math.round(v * 1000) / 10;
</script>

<div class="zones">
  <aside class="card list">
    <div class="list-head">
      <h3>Zones</h3>
      <button onclick={() => app.addZone()}>+ New</button>
    </div>
    {#each app.config?.zones ?? [] as z (z.id)}
      <button class="item" class:active={z.id === zone?.id} onclick={() => (app.selectedZoneId = z.id)}>
        <span>{z.name}</span>
        <small>{sizeLabel(z)}</small>
      </button>
    {/each}

    {#if zone}
      <div class="props">
        <label>
          Name
          <input bind:value={zone.name} oninput={() => app.scheduleSave()} />
        </label>
        <label>
          Monitor
          <select
            value={zone.monitor ?? ""}
            onchange={(e) => {
              zone.monitor = e.currentTarget.value || null;
              app.scheduleSave(true);
            }}
          >
            <option value="">Where the window is</option>
            {#each app.monitors as m (m.id)}
              <option value={m.id}>{m.number}: {m.friendlyName}</option>
            {/each}
            {#if pinnedMissing}
              <option value={zone.monitor}>Disconnected monitor</option>
            {/if}
          </select>
        </label>
        <label>
          Relative to
          <select bind:value={zone.reference} onchange={() => app.scheduleSave(true)}>
            <option value="monitor">Full monitor</option>
            <option value="workArea">Work area (excl. taskbar)</option>
          </select>
        </label>
        <label>
          Units
          <select value={zone.unit} onchange={(e) => setUnit(e.currentTarget.value as ZoneUnit)}>
            <option value="percent">Percent (scales with monitor)</option>
            <option value="pixels">Pixels (exact size)</option>
          </select>
        </label>
        <div class="nums">
          {#each FIELDS as [key, lbl] (key)}
            <label>
              {#if zone.unit === "pixels"}
                {lbl} px
                <input
                  type="number"
                  step="1"
                  min={key === "width" || key === "height" ? MIN_PX : 0}
                  value={pixelValues?.[key] ?? 0}
                  onchange={(e) => setPixel(key, e.currentTarget.valueAsNumber)}
                />
              {:else}
                {lbl} %
                <input
                  type="number"
                  step="0.5"
                  min="0"
                  max="100"
                  value={round1(zone.rect[key])}
                  onchange={(e) => setPercent(key, e.currentTarget.valueAsNumber)}
                />
              {/if}
            </label>
          {/each}
        </div>
        {#if zone.unit === "pixels"}
          <p class="dim small">Offset from the monitor's top-left. Shrinks to fit on smaller monitors.</p>
        {/if}
        <button
          class="danger"
          disabled={app.zoneInUse(zone.id)}
          title={app.zoneInUse(zone.id) ? "Used by a rule" : ""}
          onclick={() => app.deleteZone(zone.id)}
        >
          Delete zone
        </button>
      </div>
    {/if}
  </aside>

  <section class="card main">
    <MonitorLayout
      monitors={app.monitors}
      selectedId={previewMonitor?.id ?? null}
      onselect={(id) => (app.selectedMonitorId = id)}
    />
    {#if zone && previewMonitor}
      <div class="presets">
        {#each PRESETS as p (p.label)}
          <button
            onclick={() => {
              setRect(p.rect(previewMonitor));
              app.scheduleSave(true);
            }}>{p.label}</button
          >
        {/each}
      </div>
      <ZoneEditor
        rect={editRect ?? zone.rect}
        reference={referenceRect(zone, previewMonitor)}
        unit={zone.unit}
        {others}
        onchange={setRect}
        oncommit={() => app.scheduleSave(true)}
      />
      <p class="dim">
        Previewing on <strong>{previewMonitor.number}: {previewMonitor.friendlyName}</strong>.
        {#if !zone.monitor}Unpinned zones apply to whichever monitor the window goes fullscreen on.{/if}
      </p>
    {:else}
      <p class="dim">No monitors or zones available.</p>
    {/if}
  </section>
</div>

<style>
  .zones {
    display: grid;
    grid-template-columns: 250px 1fr;
    gap: 12px;
    min-height: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow: auto;
  }
  .list-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .item {
    display: flex;
    justify-content: space-between;
    text-align: left;
    background: transparent;
    border-color: transparent;
  }
  .item.active {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .item small {
    color: var(--text-dim);
  }
  .props {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .nums {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .small {
    margin: 0;
    font-size: 11px;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .presets button {
    font-size: 12px;
    padding: 3px 8px;
  }
</style>
