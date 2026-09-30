<script lang="ts">
  import { setAppsEnabled } from "$lib/apps";
  import GroupBadge from "$lib/components/GroupBadge.svelte";
  import { GROUP_COLORS, GROUP_ICONS } from "$lib/groupIcons";
  import { app, newId } from "$lib/state.svelte";
  import type { AppGroup } from "$lib/types";

  let draftName = $state("");
  let draftColor = $state(GROUP_COLORS[0]);
  let draftIcon = $state<(typeof GROUP_ICONS)[number]>("apps");
  let showCreate = $state(false);

  const groups = $derived(app.config?.groups ?? []);

  function members(groupId: string) {
    return (app.config?.knownApps ?? []).filter((item) => item.groupId === groupId);
  }

  function groupOn(groupId: string) {
    const list = members(groupId);
    return list.length > 0 && list.every((item) => item.enabled);
  }

  function toggleGroup(groupId: string, on: boolean) {
    if (!app.config?.knownApps) return;
    setAppsEnabled(
      app.config.knownApps,
      members(groupId).map((item) => item.processName),
      !on,
    );
    app.scheduleSave(true);
  }

  function addGroup() {
    if (!app.config) return;
    const group: AppGroup = {
      id: newId("group"),
      name: draftName.trim() || "Group",
      color: draftColor,
      icon: draftIcon,
    };
    app.config.groups = [...(app.config.groups ?? []), group];
    draftName = "";
    showCreate = false;
    app.scheduleSave(true);
  }

  function deleteGroup(id: string) {
    if (!app.config) return;
    app.config.groups = (app.config.groups ?? []).filter((group) => group.id !== id);
    for (const known of app.config.knownApps ?? []) {
      if (known.groupId === id) known.groupId = null;
    }
    for (const rule of app.config.rules) {
      if (rule.scope?.type === "groups") rule.scope.groupIds = rule.scope.groupIds.filter((groupId) => groupId !== id);
    }
    app.scheduleSave(true);
  }
</script>

<div class="groups">
  <div class="head">
    <p class="dim">
      Name a group, pick a color and an icon, then assign apps to it on the Apps tab. The switch turns every app in
      that group on or off. Rules can target a group.
    </p>
    {#if !showCreate}
      <button type="button" onclick={() => (showCreate = true)}>+ New group</button>
    {/if}
  </div>

  {#if showCreate}
    <form class="create card" onsubmit={(e) => { e.preventDefault(); addGroup(); }}>
      <input bind:value={draftName} placeholder="Group name" />
      <div class="swatches">
        {#each GROUP_COLORS as color (color)}
          <button type="button" class="swatch" class:picked={draftColor === color} style:background={color} aria-label={color} onclick={() => (draftColor = color)}></button>
        {/each}
      </div>
      <div class="icons">
        {#each GROUP_ICONS as icon (icon)}
          <button type="button" class="icon-btn" class:picked={draftIcon === icon} aria-label={icon} onclick={() => (draftIcon = icon)}>
            <GroupBadge {icon} color={draftColor} size={26} />
          </button>
        {/each}
      </div>
      <div class="row">
        <button type="submit">Add group</button>
        <button type="button" onclick={() => (showCreate = false)}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if groups.length === 0 && !showCreate}
    <p class="dim empty">No groups yet.</p>
  {:else if groups.length > 0}
    <ul>
      {#each groups as group (group.id)}
        {@const count = members(group.id).length}
        {@const on = groupOn(group.id)}
        <li>
          <GroupBadge icon={group.icon} color={group.color} />
          <div class="info">
            <input class="name" bind:value={group.name} aria-label="Group name" oninput={() => app.scheduleSave()} />
            <span class="dim">{count === 1 ? "1 app" : `${count} apps`}</span>
          </div>
          <div class="picks">
            {#each GROUP_COLORS as color (color)}
              <button type="button" class="swatch" class:picked={group.color === color} style:background={color} aria-label={color} onclick={() => { group.color = color; app.scheduleSave(true); }}></button>
            {/each}
            {#each GROUP_ICONS as icon (icon)}
              <button type="button" class="icon-btn" class:picked={group.icon === icon} aria-label={icon} onclick={() => { group.icon = icon; app.scheduleSave(true); }}>
                <GroupBadge {icon} color={group.color} size={20} />
              </button>
            {/each}
          </div>
          <button type="button" class="danger" onclick={() => deleteGroup(group.id)}>Delete</button>
          <button
            type="button"
            class="switch"
            class:on
            role="switch"
            aria-checked={on}
            aria-label="Turn every app in {group.name} on or off"
            disabled={count === 0}
            onclick={() => toggleGroup(group.id, on)}
          >
            <span class="knob"></span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .groups {
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
  .create {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .empty {
    margin: 0;
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
    align-items: flex-start;
    gap: 12px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  li:last-child {
    border-bottom: 0;
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 140px;
    flex: 1;
  }
  .name {
    font-weight: 600;
    min-width: 0;
  }
  .picks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    max-width: 280px;
  }
  .swatches,
  .icons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid transparent;
  }
  .swatch.picked,
  .icon-btn.picked {
    border-color: var(--text);
  }
  .icon-btn {
    padding: 0;
    border: 2px solid transparent;
    background: transparent;
    border-radius: 8px;
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
