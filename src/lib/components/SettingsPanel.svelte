<script lang="ts">
  import { app } from "$lib/state.svelte";
  import type { TransitionSpeed } from "$lib/types";

  const speeds: { id: TransitionSpeed; label: string; hint: string }[] = [
    { id: "instant", label: "Direct", hint: "No animation. The window jumps straight into the zone." },
    { id: "fast", label: "Fast", hint: "1 second." },
    { id: "normal", label: "Normal", hint: "3 seconds." },
    { id: "slow", label: "Slow", hint: "5 seconds." },
  ];

  const current = $derived(app.config?.transition ?? "normal");

  function pick(id: TransitionSpeed) {
    if (!app.config) return;
    app.config.transition = id;
    app.scheduleSave(true);
  }
</script>

<div class="settings">
  <section class="card">
    <h3>Fullscreen animation</h3>
    <p class="dim">
      When an app goes fullscreen, the window shrinks from the full monitor into your zone. When it leaves fullscreen,
      it grows back to the size it had before. Each move starts after half a second, begins slowly, and speeds up.
    </p>
    <div class="options" role="radiogroup" aria-label="Fullscreen animation speed">
      {#each speeds as speed (speed.id)}
        <button
          type="button"
          role="radio"
          aria-checked={current === speed.id}
          class:picked={current === speed.id}
          onclick={() => pick(speed.id)}
        >
          <span class="label">{speed.label}</span>
          <span class="dim">{speed.hint}</span>
        </button>
      {/each}
    </div>
  </section>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 720px;
  }
  .card p {
    margin: 0 0 10px;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .options button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    padding: 8px 10px;
    text-align: left;
    background: var(--surface-2);
    transition:
      border-color 0.2s ease,
      background 0.2s ease;
  }
  .options button.picked {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .label {
    font-weight: 600;
  }
</style>
