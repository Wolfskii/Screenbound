<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { appRows, clearLegacySeenApps, loadLegacySeenApps, rememberApps, setAppsEnabled } from "$lib/apps";
  import { app } from "$lib/state.svelte";

  let running = $state<string[]>([]);
  let query = $state("");
  let loading = $state(false);

  const groups = $derived(app.config?.groups ?? []);

  const allRows = $derived.by(() => {
    if (!app.config) return [];
    return appRows(app.config.knownApps ?? [], running);
  });

  const rows = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return allRows;
    return allRows.filter((row) => row.processName.toLowerCase().includes(q) || row.title.toLowerCase().includes(q));
  });

  const allOn = $derived(allRows.length > 0 && allRows.every((row) => row.applied));

  function listsMatch(next: { processName: string; title: string; enabled?: boolean; groupId?: string | null }[]): boolean {
    const current = app.config?.knownApps ?? [];
    return (
      current.length === next.length &&
      current.every(
        (item, i) =>
          item.processName === next[i]?.processName &&
          item.title === next[i]?.title &&
          !!item.enabled === !!next[i]?.enabled &&
          (item.groupId ?? null) === (next[i]?.groupId ?? null),
      )
    );
  }

  async function refresh() {
    loading = true;
    try {
      const windows = await api.listWindows();
      running = windows.map((window) => window.identity.processName).filter(Boolean);
      if (!app.config) return;
      const next = rememberApps(app.config.knownApps ?? [], windows);
      if (!listsMatch(next)) {
        app.config.knownApps = next;
        app.scheduleSave(true);
      }
    } catch (e) {
      app.error = String(e);
    } finally {
      loading = false;
    }
  }

  function toggle(processNames: string[], applied: boolean) {
    if (!app.config?.knownApps) return;
    setAppsEnabled(app.config.knownApps, processNames, !applied);
    app.scheduleSave(true);
  }

  function toggleAll() {
    toggle(
      allRows.map((row) => row.processName),
      allOn,
    );
  }

  function assignGroup(processName: string, groupId: string) {
    const known = app.config?.knownApps?.find((item) => item.processName.toLowerCase() === processName.toLowerCase());
    if (!known) return;
    known.groupId = groupId || null;
    app.scheduleSave(true);
  }

  onMount(() => {
    if (app.config && !(app.config.knownApps?.length)) {
      const legacy = loadLegacySeenApps();
      if (legacy.length) {
        app.config.knownApps = legacy;
        clearLegacySeenApps();
        app.scheduleSave(true);
      }
    }
    void refresh();
  });
</script>

<div class="apps">
  <div class="head">
    <p class="dim">
      Turn an app on to include it. Put it in a group from the menu; groups themselves are edited on the Groups tab.
    </p>
    <button onclick={() => refresh()} disabled={loading}>{loading ? "Refreshing…" : "Refresh"}</button>
  </div>

  <input class="search" bind:value={query} placeholder="Filter by name or process" />

    {#if allRows.length === 0}
      <p class="dim empty">No apps yet. Open one, then refresh.</p>
    {:else}
      <ul>
        <li class="all">
          <div class="info">
            <span class="title">All</span>
            <span class="dim">{allOn ? "Every app is on" : "Turn every app on or off"}</span>
          </div>
          <button type="button" class="switch" class:on={allOn} role="switch" aria-checked={allOn} aria-label="Turn every app on or off" onclick={toggleAll}>
            <span class="knob"></span>
          </button>
        </li>
        {#if rows.length === 0}
          <li><span class="dim">No apps match that filter.</span></li>
        {/if}
        {#each rows as row (row.processName.toLowerCase())}
          {@render appItem(row)}
        {/each}
      </ul>
    {/if}
</div>

{#snippet appItem(row: { processName: string; title: string; running: boolean; applied: boolean; groupId: string | null })}
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
    <select aria-label="Group for {row.processName}" value={row.groupId ?? ""} onchange={(e) => assignGroup(row.processName, e.currentTarget.value)}>
      <option value="">No group</option>
      {#each groups as group (group.id)}
        <option value={group.id}>{group.name}</option>
      {/each}
    </select>
    <button
      type="button"
      class="switch"
      class:on={row.applied}
      role="switch"
      aria-checked={row.applied}
      aria-label="Include {row.processName}"
      onclick={() => toggle([row.processName], row.applied)}
    >
      <span class="knob"></span>
    </button>
  </li>
{/snippet}

<style>
  .apps {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 860px;
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
  .empty {
    margin: 0;
  }
  select {
    max-width: 140px;
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
  li.all {
    background: var(--surface-2);
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
  .switch {
    position: relative;
    margin-left: auto;
    flex: none;
    width: 42px;
    height: 24px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: #0d0f13;
    transition:
      background 0.28s ease,
      border-color 0.28s ease;
  }
  .switch:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .switch:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .switch.on {
    background: var(--accent);
    border-color: #7cbcff;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #d5d8de;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
    transition:
      transform 0.28s cubic-bezier(0.4, 0.15, 0.2, 1),
      background 0.28s ease;
  }
  .switch.on .knob {
    transform: translateX(18px);
    background: white;
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .knob {
      transition: none;
    }
  }
</style>
