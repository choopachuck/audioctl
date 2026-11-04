<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type IconConfig } from "../lib/api";
  import { GLYPHS } from "../lib/glyphs";
  import { app, loadFile, toast } from "../lib/store.svelte";

  let { icon, onchange }: { icon: IconConfig | null; onchange: (icon: IconConfig) => void } = $props();
  const glyph = $derived(icon?.file ? "" : icon?.glyph || "speakers");
  const color = $derived(icon?.color || "#64D2FF");

  async function pickFile() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "bmp", "gif", "ico"] }],
    });
    if (typeof picked !== "string") return;
    try {
      const path = await api.importIcon(picked);
      await loadFile(path);
      onchange({ file: path });
    } catch (e) {
      toast(String(e));
    }
  }
</script>

<div class="grid">
  {#each GLYPHS as item (item.name)}
    <button
      class="glyph"
      class:on={glyph === item.name}
      style:color
      title={item.name}
      onclick={() => onchange({ glyph: item.name, color })}
    >{item.ch}</button>
  {/each}
</div>
<div class="row">
  <input type="color" value={color} oninput={(e) => onchange({ glyph: glyph || "speakers", color: (e.target as HTMLInputElement).value })} />
  <button onclick={pickFile}>Custom file</button>
  {#if icon?.file}
    <img src={app.files[icon.file]} alt="" />
    <button onclick={() => onchange({ glyph: "speakers", color })}>Use glyph</button>
  {/if}
</div>

<style>
  .grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: 4px; margin: 8px 0; }
  .glyph { font-family: var(--symbol); font-size: 18px; justify-content: center; padding: 6px; }
  img { width: 28px; height: 28px; object-fit: contain; }
</style>
