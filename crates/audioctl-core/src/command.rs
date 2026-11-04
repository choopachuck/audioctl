//! Command language shared by the CLI, HTTP, WebSocket, MQTT, hotkeys and the tray menu.
//!
//! ```text
//! Headphones v 30          volume of the device named Headphones
//! out v+5  m               default output, relative volume, then mute
//! o next                   next output device
//! i Mic                    default microphone
//! "USB Headset" d          make that device the default for its direction
//! ```

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Value {
    Abs(u32),
    Rel(i32),
}

impl Value {
    pub fn apply(self, current: u32) -> u32 {
        let v = match self {
            Value::Abs(v) => v as i64,
            Value::Rel(d) => current as i64 + d as i64,
        };
        v.clamp(0, 100) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Step {
    /// Device for the following volume / mute / default steps. `out` and `in` mean the current defaults.
    Select { device: String },
    Volume { value: Value },
    Mute,
    Unmute,
    ToggleMute,
    MakeDefault,
    /// `next`, `prev`, or a device name. Changes the default output.
    Output { target: String },
    /// `next`, `prev`, or a device name. Changes the default microphone.
    Input { target: String },
    ReturnVolume,
    Pause { ms: u64 },
    Quit,
    Restart,
    Run { program: String },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("'{0}' expects a value")]
    MissingValue(String),
    #[error("invalid value '{1}' for '{0}'")]
    InvalidValue(String, String),
    #[error("command line is longer than {0} characters")]
    TooLong(usize),
}

pub const MAX_LEN: usize = 4096;

pub fn parse(line: &str) -> Result<Vec<Step>, ParseError> {
    if line.len() > MAX_LEN {
        return Err(ParseError::TooLong(MAX_LEN));
    }
    parse_tokens(&tokenize(line))
}

fn tokenize(line: &str) -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();
    let mut chars = line.chars().peekable();
    let mut cur = String::new();
    let flush = |cur: &mut String, raw: &mut Vec<String>| {
        if !cur.is_empty() {
            raw.push(std::mem::take(cur));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '#' => break,
            '"' => {
                flush(&mut cur, &mut raw);
                let quoted: String = chars.by_ref().take_while(|&c| c != '"').collect();
                raw.push(quoted.trim().to_string());
            }
            ';' | ',' => {
                flush(&mut cur, &mut raw);
                raw.push(c.to_string());
            }
            '!' if cur.is_empty() => {
                flush(&mut cur, &mut raw);
                let rest: String = chars.by_ref().collect();
                raw.push("!".into());
                raw.push(rest.trim().to_string());
            }
            c if c.is_whitespace() => flush(&mut cur, &mut raw),
            c => cur.push(c),
        }
    }
    flush(&mut cur, &mut raw);

    let mut out = Vec::with_capacity(raw.len());
    for tok in raw {
        if let Some((cmd, rest)) = split_compact(&tok) {
            out.push(cmd.to_string());
            push_signed(&mut out, rest);
        } else if tok.len() > 1 && (tok.starts_with('+') || tok.starts_with('-')) {
            push_signed(&mut out, &tok);
        } else {
            out.push(tok);
        }
    }
    out
}

fn push_signed(out: &mut Vec<String>, s: &str) {
    if let Some(rest) = s.strip_prefix('+') {
        out.push("+".into());
        if !rest.is_empty() {
            out.push(rest.into());
        }
    } else if let Some(rest) = s.strip_prefix('-') {
        out.push("-".into());
        if !rest.is_empty() {
            out.push(rest.into());
        }
    } else {
        out.push(s.into());
    }
}

/// `v-10` and `v+5` become a command plus a signed value. Other tokens stay whole.
fn split_compact(tok: &str) -> Option<(&str, &str)> {
    let lower = tok.to_ascii_lowercase();
    let rest = lower.strip_prefix('v')?;
    let digits = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    let numeric = !rest.is_empty() && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit());
    if numeric || rest == "+" || rest == "-" {
        Some((&tok[..1], &tok[1..]))
    } else {
        None
    }
}

fn is_command(tok: &str) -> bool {
    matches!(
        tok.to_ascii_lowercase().as_str(),
        "v" | "m" | "u" | "t" | "d" | "o" | "i" | "g" | "q" | "z" | ";" | "," | "!"
    )
}

struct Cursor<'a> {
    toks: &'a [String],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn next(&mut self) -> Option<&'a str> {
        let t = self.toks.get(self.i)?;
        self.i += 1;
        Some(t.as_str())
    }

    fn value(&mut self, cmd: &str) -> Result<Value, ParseError> {
        let t = self.next().ok_or_else(|| ParseError::MissingValue(cmd.into()))?;
        let num = |s: &str| -> Result<u32, ParseError> {
            s.parse().map_err(|_| ParseError::InvalidValue(cmd.into(), s.into()))
        };
        match t {
            "+" | "-" => {
                let n = self.next().ok_or_else(|| ParseError::MissingValue(cmd.into()))?;
                let n = num(n)? as i32;
                Ok(Value::Rel(if t == "-" { -n } else { n }))
            }
            _ => Ok(Value::Abs(num(t)?)),
        }
    }

    /// Words up to the next command. `next` and `prev` stay a single word.
    fn target(&mut self, cmd: &str) -> Result<String, ParseError> {
        let first = self.next().ok_or_else(|| ParseError::MissingValue(cmd.into()))?.to_string();
        if first.eq_ignore_ascii_case("next") || first.eq_ignore_ascii_case("prev") || first.eq_ignore_ascii_case("previous") {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.toks.get(self.i).is_some_and(|t| !is_command(t)) {
            parts.push(self.next().unwrap().to_string());
        }
        Ok(parts.join(" "))
    }
}

fn parse_tokens(toks: &[String]) -> Result<Vec<Step>, ParseError> {
    let mut c = Cursor { toks, i: 0 };
    let mut steps = Vec::new();
    while c.i < toks.len() {
        if is_command(&toks[c.i]) {
            let tok = c.next().unwrap();
            let lower = tok.to_ascii_lowercase();
            let step = match lower.as_str() {
                "v" => Step::Volume { value: c.value(tok)? },
                "m" => Step::Mute,
                "u" => Step::Unmute,
                "t" => Step::ToggleMute,
                "d" => Step::MakeDefault,
                "o" => Step::Output { target: c.target(tok)? },
                "i" => Step::Input { target: c.target(tok)? },
                "g" => Step::ReturnVolume,
                ";" => Step::Pause { ms: 100 },
                "," => Step::Pause { ms: 1000 },
                "q" => Step::Quit,
                "z" => Step::Restart,
                "!" => Step::Run { program: c.target(tok)? },
                _ => unreachable!(),
            };
            steps.push(step);
        } else {
            steps.push(Step::Select { device: c.target("device")? });
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_forms_and_device_prefix() {
        assert_eq!(
            parse("Headphones v+10").unwrap(),
            vec![Step::Select { device: "Headphones".into() }, Step::Volume { value: Value::Rel(10) }]
        );
        assert_eq!(
            parse("out v 30 m").unwrap(),
            vec![
                Step::Select { device: "out".into() },
                Step::Volume { value: Value::Abs(30) },
                Step::Mute,
            ]
        );
        assert_eq!(parse("v - 5").unwrap(), vec![Step::Volume { value: Value::Rel(-5) }]);
        assert_eq!(parse("o next").unwrap(), vec![Step::Output { target: "next".into() }]);
        assert_eq!(parse("i USB Mic v 0").unwrap(), vec![
            Step::Input { target: "USB Mic".into() },
            Step::Volume { value: Value::Abs(0) },
        ]);
        assert_eq!(parse("\"USB Headset\" d").unwrap(), vec![
            Step::Select { device: "USB Headset".into() },
            Step::MakeDefault,
        ]);
        assert_eq!(parse("g # comment").unwrap(), vec![Step::ReturnVolume]);
        assert!(parse("v").is_err());
    }
}
