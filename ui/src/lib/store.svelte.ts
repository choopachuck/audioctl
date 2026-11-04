import { listen } from "@tauri-apps/api/event";
import { api, type Config, type Device, type AppInfo } from "./api";

export type View = "devices" | "settings" | "console";

export const app = $state({
  view: "devices" as View,
  devices: [] as Device[],
  peaks: {} as Record<string, number>,
  config: null as Config | null,
  info: null as AppInfo | null,
  files: {} as Record<string, string>,
  toast: "",
  log: [] as string[],
});

let toastTimer = 0;

export function toast(message: string) {
  app.toast = message;
  app.log = [`${new Date().toLocaleTimeString()}  ${message}`, ...app.log].slice(0, 200);
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (app.toast = ""), 4000);
}

export async function init() {
  const [devices, config, info] = await Promise.all([api.devices(), api.getConfig(), api.appInfo()]);
  app.devices = devices;
  app.config = config;
  app.info = info;
  await preloadFiles(devices);
  await listen<EngineEvent>("audioctl-event", (event) => {
    apply(event.payload);
  });
}

async function preloadFiles(devices: Device[]) {
  await Promise.all(devices.filter((d) => d.icon_file).map((d) => loadFile(d.icon_file!)));
}

export async function loadFile(path: string) {
  if (app.files[path]) return app.files[path];
  const url = await api.iconData(path);
  app.files[path] = url;
  return url;
}

type EngineEvent =
  | { type: "devices_changed"; devices: Device[] }
  | { type: "device_updated"; device: Device }
  | { type: "levels"; levels: { id: string; peak: number }[] }
  | { type: "notification"; message: string }
  | { type: "config_changed" }
  | { type: "quit" }
  | { type: "restart" };

function apply(event: EngineEvent) {
  if (event.type === "devices_changed") {
    app.devices = event.devices;
    void preloadFiles(event.devices);
  } else if (event.type === "device_updated") {
    const i = app.devices.findIndex((d) => d.id === event.device.id);
    if (i >= 0) app.devices[i] = event.device;
    else app.devices = [...app.devices, event.device];
    if (event.device.icon_file) void loadFile(event.device.icon_file);
  } else if (event.type === "levels") {
    for (const level of event.levels) app.peaks[level.id] = level.peak;
  } else if (event.type === "notification") {
    toast(event.message);
  } else if (event.type === "config_changed") {
    api.getConfig().then((config) => (app.config = config)).catch((e) => toast(String(e)));
  }
}

export async function saveConfig(mutate: (config: Config) => void) {
  if (!app.config) return;
  const next = structuredClone($state.snapshot(app.config)) as Config;
  mutate(next);
  await api.saveConfig(next);
  app.config = next;
}
