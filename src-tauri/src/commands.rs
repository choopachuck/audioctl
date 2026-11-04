use std::path::{Path, PathBuf};

use audioctl_core::config::Mqtt;
use audioctl_core::patch::DevicePatch;
use audioctl_core::{Config, DeviceSnapshot, Engine, ExecOutcome};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_devices(engine: State<Engine>) -> Vec<DeviceSnapshot> {
    engine.devices()
}

#[tauri::command]
pub fn patch_device(engine: State<Engine>, device: String, patch: DevicePatch) -> Result<DeviceSnapshot, String> {
    engine.apply_patch(&device, &patch).map_err(err)
}

#[tauri::command]
pub fn execute_command(engine: State<Engine>, command: String) -> Result<ExecOutcome, String> {
    engine.execute(&command).map_err(err)
}

#[tauri::command]
pub fn rescan(engine: State<Engine>) -> Result<Vec<DeviceSnapshot>, String> {
    engine.rescan().map_err(err)
}

#[tauri::command]
pub fn get_config(engine: State<Engine>) -> Config {
    engine.config()
}

#[tauri::command]
pub fn save_config(app: AppHandle, engine: State<Engine>, config: Config) -> Result<(), String> {
    let autostart = config.general.start_with_windows;
    engine.update_config(|current| *current = config).map_err(err)?;
    let launch = app.autolaunch();
    if autostart { let _ = launch.enable(); } else { let _ = launch.disable(); }
    Ok(())
}

#[tauri::command]
pub fn test_mqtt(mqtt: Mqtt) -> Result<String, String> {
    audioctl_server::mqtt::test_connection(&mqtt).map_err(err)
}

#[tauri::command]
pub fn app_info(engine: State<Engine>) -> Value {
    let cfg = engine.config();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "config_path": Config::default_path(),
        "node_id": audioctl_server::mqtt::node_id(&cfg.mqtt),
    })
}

#[tauri::command]
pub fn import_icon(source: String) -> Result<String, String> {
    let source = PathBuf::from(source);
    let dir = Config::icons_dir();
    std::fs::create_dir_all(&dir).map_err(err)?;
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp" | "gif" | "ico") {
        return Err("choose a png, jpg, bmp, gif or ico file".into());
    }
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("icon");
    let stem: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).take(40).collect();
    let mut name = format!("{stem}.{ext}");
    let mut n = 1;
    while dir.join(&name).exists() {
        name = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    std::fs::copy(&source, dir.join(&name)).map_err(err)?;
    Ok(format!("icons/{name}"))
}

#[tauri::command]
pub fn icon_data_url(path: String) -> Result<String, String> {
    let full = resolve_icon(&path)?;
    let bytes = std::fs::read(&full).map_err(err)?;
    let mime = match full.extension().and_then(|e| e.to_str()).unwrap_or("png") {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "image/png",
    };
    Ok(format!("data:{mime};base64,{}", base64(&bytes)))
}

fn resolve_icon(path: &str) -> Result<PathBuf, String> {
    let full = {
        let path = Path::new(path);
        if path.is_absolute() { path.to_path_buf() } else { Config::dir().join(path) }
    };
    let icons = Config::icons_dir().canonicalize().map_err(|_| "icons folder is missing".to_string())?;
    let full = full.canonicalize().map_err(|_| "icon file was not found".to_string())?;
    if !full.starts_with(&icons) {
        return Err("icon is outside the icons folder".into());
    }
    Ok(full)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}
