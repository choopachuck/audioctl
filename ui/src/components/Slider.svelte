<script lang="ts">
  let { value, step = 1, oncommit }: { value: number; step?: number; oncommit: (value: number) => void } = $props();
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? value);

  function commit(next: number) {
    dragging = null;
    oncommit(Math.max(0, Math.min(100, Math.round(next))));
  }
</script>

<input
  type="range"
  min="0"
  max="100"
  {step}
  value={shown}
  onpointerdown={() => (dragging = value)}
  oninput={(e) => (dragging = Number((e.target as HTMLInputElement).value))}
  onpointerup={() => dragging !== null && commit(dragging)}
  onchange={(e) => commit(Number((e.target as HTMLInputElement).value))}
  onwheel={(e) => {
    e.preventDefault();
    commit(shown + (e.deltaY < 0 ? step : -step));
  }}
/>

<style>
  input { width: 100%; accent-color: var(--accent); }
</style>
