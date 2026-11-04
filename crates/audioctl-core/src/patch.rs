//! Declarative device changes shared by the HTTP/WS API, MQTT and the GUI.

use serde::{Deserialize, Deserializer, Serialize};

use crate::command::Value;

/// Absolute number (`50`) or relative string (`"+10"`, `"-5"`, `"50"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ValueSpec(pub Value);

impl<'de> Deserialize<'de> for ValueSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(i64),
            Str(String),
        }
        let parse = |s: &str| -> Option<Value> {
            let s = s.trim();
            if let Some(r) = s.strip_prefix('+') {
                r.trim().parse().ok().map(Value::Rel)
            } else if let Some(r) = s.strip_prefix('-') {
                r.trim().parse::<i32>().ok().map(|v| Value::Rel(-v))
            } else {
                s.parse().ok().map(Value::Abs)
            }
        };
        match Raw::deserialize(d)? {
            Raw::Num(n) if (0..=100).contains(&n) => Ok(ValueSpec(Value::Abs(n as u32))),
            Raw::Num(n) => Err(serde::de::Error::custom(format!("volume {n} is outside 0..=100"))),
            Raw::Str(s) => parse(&s).map(ValueSpec).ok_or_else(|| serde::de::Error::custom(format!("invalid value '{s}'"))),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DevicePatch {
    pub volume: Option<ValueSpec>,
    pub muted: Option<bool>,
    /// `true` makes the device the default for its direction. `false` is ignored.
    pub default: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_values() {
        let p: DevicePatch = serde_json::from_str(r#"{"volume": "+5", "muted": true, "default": true}"#).unwrap();
        assert_eq!(p.volume, Some(ValueSpec(Value::Rel(5))));
        assert_eq!(p.muted, Some(true));
        assert!(serde_json::from_str::<DevicePatch>(r#"{"bogus": 1}"#).is_err());
    }
}
