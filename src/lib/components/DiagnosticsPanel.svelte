<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { app } from "$lib/state.svelte";
  import { fmtRect, type WindowInfo } from "$lib/types";

  let windows = $state<WindowInfo[]>([]);
  let filter = $state("");
  let interestingOnly = $state(false);
  let selected = $state<number | null>(null);
  let loading = $state(false);

  const shown = $derived(
    windows.filter((w) => {
      if (interestingOnly && !(w.fullscreenLike || w.managed || w.matchedRule)) return false;
      const q = filter.trim().toLowerCase();
      return (
        !q ||
        w.identity.processName.toLowerCase().includes(q) ||
        w.identity.title.toLowerCase().includes(q) ||
        w.identity.className.toLowerCase().includes(q)
      );
    }),
  );
  const detail = $derived(windows.find((w) => w.id === selected) ?? null);

  async function refresh() {
    loading = true;
    try {
      windows = await api.listWindows();
    } catch (e) {
      app.error = String(e);
    } finally {
      loading = false;
    }
  }

  // Snapshot on demand only; no background polling.
  onMount(() => {
    void refresh();
  });

  const hex = (n: number) => "0x" + (n >>> 0).toString(16).toUpperCase().padStart(8, "0");
  const time = (ms: number) => new Date(ms).toLocaleTimeString();
</script>

<div class="diag">
  <section class="card">
    <div class="bar">
      <h3>Managed windows</h3>
      <span class="dim">
        Native events: {app.status?.eventsActive ? "active" : "INACTIVE"}
      </span>
      <div class="spacer"></div>
      <button onclick={() => api.releaseAll()} disabled={!app.status?.managed.length}>Release all</button>
    </div>
    {#if app.status?.managed.length}
      <table>
        <thead>
          <tr><th>HWND</th><th>Process</th><th>Title</th><th>Fullscreen bounds</th><th>Zone bounds</th><th>Pre-fullscreen</th><th></th></tr>
        </thead>
        <tbody>
          {#each app.status.managed as m (m.id)}
            <tr class:warn={m.gaveUp}>
              <td class="mono">{m.idHex}</td>
              <td>{m.processName}</td>
              <td class="ellipsis">{m.title}</td>
              <td class="mono">{fmtRect(m.detectedBounds)}</td>
              <td class="mono">{fmtRect(m.target)}{m.gaveUp ? " (app refused)" : ""}</td>
              <td class="mono">{m.preFullscreenBounds ? fmtRect(m.preFullscreenBounds) : "—"}</td>
              <td><button onclick={() => api.releaseWindow(m.id)}>Release</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="dim">No windows are currently constrained.</p>
    {/if}
  </section>

  <section class="card windows">
    <div class="bar">
      <h3>Windows</h3>
      <input placeholder="Filter process, title, class" bind:value={filter} />
      <label class="inline"><input type="checkbox" bind:checked={interestingOnly} /> Matched / fullscreen only</label>
      <div class="spacer"></div>
      <button onclick={refresh} disabled={loading}>{loading ? "…" : "Refresh"}</button>
    </div>
    <div class="split">
      <div class="scroll">
        <table>
          <thead>
            <tr><th>HWND</th><th>Process</th><th>Class</th><th>Title</th><th>Bounds</th><th>Flags</th></tr>
          </thead>
          <tbody>
            {#each shown as w (w.id)}
              <tr class:active={w.id === selected} onclick={() => (selected = w.id)}>
                <td class="mono">{w.idHex}</td>
                <td>{w.identity.processName || "?"}</td>
                <td class="mono ellipsis">{w.identity.className}</td>
                <td class="ellipsis">{w.identity.title}</td>
                <td class="mono">{fmtRect(w.state.bounds)}</td>
                <td class="flags">
                  {#if w.managed}<span class="tag accent">managed</span>{/if}
                  {#if w.fullscreenLike}<span class="tag">fullscreen</span>{/if}
                  {#if w.state.showState !== "normal"}<span class="tag">{w.state.showState}</span>{/if}
                  {#if w.matchedRule}<span class="tag">rule</span>{/if}
                  {#if w.hung}<span class="tag warn">hung</span>{/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if detail}
        <dl class="detail">
          <dt>Process</dt><dd>{detail.identity.processName} (pid {detail.identity.process.pid})</dd>
          <dt>Executable</dt><dd class="mono small">{detail.identity.executablePath || "(access denied)"}</dd>
          <dt>Title</dt><dd>{detail.identity.title}</dd>
          <dt>Class</dt><dd class="mono">{detail.identity.className}</dd>
          <dt>Native ID</dt><dd class="mono">{detail.idHex}</dd>
          <dt>Bounds</dt><dd class="mono">{fmtRect(detail.state.bounds)}</dd>
          <dt>Visible frame</dt><dd class="mono">{fmtRect(detail.state.visibleBounds)}</dd>
          <dt>Restore bounds</dt><dd class="mono">{fmtRect(detail.state.restoreBounds)}</dd>
          <dt>Monitor</dt><dd>{detail.monitorName ?? "—"} · {Math.round((detail.state.dpi / 96) * 100)}% DPI</dd>
          <dt>Show state</dt><dd>{detail.state.showState}{detail.state.cloaked ? " (cloaked)" : ""}</dd>
          <dt>Title bar</dt><dd>{detail.state.hasTitleBar}</dd>
          <dt>Resize frame</dt><dd>{detail.state.hasResizeFrame}</dd>
          <dt>Borderless</dt><dd>{!detail.state.hasTitleBar && !detail.state.hasResizeFrame}</dd>
          <dt>Topmost</dt><dd>{detail.state.topmost}</dd>
          <dt>Style / ExStyle</dt><dd class="mono">{hex(detail.state.style.primary)} / {hex(detail.state.style.extended)}</dd>
          <dt>Fullscreen</dt><dd>{detail.fullscreenLike}</dd>
          <dt>Matched rule</dt><dd>{detail.matchedRule ?? "—"}</dd>
          <dt>Managed</dt><dd>{detail.managed}</dd>
        </dl>
      {/if}
    </div>
  </section>

  <section class="card activity">
    <h3>Activity</h3>
    <div class="scroll">
      {#each app.activity as a, i (i)}
        <div class="log {a.level}">
          <span class="mono dim">{time(a.timestampMs)}</span>
          {#if a.window}<span class="mono">{a.window}</span>{/if}
          <span>{a.message}</span>
        </div>
      {:else}
        <p class="dim">Nothing yet. Enter fullscreen in a matched application.</p>
      {/each}
    </div>
  </section>
</div>

<style>
  .diag {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .bar h3 {
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  .inline {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .split {
    display: grid;
    grid-template-columns: 1fr 320px;
    gap: 10px;
    min-height: 0;
  }
  .scroll {
    max-height: 300px;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    color: var(--text-dim);
    font-weight: 500;
    position: sticky;
    top: 0;
    background: var(--surface-1);
  }
  td,
  th {
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
  }
  tbody tr {
    cursor: default;
  }
  tbody tr:hover,
  tr.active {
    background: var(--surface-2);
  }
  tr.warn td {
    color: var(--warn);
  }
  .ellipsis {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .flags {
    white-space: nowrap;
  }
  .tag {
    font-size: 10px;
    border: 1px solid var(--border-strong);
    border-radius: 3px;
    padding: 0 4px;
    margin-right: 2px;
  }
  .tag.accent {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag.warn {
    border-color: var(--warn);
    color: var(--warn);
  }
  .detail {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 10px;
    margin: 0;
    font-size: 12px;
    align-content: start;
  }
  .detail dt {
    color: var(--text-dim);
  }
  .detail dd {
    margin: 0;
    word-break: break-all;
  }
  .small {
    font-size: 11px;
  }
  .activity .scroll {
    max-height: 200px;
  }
  .log {
    display: flex;
    gap: 8px;
    font-size: 12px;
    padding: 2px 0;
    border-bottom: 1px solid var(--border);
  }
  .log.warn {
    color: var(--warn);
  }
  .log.error {
    color: var(--error);
  }
</style>
