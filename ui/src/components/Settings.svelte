<script lang="ts">
  import { api, type Config } from "../lib/api";
  import { app, saveConfig, toast } from "../lib/store.svelte";

  let mqttResult = $state("");
  const config = $derived(app.config);

  function save(mutate: (config: Config) => void) {
    saveConfig(mutate).catch((e) => toast(String(e)));
  }

  function token() {
    const bytes = crypto.getRandomValues(new Uint8Array(18));
    return btoa(String.fromCharCode(...bytes)).replace(/[^a-zA-Z0-9]/g, "").slice(0, 24);
  }

  function capture(event: KeyboardEvent, index: number) {
    if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return;
    event.preventDefault();
    const parts = [];
    if (event.ctrlKey) parts.push("Ctrl");
    if (event.altKey) parts.push("Alt");
    if (event.shiftKey) parts.push("Shift");
    if (event.metaKey) parts.push("Win");
    const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
    parts.push(key);
    const keys = parts.join("+");
    save((c) => (c.hotkey[index].keys = keys));
  }

  async function test() {
    if (!config) return;
    mqttResult = "Connecting…";
    try {
      mqttResult = await api.testMqtt(config.mqtt);
    } catch (e) {
      mqttResult = String(e);
    }
  }

  function addHotkey() {
    save((c) => c.hotkey.push({ keys: "Ctrl+Alt+F1", command: "o next" }));
  }
  function addMenu() {
    save((c) => c.menu.push({ label: "Quiet", command: "out v 10" }));
  }
</script>

{#if config}
  <main class="scroll form">
    <h2>General</h2>
    <label class="check"><input type="checkbox" checked={config.general.start_with_windows} onchange={(e) => save((c) => (c.general.start_with_windows = (e.target as HTMLInputElement).checked))} /> Start with Windows</label>
    <label class="check"><input type="checkbox" checked={config.general.set_communications_default} onchange={(e) => save((c) => (c.general.set_communications_default = (e.target as HTMLInputElement).checked))} /> Also set the communications device</label>
    <label class="field">Volume step <input type="number" min="1" max="50" value={config.general.volume_step} onchange={(e) => save((c) => (c.general.volume_step = Number((e.target as HTMLInputElement).value) || 5))} /></label>

    <h2>Tray</h2>
    <label class="check"><input type="checkbox" checked={config.tray.enabled} onchange={(e) => save((c) => (c.tray.enabled = (e.target as HTMLInputElement).checked))} /> Show tray icon</label>
    <label class="check"><input type="checkbox" checked={config.tray.show_input_icon} onchange={(e) => save((c) => (c.tray.show_input_icon = (e.target as HTMLInputElement).checked))} /> Second icon for the microphone</label>
    <label class="check"><input type="checkbox" checked={config.tray.show_volume_badge} onchange={(e) => save((c) => (c.tray.show_volume_badge = (e.target as HTMLInputElement).checked))} /> Volume number on the icon</label>
    <label class="check"><input type="checkbox" checked={config.tray.hide_window_on_blur} onchange={(e) => save((c) => (c.tray.hide_window_on_blur = (e.target as HTMLInputElement).checked))} /> Hide the window when it loses focus</label>
    <label class="field">Wheel step <input type="number" min="1" max="50" value={config.tray.wheel_step} onchange={(e) => save((c) => (c.tray.wheel_step = Number((e.target as HTMLInputElement).value) || 5))} /></label>

    <h2>Hotkeys</h2>
    <p class="muted">Click the key field and press the combination. A warning appears here if another program already owns it.</p>
    {#each config.hotkey as entry, i (i)}
      <div class="row">
        <input type="text" value={entry.keys} onkeydown={(e) => capture(e, i)} />
        <input class="grow" type="text" value={entry.command} onchange={(e) => save((c) => (c.hotkey[i].command = (e.target as HTMLInputElement).value))} />
        <button class="danger" onclick={() => save((c) => c.hotkey.splice(i, 1))}>Remove</button>
      </div>
    {/each}
    <button onclick={addHotkey}>Add hotkey</button>

    <h2>API</h2>
    <label class="check"><input type="checkbox" checked={config.api.enabled} onchange={(e) => save((c) => (c.api.enabled = (e.target as HTMLInputElement).checked))} /> Enabled</label>
    <label class="field">Bind <input type="text" value={config.api.bind} onchange={(e) => save((c) => (c.api.bind = (e.target as HTMLInputElement).value))} /></label>
    <label class="field">Token <input type="text" value={config.api.token ?? ""} onchange={(e) => save((c) => (c.api.token = (e.target as HTMLInputElement).value || null))} /></label>
    <button onclick={() => save((c) => (c.api.token = token()))}>Generate token</button>

    <h2>MQTT</h2>
    <label class="check"><input type="checkbox" checked={config.mqtt.enabled} onchange={(e) => save((c) => (c.mqtt.enabled = (e.target as HTMLInputElement).checked))} /> Enabled</label>
    <label class="field">Broker <input type="text" value={config.mqtt.host} onchange={(e) => save((c) => (c.mqtt.host = (e.target as HTMLInputElement).value))} /></label>
    <label class="field">Port <input type="number" value={config.mqtt.port} onchange={(e) => save((c) => (c.mqtt.port = Number((e.target as HTMLInputElement).value) || 1883))} /></label>
    <label class="field">Username <input type="text" value={config.mqtt.username ?? ""} onchange={(e) => save((c) => (c.mqtt.username = (e.target as HTMLInputElement).value || null))} /></label>
    <label class="field">Password <input type="password" value={config.mqtt.password ?? ""} onchange={(e) => save((c) => (c.mqtt.password = (e.target as HTMLInputElement).value || null))} /></label>
    <label class="field">Base topic <input type="text" value={config.mqtt.base_topic} onchange={(e) => save((c) => (c.mqtt.base_topic = (e.target as HTMLInputElement).value))} /></label>
    <label class="field">Node id <input type="text" placeholder={app.info?.node_id} value={config.mqtt.node_id ?? ""} onchange={(e) => save((c) => (c.mqtt.node_id = (e.target as HTMLInputElement).value || null))} /></label>
    <label class="field">Discovery prefix <input type="text" value={config.mqtt.discovery_prefix} onchange={(e) => save((c) => (c.mqtt.discovery_prefix = (e.target as HTMLInputElement).value))} /></label>
    <label class="check"><input type="checkbox" checked={config.mqtt.discovery} onchange={(e) => save((c) => (c.mqtt.discovery = (e.target as HTMLInputElement).checked))} /> Home Assistant MQTT discovery</label>
    <p class="muted">Turn discovery off when the audioctl HACS integration is installed, or every control appears twice.</p>
    <div class="row">
      <button onclick={test}>Test connection</button>
      {#if mqttResult}<span class="muted">{mqttResult}</span>{/if}
    </div>

    <h2>Tray menu</h2>
    {#each config.menu as entry, i (i)}
      <div class="row">
        <input type="text" value={entry.label} onchange={(e) => save((c) => (c.menu[i].label = (e.target as HTMLInputElement).value))} />
        <input class="grow" type="text" value={entry.command} onchange={(e) => save((c) => (c.menu[i].command = (e.target as HTMLInputElement).value))} />
        <button class="danger" onclick={() => save((c) => c.menu.splice(i, 1))}>Remove</button>
      </div>
    {/each}
    <button onclick={addMenu}>Add menu item</button>

    <p class="footer">audioctl {app.info?.version} · {app.info?.config_path}</p>
  </main>
{/if}
