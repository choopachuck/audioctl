//! `Ctrl+Alt+F1` and the Win32 modifier / virtual-key values `RegisterHotKey` expects.

use serde::{Deserialize, Serialize};

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MOD_NOREPEAT: u32 = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Combo {
    pub modifiers: u32,
    pub vk: u32,
}

pub fn parse(text: &str) -> Result<Combo, String> {
    let parts: Vec<&str> = text.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err("empty hotkey".into());
    }
    let (key, mods) = parts.split_last().unwrap();
    let mut modifiers = 0u32;
    for m in mods {
        modifiers |= match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => MOD_CONTROL,
            "alt" => MOD_ALT,
            "shift" => MOD_SHIFT,
            "win" | "super" | "meta" => MOD_WIN,
            other => return Err(format!("unknown modifier '{other}'")),
        };
    }
    let vk = virtual_key(key).ok_or_else(|| format!("unknown key '{key}'"))?;
    Ok(Combo { modifiers, vk })
}

fn virtual_key(key: &str) -> Option<u32> {
    let k = key.trim();
    let lower = k.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1);
            }
        }
    }
    if k.chars().count() == 1 {
        let c = k.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_uppercase() as u32);
        }
        if c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    Some(match lower.as_str() {
        "space" => 0x20,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "home" => 0x24,
        "end" => 0x23,
        "insert" | "ins" => 0x2D,
        "delete" | "del" => 0x2E,
        "plus" => 0xBB,
        "minus" => 0xBD,
        "comma" => 0xBC,
        "period" => 0xBE,
        "volumeup" => 0xAF,
        "volumedown" => 0xAE,
        "volumemute" => 0xAD,
        "medianext" => 0xB0,
        "mediaprev" | "mediaprevious" => 0xB1,
        "mediaplay" => 0xB3,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_function_keys() {
        let c = parse("Ctrl+Alt+F1").unwrap();
        assert_eq!(c.modifiers, MOD_CONTROL | MOD_ALT);
        assert_eq!(c.vk, 0x70);
        let c = parse("ctrl + shift + m").unwrap();
        assert_eq!(c.modifiers, MOD_CONTROL | MOD_SHIFT);
        assert_eq!(c.vk, u32::from(b'M'));
        assert!(parse("F25").is_err());
        assert!(parse("").is_err());
        assert!(parse("Ctrl+Nope").is_err());
    }
}
