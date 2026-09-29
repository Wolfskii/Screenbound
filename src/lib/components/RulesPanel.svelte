<script lang="ts">
  import { app, newId } from "$lib/state.svelte";
  import type { Action, ChromeMode, Matcher, WindowRule } from "$lib/types";

  /** Rules built from a plain list of process names are editable as text; others are shown raw. */
  function processNames(m: Matcher): string[] | null {
    if (m.type === "processName") return [m.value];
    if (m.type === "any" && m.value.every((x) => x.type === "processName")) {
      return m.value.map((x) => x.value as string);
    }
    return null;
  }

  function matcherFromText(text: string): Matcher {
    const names = text
      .split(/[,\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    return names.length === 1
      ? { type: "processName", value: names[0] }
      : { type: "any", value: names.map((value) => ({ type: "processName", value })) };
  }

  function zoneAction(rule: WindowRule): Action | undefined {
    return rule.actions.find((a) => a.type === "fullscreenZone");
  }

  function addRule() {
    const zoneId = app.config?.zones[0]?.id;
    if (!app.config || !zoneId) return;
    app.config.rules.push({
      id: newId("rule"),
      name: "New rule",
      enabled: true,
      matcher: { type: "any", value: [] },
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
      Rules are checked top to bottom; the first enabled match wins. When a matching window enters fullscreen,
      ScreenBound constrains it to the rule's zone.
    </p>
    <button onclick={addRule} disabled={!app.config?.zones.length}>+ New rule</button>
  </div>

  {#each app.config?.rules ?? [] as rule, i (rule.id)}
    {@const names = processNames(rule.matcher)}
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
          Applications (process names)
          {#if names}
            <input
              value={names.join(", ")}
              placeholder="chrome.exe, msedge.exe"
              onchange={(e) => {
                rule.matcher = matcherFromText(e.currentTarget.value);
                app.scheduleSave(true);
              }}
            />
          {:else}
            <code class="raw">{JSON.stringify(rule.matcher)}</code>
          {/if}
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
  .raw {
    font-size: 11px;
    word-break: break-all;
  }
</style>
