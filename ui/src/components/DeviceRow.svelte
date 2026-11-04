<script lang="ts">
  import { api, type Device } from "../lib/api";
  import { app, saveConfig, toast } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import IconPicker from "./IconPicker.svelte";
  import Slider from "./Slider.svelte";

  let { device }: { device: Device } = $props();
  let open = $state(false);
  const peak = $derived(app.peaks[device.id] ?? 0);
  const step = $derived(app.config?.general.volume_step || 5);
  const configured = $derived(app.config?.device.find((d) => d.id === device.id));

  async function patch(body: { volume?: number; muted?: boolean; default?: boolean }) {
    try {
      const next = await api.patch(device.id, body);
      const i = app.devices.findIndex((d) => d.id === next.id);
      if (i >= 0) app.devices[i] = next;
    } catch (e) {
      toast(String(e));
    }
  }

  function ensure(mutate: (entry: NonNullable<typeof configured>) => void) {
    saveConfig((config) => {
      let entry = config.device.find((d) => d.id === device.id);
      if (!entry) {
        entry = { id: device.id, alias: null, hidden: false, in_cycle: true, icon: null };
        config.device.push(entry);
      }
      mutate(entry);
    }).catch((e) => toast(String(e)));
  }
</script>

<article class="card" class:dim={device.hidden}>
  <div class="row">
    {#if device.icon_file && app.files[device.icon_file]}
      <img class="file" src={app.files[device.icon_file]} alt="" />
    {:else}
      <span class="sym" style:color={device.icon_color}>{device.icon_char}</span>
    {/if}
    <div class="grow">
      <div class="row">
        <strong class="grow">{device.display_name}</strong>
        <span class="muted">{device.muted ? "muted" : `${device.volume}%`}</span>
      </div>
      <div class="sub muted">{device.adapter || device.form_factor}{device.hidden ? " · hidden" : ""}</div>
      <div class="meter"><span style:width={`${Math.round(peak * 100)}%`}></span></div>
      <Slider value={device.volume} {step} oncommit={(volume) => patch({ volume })} />
    </div>
    <button class="icon" class:on={device.muted} title="Mute" onclick={() => patch({ muted: !device.muted })}>
      <Icon name={device.muted ? "mute" : "volume"} />
    </button>
    <button class="icon" class:on={device.is_default} title="Default" onclick={() => patch({ default: true })}>●</button>
    <button class="icon" title="Device settings" onclick={() => (open = !open)}>▾</button>
  </div>
  {#if open}
    <div class="details">
      <label class="field">Alias <input type="text" value={configured?.alias ?? ""} onchange={(e) => ensure((d) => (d.alias = (e.target as HTMLInputElement).value || null))} /></label>
      <label class="check"><input type="checkbox" checked={device.hidden} onchange={(e) => ensure((d) => (d.hidden = (e.target as HTMLInputElement).checked))} /> Hidden</label>
      <label class="check"><input type="checkbox" checked={device.in_cycle} onchange={(e) => ensure((d) => (d.in_cycle = (e.target as HTMLInputElement).checked))} /> Include in next/previous</label>
      <IconPicker icon={configured?.icon ?? { glyph: device.icon_glyph, color: device.icon_color }} onchange={(icon) => ensure((d) => (d.icon = icon))} />
    </div>
  {/if}
</article>

<style>
  .dim { opacity: 0.55; }
  .file { width: 28px; height: 28px; object-fit: contain; }
  .sub { font-size: 11px; }
  .meter { height: 3px; background: var(--track); border-radius: 2px; margin: 4px 0; }
  .meter span { display: block; height: 100%; background: var(--accent); border-radius: 2px; }
  .details { display: grid; gap: 8px; margin-top: 8px; }
  .field { display: grid; gap: 4px; }
</style>
