use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub api: Api,
    pub mqtt: Mqtt,
    pub tray: Tray,
    #[serde(rename = "device")]
    pub devices: Vec<DeviceConfig>,
    #[serde(rename = "hotkey")]
    pub hotkeys: Vec<Hotkey>,
    #[serde(rename = "menu")]
    pub menu: Vec<MenuEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub start_with_windows: bool,
    /// Also move the communications role when picking the default device.
    pub set_communications_default: bool,
    /// Step used by the volume slider in the window.
    pub volume_step: u32,
}

impl Default for General {
    fn default() -> Self {
        Self { start_with_windows: false, set_communications_default: true, volume_step: 5 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Api {
    pub enabled: bool,
    /// `0.0.0.0:7686` listens beyond this PC. Set a token in that case.
    pub bind: String,
    pub token: Option<String>,
}

impl Default for Api {
    fn default() -> Self {
        Self { enabled: true, bind: "127.0.0.1:7686".into(), token: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Mqtt {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub discovery_prefix: String,
    pub base_topic: String,
    pub node_id: Option<String>,
    /// Publish Home Assistant MQTT discovery. Turn this off when the audioctl
    /// HACS integration is installed, otherwise every entity appears twice.
    #[serde(default = "default_true")]
    pub discovery: bool,
}

impl Default for Mqtt {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "homeassistant.local".into(),
            port: 1883,
            username: None,
            password: None,
            discovery_prefix: "homeassistant".into(),
            base_topic: "audioctl".into(),
            node_id: None,
            discovery: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tray {
    pub enabled: bool,
    pub wheel_step: u32,
    /// Second icon for the default microphone.
    pub show_input_icon: bool,
    /// Draw the volume number on the tray icon.
    pub show_volume_badge: bool,
    pub hide_window_on_blur: bool,
}

impl Default for Tray {
    fn default() -> Self {
        Self {
            enabled: true,
            wheel_step: 5,
            show_input_icon: false,
            show_volume_badge: false,
            hide_window_on_blur: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum IconConfig {
    File { file: String },
    Glyph { glyph: String, #[serde(default)] color: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    pub id: String,
    pub alias: Option<String>,
    pub hidden: bool,
    #[serde(default = "default_true")]
    pub in_cycle: bool,
    pub icon: Option<IconConfig>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self { id: String::new(), alias: None, hidden: false, in_cycle: true, icon: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hotkey {
    pub keys: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    pub command: String,
}

impl Config {
    pub fn dir() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("audioctl")
    }

    pub fn default_path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn icons_dir() -> PathBuf {
        Self::dir().join("icons")
    }

    pub fn load(path: &Path) -> anyhow::Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn device(&self, id: &str) -> Option<&DeviceConfig> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn device_mut(&mut self, id: &str) -> &mut DeviceConfig {
        if let Some(i) = self.devices.iter().position(|d| d.id == id) {
            &mut self.devices[i]
        } else {
            self.devices.push(DeviceConfig { id: id.to_string(), ..Default::default() });
            self.devices.last_mut().unwrap()
        }
    }
}

/// `icons/foo.png` is relative to the config directory. Absolute paths are kept.
pub fn resolve_icon_file(file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    if path.is_absolute() {
        path
    } else {
        Config::dir().join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = Config::default();
        c.device_mut("{0.0.0.00000000}.{spk}").alias = Some("Desk".into());
        c.device_mut("{0.0.0.00000000}.{spk}").icon =
            Some(IconConfig::Glyph { glyph: "headphones".into(), color: Some("#64D2FF".into()) });
        c.hotkeys.push(Hotkey { keys: "Ctrl+Alt+F1".into(), command: "o next".into() });
        c.menu.push(MenuEntry { label: "Quiet".into(), command: "out v 10".into() });
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.devices[0].alias.as_deref(), Some("Desk"));
        assert_eq!(back.hotkeys[0].command, "o next");
        assert_eq!(back.api.bind, "127.0.0.1:7686");
        assert!(back.devices[0].in_cycle);
    }

    #[test]
    fn empty_file_gives_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert!(c.api.enabled);
        assert!(c.mqtt.discovery);
        assert_eq!(c.tray.wheel_step, 5);
        assert!(c.general.set_communications_default);
    }

    #[test]
    fn mqtt_without_discovery_field_keeps_discovery_on() {
        let c: Config = toml::from_str("[mqtt]\nenabled = true\n").unwrap();
        assert!(c.mqtt.discovery);
        assert_eq!(c.mqtt.base_topic, "audioctl");
    }

    #[test]
    fn icon_file_or_glyph() {
        let file: IconConfig = toml::from_str("file = \"icons/a.png\"").unwrap();
        assert!(matches!(file, IconConfig::File { .. }));
        let glyph: IconConfig = toml::from_str("glyph = \"headset\"\ncolor = \"#fff\"").unwrap();
        assert!(matches!(glyph, IconConfig::Glyph { .. }));
    }
}
