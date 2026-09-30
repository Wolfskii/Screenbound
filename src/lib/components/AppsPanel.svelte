<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import {
    appRows,
    disableProcess,
    enableProcess,
    loadSeenApps,
    rememberApps,
    saveSeenApps,
    type SeenApp,
  } from "$lib/apps";
  import { app } from "$lib/state.svelte";

  let seen = $state<SeenApp[]>(loadSeenApps());
  let running = $state<string[]>([]);
  let query = $state("");
  let loading = $state(false);

  const rows = $derived.by(() => {
    if (!app.config) return [];
    const q = query.trim().toLowerCase();
    return appRows(seen, app.config.rules, running).filter((row) => {
      if (!q) return true;
      return row.processName.toLowerCase().includes(q) || row.title.toLowerCase().includes(q);
    });
  });

  async function refresh() {
    loading = true;
    try {
      const windows = await api.listWindows();
      running = windows.map((window) => window.identity.processName).filter(Boolean);
      seen = rememberApps(seen, windows);
      saveSeenApps(seen);
    } catch (e) {
      app.error = String(e);
    } finally {
      loading = false;
    }
  }

  function toggle(processName: string, applied: boolean) {
    if (!app.config) return;
    if (applied) disableProcess(app.config.rules, processName);
    else enableProcess(app.config, processName);
    app.scheduleSave(true);
  }

  onMount(() => {
    void refresh();
  });
</script>

<div class="apps">
  <div class="head">
    <p class="dim">
      Apps that have a window open, plus ones ScreenBound has seen before. Turn one on to constrain its fullscreen
      windows to your zone. Windows has no list of "media players", so this is how a player like VLC gets included.
    </p>
    <button onclick={() => refresh()} disabled={loading}>{loading ? "Refreshing…" : "Refresh"}</button>
  </div>

  <input class="search" bind:value={query} placeholder="Filter by name or process" />

  {#if !app.config?.zones.length}
    <p class="dim">Add a zone first. Toggles need a zone to constrain fullscreen into.</p>
  {:else if rows.length === 0}
    <p class="dim">No apps yet. Open one, then refresh.</p>
  {:else}
    <ul>
      {#each rows as row (row.processName.toLowerCase())}
        <li>
          <div class="info">
            <span class="title">{row.title && row.title.toLowerCase() !== row.processName.toLowerCase() ? row.title : row.processName}</span>
            <span class="dim">
              {#if row.title && row.title.toLowerCase() !== row.processName.toLowerCase()}
                {row.processName} ·
              {/if}
              {row.running ? "Running" : "Not running"}
            </span>
          </div>
          <input
            type="checkbox"
            checked={row.applied}
            aria-label="Constrain {row.processName} when it is fullscreen"
            onchange={() => toggle(row.processName, row.applied)}
          />
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .apps {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 720px;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .head p {
    margin: 0;
  }
  .search {
    width: 100%;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-1);
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }
  li:last-child {
    border-bottom: 0;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  input[type="checkbox"] {
    margin-left: auto;
    flex: none;
  }
</style>
