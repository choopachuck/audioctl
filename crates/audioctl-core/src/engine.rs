use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::backend::{self, AudioMsg, BackendKind, Driver, Notice};
use crate::command::{self, Step, Value};
use crate::config::Config;
use crate::model::{choice_labels, icon_for, key_from_id, DeviceSnapshot, Flow, LiveDevice};
use crate::patch::{DevicePatch, ValueSpec};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    DevicesChanged { devices: Vec<DeviceSnapshot> },
    DeviceUpdated { device: DeviceSnapshot },
    Levels { levels: Vec<Level> },
    Notification { message: String },
    ConfigChanged,
    Quit,
    Restart,
}

#[derive(Debug, Clone, Serialize)]
pub struct Level {
    pub id: String,
    pub peak: f32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExecOutcome {
    pub messages: Vec<String>,
    /// Volume requested by `g`, so the CLI can use it as its exit code.
    pub value: Option<i64>,
    pub quit: bool,
    pub restart: bool,
}

struct Inner {
    config: RwLock<Config>,
    config_path: Option<PathBuf>,
    devices: RwLock<Vec<LiveDevice>>,
    events: broadcast::Sender<Event>,
    tx: mpsc::Sender<AudioMsg>,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    pub fn new(config: Config, config_path: Option<PathBuf>, kind: BackendKind) -> Self {
        let (events, _) = broadcast::channel(64);
        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(Inner { config: RwLock::new(config), config_path, devices: RwLock::new(Vec::new()), events, tx: tx.clone() });
        let thread_inner = inner.clone();
        let thread_tx = tx.clone();
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let driver = backend::open(kind, thread_tx);
                run_loop(thread_inner, driver, rx);
            })
            .expect("spawn audio thread");
        Self { inner }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    pub fn config(&self) -> Config {
        self.inner.config.read().clone()
    }

    pub fn update_config(&self, f: impl FnOnce(&mut Config)) -> anyhow::Result<()> {
        {
            let mut cfg = self.inner.config.write();
            f(&mut cfg);
            if let Some(path) = &self.inner.config_path {
                cfg.save(path)?;
            }
        }
        let _ = self.inner.events.send(Event::ConfigChanged);
        Ok(())
    }

    pub fn notify(&self, message: impl Into<String>) {
        let _ = self.inner.events.send(Event::Notification { message: message.into() });
    }

    pub fn devices(&self) -> Vec<DeviceSnapshot> {
        snapshots(&self.inner)
    }

    pub fn resolve(&self, reference: &str) -> Option<DeviceSnapshot> {
        let id = self.resolve_id(reference)?;
        self.devices().into_iter().find(|d| d.id == id)
    }

    pub fn default_of(&self, flow: Flow) -> Option<DeviceSnapshot> {
        self.devices().into_iter().find(|d| d.flow == flow && d.is_default)
    }

    pub fn rescan(&self) -> anyhow::Result<Vec<DeviceSnapshot>> {
        self.call(AudioMsg::Rescan { reply: None }, true)?;
        Ok(self.devices())
    }

    pub fn set_volume(&self, id: &str, value: Value) -> anyhow::Result<DeviceSnapshot> {
        let volume = self.preview_volume(id, value)?;
        self.call(AudioMsg::SetVolume { id: id.to_string(), volume, reply: None }, true)?;
        self.snapshot(id)
    }

    pub fn set_mute(&self, id: &str, muted: bool) -> anyhow::Result<DeviceSnapshot> {
        self.call(AudioMsg::SetMute { id: id.to_string(), muted, reply: None }, true)?;
        self.snapshot(id)
    }

    pub fn set_default(&self, id: &str) -> anyhow::Result<DeviceSnapshot> {
        let communications = self.config().general.set_communications_default;
        self.call(AudioMsg::SetDefault { id: id.to_string(), communications, reply: None }, true)?;
        self.snapshot(id)
    }

    /// Fast path for the tray wheel: the cache updates immediately and the write is queued.
    pub fn adjust_volume(&self, id: &str, value: Value) -> anyhow::Result<()> {
        let volume = self.preview_volume(id, value)?;
        let muted = self.snapshot(id)?.muted;
        if write_volume(&self.inner, id, volume, muted) {
            emit_device(&self.inner, id);
        }
        self.inner.tx.send(AudioMsg::SetVolume { id: id.to_string(), volume, reply: None }).map_err(|_| anyhow!("audio thread stopped"))?;
        Ok(())
    }

    pub fn adjust_mute(&self, id: &str) -> anyhow::Result<()> {
        let snap = self.snapshot(id)?;
        let muted = !snap.muted;
        if write_volume(&self.inner, id, snap.volume, muted) {
            emit_device(&self.inner, id);
        }
        self.inner.tx.send(AudioMsg::SetMute { id: id.to_string(), muted, reply: None }).map_err(|_| anyhow!("audio thread stopped"))?;
        Ok(())
    }

    pub fn cycle(&self, flow: Flow, dir: i32) -> anyhow::Result<DeviceSnapshot> {
        let snaps = self.devices();
        let list: Vec<&DeviceSnapshot> = snaps.iter().filter(|d| d.flow == flow && d.in_cycle && !d.hidden).collect();
        if list.is_empty() {
            anyhow::bail!("no {flow} devices to switch between");
        }
        let pos = list.iter().position(|d| d.is_default).unwrap_or(0);
        let next = (pos as i32 + dir).rem_euclid(list.len() as i32) as usize;
        self.set_default(&list[next].id)
    }

    pub fn activate(&self, flow: Flow, reference: &str) -> anyhow::Result<DeviceSnapshot> {
        let reference = reference.trim();
        if reference.eq_ignore_ascii_case("next") {
            return self.cycle(flow, 1);
        }
        if reference.eq_ignore_ascii_case("prev") || reference.eq_ignore_ascii_case("previous") {
            return self.cycle(flow, -1);
        }
        let id = self.resolve_id(reference).ok_or_else(|| anyhow!("device '{reference}' not found"))?;
        let snap = self.snapshot(&id)?;
        if snap.flow != flow {
            anyhow::bail!("{reference} is not an {flow} device");
        }
        self.set_default(&id)
    }

    pub fn apply_patch(&self, reference: &str, patch: &DevicePatch) -> anyhow::Result<DeviceSnapshot> {
        let id = self.resolve_id(reference).ok_or_else(|| anyhow!("device '{reference}' not found"))?;
        if let Some(ValueSpec(value)) = patch.volume {
            self.set_volume(&id, value)?;
        }
        if let Some(muted) = patch.muted {
            self.set_mute(&id, muted)?;
        }
        if patch.default == Some(true) {
            self.set_default(&id)?;
        }
        self.snapshot(&id)
    }

    pub fn execute(&self, line: &str) -> anyhow::Result<ExecOutcome> {
        let steps = command::parse(line)?;
        let mut outcome = ExecOutcome::default();
        let mut target = self.resolve_id("out");
        for step in steps {
            match step {
                Step::Select { device } => {
                    target = Some(self.resolve_id(&device).ok_or_else(|| anyhow!("device '{device}' not found"))?);
                }
                Step::Volume { value } => {
                    let id = need(&target)?;
                    let snap = self.set_volume(&id, value)?;
                    outcome.messages.push(format!("{} volume {}", snap.display_name, snap.volume));
                }
                Step::Mute => {
                    let id = need(&target)?;
                    let snap = self.set_mute(&id, true)?;
                    outcome.messages.push(format!("{} muted", snap.display_name));
                }
                Step::Unmute => {
                    let id = need(&target)?;
                    let snap = self.set_mute(&id, false)?;
                    outcome.messages.push(format!("{} unmuted", snap.display_name));
                }
                Step::ToggleMute => {
                    let id = need(&target)?;
                    let muted = !self.snapshot(&id)?.muted;
                    let snap = self.set_mute(&id, muted)?;
                    outcome.messages.push(format!("{} {}", snap.display_name, if snap.muted { "muted" } else { "unmuted" }));
                }
                Step::MakeDefault => {
                    let id = need(&target)?;
                    let snap = self.set_default(&id)?;
                    outcome.messages.push(format!("{} default", snap.display_name));
                }
                Step::Output { target: name } => {
                    let snap = self.activate(Flow::Render, &name)?;
                    target = Some(snap.id.clone());
                    outcome.messages.push(format!("output {}", snap.display_name));
                }
                Step::Input { target: name } => {
                    let snap = self.activate(Flow::Capture, &name)?;
                    target = Some(snap.id.clone());
                    outcome.messages.push(format!("input {}", snap.display_name));
                }
                Step::ReturnVolume => {
                    let id = need(&target)?;
                    let snap = self.snapshot(&id)?;
                    outcome.value = Some(snap.volume as i64);
                    outcome.messages.push(snap.volume.to_string());
                }
                Step::Pause { ms } => thread::sleep(Duration::from_millis(ms.min(10_000))),
                Step::Quit => outcome.quit = true,
                Step::Restart => outcome.restart = true,
                Step::Run { program } => {
                    if program.is_empty() {
                        anyhow::bail!("missing program");
                    }
                    spawn_program(&program)?;
                    outcome.messages.push(format!("started {program}"));
                }
            }
        }
        if outcome.quit {
            let _ = self.inner.events.send(Event::Quit);
        }
        if outcome.restart {
            let _ = self.inner.events.send(Event::Restart);
        }
        Ok(outcome)
    }

    pub fn icon_png(&self, reference: &str, size: u32, badge: bool) -> anyhow::Result<Vec<u8>> {
        let snap = self.resolve(reference).ok_or_else(|| anyhow!("device '{reference}' not found"))?;
        let file = snap.icon_file.as_deref().map(crate::config::resolve_icon_file);
        let glyph = snap.icon_char.chars().next().unwrap_or('\u{E7F5}');
        crate::icon::render_png(&crate::icon::IconRequest {
            glyph,
            color: crate::icon::parse_color(&snap.icon_color),
            file,
            muted: snap.muted,
            badge: badge.then_some(snap.volume),
            size: size.clamp(16, 256),
        })
        .ok_or_else(|| anyhow!("could not render icon"))
    }

    fn resolve_id(&self, reference: &str) -> Option<String> {
        let reference = reference.trim();
        if reference.is_empty() {
            return None;
        }
        let snaps = self.devices();
        if reference.eq_ignore_ascii_case("out") {
            return snaps.iter().find(|d| d.flow == Flow::Render && d.is_default).map(|d| d.id.clone());
        }
        if reference.eq_ignore_ascii_case("in") {
            return snaps.iter().find(|d| d.flow == Flow::Capture && d.is_default).map(|d| d.id.clone());
        }
        if let Some(d) = snaps.iter().find(|d| d.id.eq_ignore_ascii_case(reference) || d.key.eq_ignore_ascii_case(reference)) {
            return Some(d.id.clone());
        }
        if let Some(d) = snaps.iter().find(|d| d.alias.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(reference))) {
            return Some(d.id.clone());
        }
        if let Some(d) = snaps.iter().find(|d| d.display_name.eq_ignore_ascii_case(reference) || d.name.eq_ignore_ascii_case(reference)) {
            return Some(d.id.clone());
        }
        for flow in [Flow::Render, Flow::Capture] {
            if let Some((id, _)) = choice_labels(&snaps, flow).into_iter().find(|(_, label)| label.eq_ignore_ascii_case(reference)) {
                return Some(id);
            }
        }
        if let Ok(index) = reference.parse::<usize>() {
            if let Some(d) = snaps.iter().find(|d| d.index == index) {
                return Some(d.id.clone());
            }
        }
        None
    }

    fn snapshot(&self, id: &str) -> anyhow::Result<DeviceSnapshot> {
        self.devices().into_iter().find(|d| d.id == id).ok_or_else(|| anyhow!("device '{id}' not found"))
    }

    fn preview_volume(&self, id: &str, value: Value) -> anyhow::Result<u32> {
        let snap = self.snapshot(id)?;
        Ok(value.apply(snap.volume))
    }

    /// Sends a job and, when `wait` is set, waits until the audio thread has applied it.
    fn call(&self, msg: AudioMsg, wait: bool) -> anyhow::Result<()> {
        let (msg, rx) = if wait { with_reply(msg) } else { (msg, None) };
        self.inner.tx.send(msg).map_err(|_| anyhow!("audio thread stopped"))?;
        if let Some(rx) = rx {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(result) => result?,
                Err(mpsc::RecvTimeoutError::Timeout) => anyhow::bail!("audio device timed out"),
                Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("audio device dropped the reply"),
            }
        }
        Ok(())
    }
}

fn need(target: &Option<String>) -> anyhow::Result<String> {
    target.clone().ok_or_else(|| anyhow!("no audio device selected"))
}

fn with_reply(msg: AudioMsg) -> (AudioMsg, Option<mpsc::Receiver<anyhow::Result<()>>>) {
    let (tx, rx) = mpsc::channel();
    let msg = match msg {
        AudioMsg::Rescan { .. } => AudioMsg::Rescan { reply: Some(tx) },
        AudioMsg::SetVolume { id, volume, .. } => AudioMsg::SetVolume { id, volume, reply: Some(tx) },
        AudioMsg::SetMute { id, muted, .. } => AudioMsg::SetMute { id, muted, reply: Some(tx) },
        AudioMsg::SetDefault { id, communications, .. } => AudioMsg::SetDefault { id, communications, reply: Some(tx) },
        other => other,
    };
    (msg, Some(rx))
}

fn spawn_program(program: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd").arg("/C").arg(program).spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("sh").arg("-c").arg(program).spawn()?;
    }
    Ok(())
}

fn snapshots(inner: &Inner) -> Vec<DeviceSnapshot> {
    let config = inner.config.read().clone();
    let mut devices: Vec<LiveDevice> = inner.devices.read().clone();
    devices.sort_by(|a, b| {
        flow_rank(a.flow).cmp(&flow_rank(b.flow)).then(a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())).then(a.id.cmp(&b.id))
    });
    devices
        .iter()
        .enumerate()
        .map(|(i, live)| {
            let configured = config.device(&live.id);
            let alias = configured.and_then(|c| c.alias.clone()).filter(|s| !s.is_empty());
            let (glyph, ch, color, file) = icon_for(live, configured.and_then(|c| c.icon.as_ref()));
            DeviceSnapshot {
                id: live.id.clone(),
                key: key_from_id(&live.id),
                index: i + 1,
                display_name: alias.clone().unwrap_or_else(|| live.name.clone()),
                name: live.name.clone(),
                alias,
                flow: live.flow,
                form_factor: live.form_factor,
                adapter: live.adapter.clone(),
                volume: live.volume,
                muted: live.muted,
                is_default: live.is_default,
                is_default_communications: live.is_default_communications,
                hidden: configured.is_some_and(|c| c.hidden),
                in_cycle: configured.map(|c| c.in_cycle).unwrap_or(true),
                icon_glyph: glyph,
                icon_char: ch,
                icon_color: color,
                icon_file: file,
            }
        })
        .collect()
}

fn flow_rank(flow: Flow) -> u8 {
    match flow {
        Flow::Render => 0,
        Flow::Capture => 1,
    }
}

fn write_volume(inner: &Inner, id: &str, volume: u32, muted: bool) -> bool {
    let mut devices = inner.devices.write();
    let Some(device) = devices.iter_mut().find(|d| d.id == id) else { return false };
    if device.volume == volume && device.muted == muted {
        return false;
    }
    device.volume = volume;
    device.muted = muted;
    true
}

fn cached_mute(inner: &Inner, id: &str) -> bool {
    inner.devices.read().iter().find(|d| d.id == id).is_some_and(|d| d.muted)
}

fn cached_volume(inner: &Inner, id: &str) -> u32 {
    inner.devices.read().iter().find(|d| d.id == id).map(|d| d.volume).unwrap_or(0)
}

fn emit_device(inner: &Inner, id: &str) {
    if let Some(device) = snapshots(inner).into_iter().find(|d| d.id == id) {
        let _ = inner.events.send(Event::DeviceUpdated { device });
    }
}

fn emit_all(inner: &Inner) {
    let _ = inner.events.send(Event::DevicesChanged { devices: snapshots(inner) });
}

fn apply_role(inner: &Inner, flow: Flow, id: &str, communications: bool) -> bool {
    let mut devices = inner.devices.write();
    let mut changed = false;
    for device in devices.iter_mut() {
        if device.flow != flow {
            continue;
        }
        let mine = !id.is_empty() && device.id.eq_ignore_ascii_case(id);
        if communications {
            if device.is_default_communications != mine {
                device.is_default_communications = mine;
                changed = true;
            }
        } else if device.is_default != mine {
            device.is_default = mine;
            changed = true;
        }
    }
    changed
}

fn run_loop(inner: Arc<Inner>, mut driver: Box<dyn Driver>, rx: mpsc::Receiver<AudioMsg>) {
    let mut peaks: HashMap<String, u8> = HashMap::new();
    loop {
        let first = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(msg) => msg,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                poll_peaks(&inner, driver.as_mut(), &mut peaks);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if matches!(first, AudioMsg::Shutdown) {
            break;
        }
        let mut batch = vec![first];
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, AudioMsg::Shutdown) {
                handle_batch(&inner, driver.as_mut(), batch);
                return;
            }
            batch.push(msg);
        }
        handle_batch(&inner, driver.as_mut(), batch);
    }
}

fn handle_batch(inner: &Inner, driver: &mut dyn Driver, batch: Vec<AudioMsg>) {
    let mut last_volume: HashMap<String, usize> = HashMap::new();
    let mut last_mute: HashMap<String, usize> = HashMap::new();
    for (i, msg) in batch.iter().enumerate() {
        match msg {
            AudioMsg::SetVolume { id, .. } => {
                last_volume.insert(id.clone(), i);
            }
            AudioMsg::SetMute { id, .. } => {
                last_mute.insert(id.clone(), i);
            }
            _ => {}
        }
    }
    let mut refresh = false;
    for (i, msg) in batch.into_iter().enumerate() {
        match msg {
            AudioMsg::Shutdown => return,
            AudioMsg::Notice(Notice::Refresh) => refresh = true,
            AudioMsg::Notice(Notice::Volume { id, volume, muted }) => {
                if write_volume(inner, &id, volume, muted) {
                    emit_device(inner, &id);
                }
            }
            AudioMsg::Notice(Notice::Default { flow, id, communications }) => {
                if apply_role(inner, flow, &id, communications) {
                    emit_all(inner);
                }
            }
            AudioMsg::Rescan { reply } => {
                let result = replace_devices(inner, driver);
                if let Some(tx) = reply {
                    let _ = tx.send(result);
                }
            }
            AudioMsg::SetVolume { id, volume, reply } => {
                if last_volume.get(&id) != Some(&i) {
                    if let Some(tx) = reply {
                        let _ = tx.send(Ok(()));
                    }
                    continue;
                }
                let result = driver.set_volume(&id, volume);
                if result.is_ok() {
                    if write_volume(inner, &id, volume, cached_mute(inner, &id)) {
                        emit_device(inner, &id);
                    }
                } else if let Ok((volume, muted)) = driver.read(&id) {
                    if write_volume(inner, &id, volume, muted) {
                        emit_device(inner, &id);
                    }
                }
                if let Some(tx) = reply {
                    let _ = tx.send(result);
                }
            }
            AudioMsg::SetMute { id, muted, reply } => {
                if last_mute.get(&id) != Some(&i) {
                    if let Some(tx) = reply {
                        let _ = tx.send(Ok(()));
                    }
                    continue;
                }
                let result = driver.set_mute(&id, muted);
                if result.is_ok() {
                    if write_volume(inner, &id, cached_volume(inner, &id), muted) {
                        emit_device(inner, &id);
                    }
                } else if let Ok((volume, muted)) = driver.read(&id) {
                    if write_volume(inner, &id, volume, muted) {
                        emit_device(inner, &id);
                    }
                }
                if let Some(tx) = reply {
                    let _ = tx.send(result);
                }
            }
            AudioMsg::SetDefault { id, communications, reply } => {
                let result = driver.set_default(&id, communications);
                if result.is_ok() {
                    // Drop the read guard before apply_role takes the write guard.
                    let flow = inner.devices.read().iter().find(|d| d.id == id).map(|d| d.flow);
                    if let Some(flow) = flow {
                        let _ = apply_role(inner, flow, &id, false);
                        if communications {
                            let _ = apply_role(inner, flow, &id, true);
                        }
                        emit_all(inner);
                    }
                }
                if let Some(tx) = reply {
                    let _ = tx.send(result);
                }
            }
        }
    }
    if refresh {
        if let Err(e) = replace_devices(inner, driver) {
            tracing::warn!("rescan: {e:#}");
        }
    }
}

fn replace_devices(inner: &Inner, driver: &mut dyn Driver) -> anyhow::Result<()> {
    let devices = driver.scan()?;
    *inner.devices.write() = devices;
    emit_all(inner);
    Ok(())
}

fn poll_peaks(inner: &Inner, driver: &mut dyn Driver, last: &mut HashMap<String, u8>) {
    let peaks = driver.peaks();
    if peaks.is_empty() && last.is_empty() {
        return;
    }
    let mut changed = peaks.len() != last.len();
    let levels = peaks
        .into_iter()
        .map(|(id, peak)| {
            let q = (peak.clamp(0.0, 1.0) * 100.0).round() as u8;
            if last.get(&id) != Some(&q) {
                changed = true;
                last.insert(id.clone(), q);
            }
            Level { id, peak: q as f32 / 100.0 }
        })
        .collect::<Vec<_>>();
    last.retain(|id, _| levels.iter().any(|level| &level.id == id));
    if changed {
        let _ = inner.events.send(Event::Levels { levels });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        let engine = Engine::new(Config::default(), None, BackendKind::Mock(crate::backend::MockBackend::demo()));
        engine.rescan().unwrap();
        engine
    }

    #[test]
    fn command_sets_volume_cycles_and_mutes() {
        let engine = engine();
        let out = engine.execute("Headphones v+5 g").unwrap();
        assert_eq!(out.value, Some(25));
        assert_eq!(engine.resolve("headphones").unwrap().volume, 25);

        engine.execute("o next").unwrap();
        assert_eq!(engine.default_of(Flow::Render).unwrap().name, "Headphones");
        engine.execute("o next").unwrap();
        assert_eq!(engine.default_of(Flow::Render).unwrap().name, "Speakers");

        engine.execute("out m t").unwrap();
        assert!(!engine.default_of(Flow::Render).unwrap().muted);
        engine.execute("in v 10").unwrap();
        assert_eq!(engine.default_of(Flow::Capture).unwrap().volume, 10);
    }

    #[test]
    fn hidden_and_out_of_cycle_devices_are_skipped() {
        let engine = engine();
        engine.update_config(|c| c.device_mut("headphones").in_cycle = false).unwrap();
        engine.execute("o next").unwrap();
        assert_eq!(engine.default_of(Flow::Render).unwrap().name, "Speakers");
        engine.update_config(|c| {
            c.device_mut("headphones").in_cycle = true;
            c.device_mut("headphones").hidden = true;
        })
        .unwrap();
        engine.execute("o next").unwrap();
        assert_eq!(engine.default_of(Flow::Render).unwrap().name, "Speakers");
        assert!(engine.resolve("headphones").unwrap().hidden);
    }

    #[test]
    fn patch_makes_a_device_default() {
        let engine = engine();
        let snap = engine.apply_patch("mic", &DevicePatch { volume: Some(ValueSpec(Value::Abs(15))), muted: Some(true), default: Some(true) }).unwrap();
        assert_eq!(snap.volume, 15);
        assert!(snap.muted && snap.is_default);
        assert!(!engine.resolve("speakers").unwrap().is_default || engine.resolve("speakers").unwrap().flow == Flow::Render);
    }
}
