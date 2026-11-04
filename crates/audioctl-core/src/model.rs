use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::glyphs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    Render,
    Capture,
}

impl Flow {
    pub fn as_str(self) -> &'static str {
        match self {
            Flow::Render => "render",
            Flow::Capture => "capture",
        }
    }
}

impl std::fmt::Display for Flow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Flow::Render => "output",
            Flow::Capture => "input",
        })
    }
}

/// `PKEY_AudioEndpoint_FormFactor` values that matter for the default icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FormFactor {
    Speakers,
    Headphones,
    Headset,
    Microphone,
    Handset,
    Line,
    Spdif,
    /// HDMI / DisplayPort audio and other digital endpoints.
    Digital,
    Network,
    #[default]
    Unknown,
}

pub fn key_from_id(id: &str) -> String {
    let mut key = String::with_capacity(id.len());
    for c in id.chars() {
        if c.is_ascii_alphanumeric() {
            key.push(c.to_ascii_lowercase());
        } else if !key.ends_with('_') {
            key.push('_');
        }
    }
    key.trim_matches('_').to_string()
}

/// What Windows reported. Alias, icon and visibility come from the config at snapshot time.
#[derive(Debug, Clone)]
pub struct LiveDevice {
    pub id: String,
    pub name: String,
    pub adapter: String,
    pub flow: Flow,
    pub form_factor: FormFactor,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
    pub is_default_communications: bool,
}

/// API, UI and MQTT view of one endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceSnapshot {
    pub id: String,
    pub key: String,
    pub index: usize,
    pub name: String,
    pub alias: Option<String>,
    pub display_name: String,
    pub flow: Flow,
    pub form_factor: FormFactor,
    pub adapter: String,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
    pub is_default_communications: bool,
    pub hidden: bool,
    pub in_cycle: bool,
    /// Glyph name (`headphones`) even when a custom file is set, so the UI can fall back.
    pub icon_glyph: String,
    /// The single character drawn with Segoe Fluent Icons / MDL2 Assets.
    pub icon_char: String,
    pub icon_color: String,
    /// Path relative to the audioctl config directory, or absolute.
    pub icon_file: Option<String>,
}

/// Unique labels for Home Assistant selects. Duplicates gain the device index.
pub fn choice_labels(devices: &[DeviceSnapshot], flow: Flow) -> Vec<(String, String)> {
    let rows: Vec<&DeviceSnapshot> = devices.iter().filter(|d| d.flow == flow && !d.hidden).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for d in &rows {
        *counts.entry(d.display_name.as_str()).or_default() += 1;
    }
    rows.into_iter()
        .map(|d| {
            let label = if counts[d.display_name.as_str()] > 1 {
                format!("{} ({})", d.display_name, d.index)
            } else {
                d.display_name.clone()
            };
            (d.id.clone(), label)
        })
        .collect()
}

pub(crate) fn icon_for(device: &LiveDevice, icon: Option<&crate::config::IconConfig>) -> (String, String, String, Option<String>) {
    let mut glyph_name = glyphs::default_name(device.flow, device.form_factor).to_string();
    let mut color = glyphs::default_color(device.flow).to_string();
    let mut file = None;
    if let Some(icon) = icon {
        match icon {
            crate::config::IconConfig::File { file: path } => {
                if !path.trim().is_empty() {
                    file = Some(path.clone());
                }
            }
            crate::config::IconConfig::Glyph { glyph, color: custom } => {
                if glyphs::glyph(glyph).is_some() {
                    glyph_name = glyph.to_ascii_lowercase();
                }
                if let Some(c) = custom.clone().filter(|c| !c.is_empty()) {
                    color = c;
                }
            }
        }
    }
    let ch = glyphs::glyph(&glyph_name).unwrap_or('\u{E7F5}');
    (glyph_name, ch.to_string(), color, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, name: &str, flow: Flow, index: usize) -> DeviceSnapshot {
        DeviceSnapshot {
            id: id.into(),
            key: key_from_id(id),
            index,
            name: name.into(),
            alias: None,
            display_name: name.into(),
            flow,
            form_factor: FormFactor::Speakers,
            adapter: String::new(),
            volume: 0,
            muted: false,
            is_default: false,
            is_default_communications: false,
            hidden: false,
            in_cycle: true,
            icon_glyph: "speakers".into(),
            icon_char: "\u{E7F5}".into(),
            icon_color: "#fff".into(),
            icon_file: None,
        }
    }

    #[test]
    fn key_strips_punctuation() {
        assert_eq!(key_from_id("{0.0.0.00000000}.{ABC}"), "0_0_0_00000000_abc");
    }

    #[test]
    fn duplicate_names_get_an_index() {
        let devices = vec![snap("a", "Headphones", Flow::Render, 1), snap("b", "Headphones", Flow::Render, 2)];
        let labels = choice_labels(&devices, Flow::Render);
        assert_eq!(labels[0].1, "Headphones (1)");
        assert_eq!(labels[1].1, "Headphones (2)");
    }
}
