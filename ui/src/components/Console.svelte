<script lang="ts">
  import { api } from "../lib/api";
  import { app, toast } from "../lib/store.svelte";
  let line = $state("out v +5");

  async function run() {
    const command = line.trim();
    if (!command) return;
    try {
      const out = await api.command(command);
      for (const message of out.messages) toast(message);
      if (out.messages.length === 0) toast(command);
    } catch (e) {
      toast(String(e));
    }
  }
</script>

<main class="scroll">
  <p class="muted">Same commands as the CLI, hotkeys, MQTT and the tray menu. Examples: <code>o next</code>, <code>Headphones v 30</code>, <code>in t</code>.</p>
  <form class="row" onsubmit={(e) => { e.preventDefault(); run(); }}>
    <input class="grow" type="text" bind:value={line} />
    <button class="primary" type="submit">Run</button>
  </form>
  <pre>{app.log.join("\n")}</pre>
</main>

<style>
  pre { white-space: pre-wrap; font-family: Consolas, monospace; font-size: 12px; }
  code { font-family: Consolas, monospace; }
</style>
