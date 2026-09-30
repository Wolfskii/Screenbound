<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (next: boolean) => void } = $props();
</script>

<button
  type="button"
  class="switch"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
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
