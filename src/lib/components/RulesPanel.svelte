<script lang="ts">
  import GroupBadge from "$lib/components/GroupBadge.svelte";
  import { app, newId } from "$lib/state.svelte";
  import type { Action, ChromeMode, RuleScope, WindowRule } from "$lib/types";

  function zoneAction(rule: WindowRule): Action | undefined {
    return rule.actions.find((a) => a.type === "fullscreenZone");
  }

  function scopeKind(rule: WindowRule): "legacy" | RuleScope["type"] {
    return rule.scope?.type ?? "legacy";
  }

  function setScopeKind(rule: WindowRule, kind: string) {
    if (kind === "legacy") return;
    if (kind === "all") rule.scope = { type: "all" };
    else if (kind === "groups") rule.scope = { type: "groups", groupIds: rule.scope?.type === "groups" ? rule.scope.groupIds : [] };
    else rule.scope = { type: "apps", processNames: rule.scope?.type === "apps" ? rule.scope.processNames : [] };
    rule.matcher = { type: "any", value: [] };
    app.scheduleSave(true);
  }

  function toggleId(list: string[], id: string): string[] {
    return list.includes(id) ? list.filter((item) => item !== id) : [...list, id];
  }

  function addRule() {
    const zoneId = app.config?.zones[0]?.id;
    if (!app.config || !zoneId) return;
    app.config.rules.push({
      id: newId("rule"),
      name: "New rule",
      enabled: true,
      matcher: { type: "any", value: [] },
      scope: { type: "all" },
      actions: [{ type: "fullscreenZone", zoneId, chrome: "keep" }],
    });
    app.scheduleSave(true);
  }

  function removeRule(id: string) {
    if (!app.config) return;
    app.config.rules = app.config.rules.filter((r) => r.id !== id);
    app.scheduleSave(true);
  }

  function move(index: number, delta: number) {
    if (!app.config) return;
    const rules = app.config.rules;
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    [rules[index], rules[target]] = [rules[target], rules[index]];
    app.scheduleSave(true);
  }
</script>

<div class="rules">
  <div class="head">
    <p class="dim">
      Rules are checked top to bottom; the first enabled match wins. A rule applies to apps that are turned on:
      every one of them, specific groups, or specific apps. Put a narrower rule above an All rule.
    </p>
    <button onclick={addRule} disabled={!app.config?.zones.length}>+ New rule</button>
  </div>

  {#each app.config?.rules ?? [] as rule, i (rule.id)}
    {@const action = zoneAction(rule)}
    <div class="card rule" class:disabled={!rule.enabled}>
      <div class="row">
        <label class="toggle">
          <input type="checkbox" bind:checked={rule.enabled} onchange={() => app.scheduleSave(true)} />
        </label>
        <input class="name" bind:value={rule.name} oninput={() => app.scheduleSave()} />
        <div class="spacer"></div>
        <button title="Move up" onclick={() => move(i, -1)} disabled={i === 0}>↑</button>
        <button title="Move down" onclick={() => move(i, 1)} disabled={i === (app.config?.rules.length ?? 0) - 1}>↓</button>
        <button class="danger" onclick={() => removeRule(rule.id)}>Delete</button>
      </div>

      <div class="grid">
        <label>
          Applies to
          <select value={scopeKind(rule)} onchange={(e) => setScopeKind(rule, e.currentTarget.value)}>
            {#if !rule.scope}<option value="legacy">Previous process list</option>{/if}
            <option value="all">All apps that are on</option>
            <option value="groups">Specific groups</option>
            <option value="apps">Specific apps</option>
          </select>
        </label>

        {#if action}
          <label>
            Fullscreen zone
            <select bind:value={action.zoneId} onchange={() => app.scheduleSave(true)}>
              {#each app.config?.zones ?? [] as z (z.id)}
                <option value={z.id}>{z.name}</option>
              {/each}
            </select>
          </label>
          <label>
            Window chrome
            <select
              value={action.chrome}
              onchange={(e) => {
                action.chrome = e.currentTarget.value as ChromeMode;
                app.scheduleSave(true);
              }}
            >
              <option value="keep">Keep title bar (default)</option>
              <option value="hide">Hide title bar (borderless)</option>
            </select>
          </label>
        {/if}
      </div>

      {#if rule.scope?.type === "groups"}
        <div class="chips">
          {#each app.config?.groups ?? [] as group (group.id)}
            <button
              type="button"
              class="chip"
              class:on={rule.scope.groupIds.includes(group.id)}
              onclick={() => {
                if (rule.scope?.type !== "groups") return;
                rule.scope.groupIds = toggleId(rule.scope.groupIds, group.id);
                app.scheduleSave(true);
              }}
            >
              <GroupBadge icon={group.icon} color={group.color} size={18} />
              {group.name}
            </button>
          {:else}
            <span class="dim">No groups yet. Create them on the Groups tab.</span>
          {/each}
        </div>
      {:else if rule.scope?.type === "apps"}
        <div class="chips">
          {#each app.config?.knownApps ?? [] as known (known.processName)}
            <button
              type="button"
              class="chip"
              class:on={rule.scope.processNames.some((name) => name.toLowerCase() === known.processName.toLowerCase())}
              onclick={() => {
                if (rule.scope?.type !== "apps") return;
                const has = rule.scope.processNames.some((name) => name.toLowerCase() === known.processName.toLowerCase());
                rule.scope.processNames = has
                  ? rule.scope.processNames.filter((name) => name.toLowerCase() !== known.processName.toLowerCase())
                  : [...rule.scope.processNames, known.processName];
                app.scheduleSave(true);
              }}
            >
              {known.title && known.title.toLowerCase() !== known.processName.toLowerCase() ? known.title : known.processName}
            </button>
          {:else}
            <span class="dim">No apps seen yet. Open one, then refresh the Apps tab.</span>
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .rules {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .head p {
    margin: 0;
  }
  .rule {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .rule.disabled {
    opacity: 0.6;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .name {
    font-weight: 600;
    min-width: 220px;
  }
  .spacer {
    flex: 1;
  }
  .grid {
    display: grid;
    grid-template-columns: 2fr 1fr 1fr;
    gap: 10px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
</style>
