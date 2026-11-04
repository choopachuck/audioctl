//! `audioctl.exe out v 30` forwards the command to the running instance and exits.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use anyhow::{bail, Context};
use audioctl_core::Config;
use serde_json::{json, Value};

pub const HIDDEN_FLAGS: &[&str] = &["--hidden", "--minimized", "/hidden"];

pub fn command_from_args(args: &[String]) -> Option<String> {
    let parts: Vec<&str> = args.iter().map(String::as_str).filter(|a| !HIDDEN_FLAGS.contains(a)).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

pub fn forward(cfg: &Config, line: &str) -> anyhow::Result<Value> {
    if !cfg.api.enabled {
        bail!("API disabled");
    }
    let mut addr: SocketAddr = cfg.api.bind.parse().context("api.bind")?;
    if addr.ip().is_unspecified() {
        addr.set_ip([127, 0, 0, 1].into());
    }
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(400))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let body = json!({ "command": line }).to_string();
    let auth = cfg.api.token.as_deref().filter(|t| !t.is_empty()).map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    write!(
        stream,
        "POST /api/command HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{auth}\r\n{body}",
        body.len()
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (head, body) = response.split_once("\r\n\r\n").context("malformed response")?;
    let status: u16 = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status != 200 {
        bail!("{}", value.get("error").and_then(Value::as_str).unwrap_or(head));
    }
    Ok(value)
}

pub fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_flags() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(command_from_args(&a(&["--hidden"])), None);
        assert_eq!(command_from_args(&a(&["Headphones", "v", "30"])).as_deref(), Some("Headphones v 30"));
    }
}
