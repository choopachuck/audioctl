<script lang="ts">
  let { name }: { name: "back" | "refresh" | "settings" | "terminal" | "close" | "mute" | "volume" } = $props();
  const paths: Record<string, string> = {
    back: "M14 6 L8 12 L14 18",
    refresh: "M19 12a7 7 0 1 1-2-5 M16 3v5h5",
    settings: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z M12 3v2 M12 19v2 M3 12h2 M19 12h2 M5.6 5.6l1.4 1.4 M17 17l1.4 1.4 M18.4 5.6 17 7 M7 17l-1.4 1.4",
    terminal: "M5 8l5 4-5 4 M12 16h7",
    close: "M7 7l10 10 M17 7L7 17",
    mute: "M6 10h3l4-3v10l-4-3H6z M16 10l4 4 M20 10l-4 4",
    volume: "M6 10h3l4-3v10l-4-3H6z M16 9a4 4 0 0 1 0 6",
  };
</script>

<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
  <path d={paths[name]} />
</svg>
