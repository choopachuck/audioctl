import { invoke } from "@tauri-apps/api/core";

export interface Device {
  id: string;
  key: string;
  index: number;
  name: string;
  alias: string | null;
  display_name: string;
  flow: "render" | "capture";
  form_factor: string;
  adapter: string;
  volume: number;
  muted: boolean;
  is_default: boolean;
  is_default_communications: boolean;
  hidden: boolean;
  in_cycle: boolean;
  icon_glyph: string;
  icon_char: string;
  icon_color: string;
  icon_file: string | null;
}

export interface IconConfig {
  glyph?: string;
  color?: string | null;
  file?: string;
}

export interface DeviceConfig {
  id: string;
  alias: string | null;
  hidden: boolean;
  in_cycle: boolean;
  icon: IconConfig | null;
}

export interface Hotkey {
  keys: string;
  command: string;
}

export interface MenuEntry {
  label: string;
  command: string;
}

export interface Config {
  general: { start_with_windows: boolean; set_communications_default: boolean; volume_step: number };
  api: { enabled: boolean; bind: string; token: string | null };
  mqtt: {
    enabled: boolean;
    host: string;
    port: number;
    username: string | null;
    password: string | null;
    discovery_prefix: string;
    base_topic: string;
    node_id: string | null;
    discovery: boolean;
  };
  tray: {
    enabled: boolean;
    wheel_step: number;
    show_input_icon: boolean;
    show_volume_badge: boolean;
    hide_window_on_blur: boolean;
  };
  device: DeviceConfig[];
  hotkey: Hotkey[];
  menu: MenuEntry[];
}

export interface AppInfo {
  version: string;
  config_path: string;
  node_id: string;
}

export interface Outcome {
  messages: string[];
  value: number | null;
  quit: boolean;
  restart: boolean;
}

export const api = {
  devices: () => invoke<Device[]>("get_devices"),
  patch: (device: string, patch: { volume?: number | string; muted?: boolean; default?: boolean }) =>
    invoke<Device>("patch_device", { device, patch }),
  command: (command: string) => invoke<Outcome>("execute_command", { command }),
  rescan: () => invoke<Device[]>("rescan"),
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  testMqtt: (mqtt: Config["mqtt"]) => invoke<string>("test_mqtt", { mqtt }),
  appInfo: () => invoke<AppInfo>("app_info"),
  importIcon: (source: string) => invoke<string>("import_icon", { source }),
  iconData: (path: string) => invoke<string>("icon_data_url", { path }),
};
