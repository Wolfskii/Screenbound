<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import DiagnosticsPanel from "$lib/components/DiagnosticsPanel.svelte";
  import RulesPanel from "$lib/components/RulesPanel.svelte";
  import ZonesPanel from "$lib/components/ZonesPanel.svelte";
  import { api } from "$lib/api";
  import { app } from "$lib/state.svelte";

  type Tab = "zones" | "rules" | "diagnostics";
  let tab = $state<Tab>("zones");

  onMount(() => void app.init());
  onDestroy(() => app.dispose());

  const managedCount = $derived(app.status?.managed.length ?? 0);
</script>

<div class="shell">
  <header>
    <div class="brand">ScreenBound</div>
    <nav>
      {#each [["zones", "Zones"], ["rules", "Rules"], ["diagnostics", "Diagnostics"]] as [id, label] (id)}
        <button class:active={tab === id} onclick={() => (tab = id as Tab)}>{label}</button>
      {/each}
    </nav>
    <div class="spacer"></div>
    <span class="dim save">
      {#if app.saveState === "pending" || app.saveState === "saving"}Saving…{:else if app.saveState === "saved"}Saved{/if}
    </span>
    {#if managedCount > 0}
      <button class="managed" onclick={() => api.releaseAll()} title="Restore all constrained windows">
        {managedCount} constrained · Release
      </button>
    {/if}
    <label class="enable">
      <input
        type="checkbox"
        checked={app.config?.enabled ?? false}
        disabled={!app.config}
        onchange={(e) => app.setEnabled(e.currentTarget.checked)}
      />
      Enabled
    </label>
  </header>

  {#if app.error}
    <div class="error">
      {app.error}
      <button onclick={() => (app.error = null)}>Dismiss</button>
    </div>
  {/if}

  <main>
    {#if !app.config}
      <p class="dim">Loading…</p>
    {:else if tab === "zones"}
      <ZonesPanel />
    {:else if tab === "rules"}
      <RulesPanel />
    {:else}
      <DiagnosticsPanel />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-1);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.3px;
  }
  nav {
    display: flex;
    gap: 2px;
  }
  nav button {
    background: transparent;
    border-color: transparent;
  }
  nav button.active {
    background: var(--surface-2);
    border-color: var(--border-strong);
  }
  .spacer {
    flex: 1;
  }
  .save {
    font-size: 12px;
  }
  .managed {
    border-color: var(--accent);
    color: var(--accent);
  }
  .enable {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    color: var(--text);
    font-size: 13px;
  }
  .error {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 14px;
    background: rgba(255, 107, 107, 0.12);
    color: var(--error);
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 12px 14px;
    min-height: 0;
  }
</style>
