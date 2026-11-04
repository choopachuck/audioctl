//! Segoe MDL2 / Fluent Icons codepoints. Fluent Icons keeps the MDL2 private-use values.

use crate::model::{Flow, FormFactor};

pub struct Glyph {
    pub name: &'static str,
    pub ch: char,
}

pub const GLYPHS: &[Glyph] = &[
    Glyph { name: "speakers", ch: '\u{E7F5}' },
    Glyph { name: "headphones", ch: '\u{E7F6}' },
    Glyph { name: "headset", ch: '\u{E95B}' },
    Glyph { name: "microphone", ch: '\u{E720}' },
    Glyph { name: "monitor", ch: '\u{E7F4}' },
    Glyph { name: "tv", ch: '\u{E7F4}' },
    Glyph { name: "bluetooth", ch: '\u{E702}' },
    Glyph { name: "webcam", ch: '\u{E722}' },
    Glyph { name: "gamepad", ch: '\u{E7FC}' },
    Glyph { name: "network", ch: '\u{E968}' },
    Glyph { name: "car", ch: '\u{E804}' },
];

pub fn glyph(name: &str) -> Option<char> {
    GLYPHS.iter().find(|g| g.name.eq_ignore_ascii_case(name)).map(|g| g.ch)
}

pub fn name_of(ch: char) -> Option<&'static str> {
    GLYPHS.iter().find(|g| g.ch == ch).map(|g| g.name)
}

/// Icon name suggested by the endpoint form factor Windows reports.
pub fn default_name(flow: Flow, form: FormFactor) -> &'static str {
    match form {
        FormFactor::Headphones => "headphones",
        FormFactor::Headset => "headset",
        FormFactor::Microphone | FormFactor::Handset => "microphone",
        FormFactor::Digital => "monitor",
        FormFactor::Network => "network",
        FormFactor::Speakers | FormFactor::Line | FormFactor::Spdif => "speakers",
        FormFactor::Unknown => match flow {
            Flow::Render => "speakers",
            Flow::Capture => "microphone",
        },
    }
}

pub fn default_color(flow: Flow) -> &'static str {
    match flow {
        Flow::Render => "#64D2FF",
        Flow::Capture => "#30D158",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_glyphs_resolve() {
        assert_eq!(glyph("headphones"), Some('\u{E7F6}'));
        assert_eq!(glyph("nope"), None);
        assert_eq!(default_name(Flow::Render, FormFactor::Digital), "monitor");
        assert_eq!(default_name(Flow::Capture, FormFactor::Unknown), "microphone");
    }
}
