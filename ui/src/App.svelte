<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { app, init, toast } from "./lib/store.svelte";
  import Console from "./components/Console.svelte";
  import DeviceRow from "./components/DeviceRow.svelte";
  import Icon from "./components/Icon.svelte";
  import Settings from "./components/Settings.svelte";

  let scanning = $state(false);
  const titles = { devices: "audioctl", settings: "Settings", console: "Commands" };
  const outputs = $derived(app.devices.filter((d) => d.flow === "render"));
  const inputs = $derived(app.devices.filter((d) => d.flow === "capture"));

  onMount(() => {
    init().catch((e) => toast(String(e)));
    const esc = (e: KeyboardEvent) => e.key === "Escape" && getCurrentWindow().hide();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  });

  async function rescan() {
    scanning = true;
    try {
      app.devices = await api.rescan();
    } catch (e) {
      toast(String(e));
    } finally {
      scanning = false;
    }
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  {#if app.view !== "devices"}
    <button class="icon" title="Back" onclick={() => (app.view = "devices")}><Icon name="back" /></button>
  {/if}
  <span class="title" data-tauri-drag-region>{titles[app.view]}</span>
  {#if app.view === "devices"}
    <button class="icon" class:spin={scanning} title="Rescan" onclick={rescan}><Icon name="refresh" /></button>
    <button class="icon" title="Commands" onclick={() => (app.view = "console")}><Icon name="terminal" /></button>
    <button class="icon" title="Settings" onclick={() => (app.view = "settings")}><Icon name="settings" /></button>
  {/if}
  <button class="icon" title="Hide (Esc)" onclick={() => getCurrentWindow().hide()}><Icon name="close" /></button>
</header>

{#if app.view === "devices"}
  <main class="scroll">
    <div class="section">Output</div>
    {#each outputs as device (device.id)}
      <DeviceRow {device} />
    {:else}
      <p class="muted">No playback devices.</p>
    {/each}
    <div class="section">Microphones</div>
    {#each inputs as device (device.id)}
      <DeviceRow {device} />
    {:else}
      <p class="muted">No microphones.</p>
    {/each}
  </main>
{:else if app.view === "settings"}
  <Settings />
{:else}
  <Console />
{/if}

{#if app.toast}
  <div class="toast">{app.toast}</div>
{/if}

<style>
  .titlebar { display: flex; align-items: center; gap: 2px; padding: 8px 8px 6px 14px; }
  .title { flex: 1; font-weight: 600; }
  .spin { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
