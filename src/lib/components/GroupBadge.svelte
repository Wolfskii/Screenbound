<script lang="ts">
  import { iconPath } from "$lib/groupIcons";

  let { icon, color, size = 22 }: { icon: string; color: string; size?: number } = $props();
</script>

<span class="badge" style:background={color} style:width="{size}px" style:height="{size}px">
  <svg viewBox="0 0 24 24" width={Math.round(size * 0.62)} height={Math.round(size * 0.62)} fill="none" stroke="#fff" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round">
    <path d={iconPath(icon)} />
  </svg>
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    flex: none;
  }
</style>
