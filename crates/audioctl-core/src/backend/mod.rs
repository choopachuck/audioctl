//! Audio endpoints. COM objects stay on the audio thread; this module only describes the jobs
//! that thread runs and the notices Windows callbacks post back.

mod mock;
#[cfg(windows)]
mod windows;

use std::sync::mpsc::Sender;

pub use mock::MockBackend;

use crate::model::{Flow, LiveDevice};

pub enum BackendKind {
    Mock(MockBackend),
    #[cfg(windows)]
    System,
}

pub fn default_kind() -> BackendKind {
    #[cfg(windows)]
    {
        BackendKind::System
    }
    #[cfg(not(windows))]
    {
        BackendKind::Mock(MockBackend::demo())
    }
}

pub(crate) enum AudioMsg {
    Rescan { reply: Option<Sender<anyhow::Result<()>>> },
    SetVolume { id: String, volume: u32, reply: Option<Sender<anyhow::Result<()>>> },
    SetMute { id: String, muted: bool, reply: Option<Sender<anyhow::Result<()>>> },
    SetDefault { id: String, communications: bool, reply: Option<Sender<anyhow::Result<()>>> },
    Notice(Notice),
    #[allow(dead_code)]
    Shutdown,
}

#[derive(Debug, Clone)]
pub(crate) enum Notice {
    Refresh,
    Volume { id: String, volume: u32, muted: bool },
    /// `communications` selects the communications role. Otherwise this is the console role,
    /// which is the default device the rest of the app follows.
    Default { flow: Flow, id: String, communications: bool },
}

pub(crate) trait Driver {
    fn scan(&mut self) -> anyhow::Result<Vec<LiveDevice>>;
    fn set_volume(&mut self, id: &str, volume: u32) -> anyhow::Result<()>;
    fn set_mute(&mut self, id: &str, muted: bool) -> anyhow::Result<()>;
    fn set_default(&mut self, id: &str, communications: bool) -> anyhow::Result<()>;
    fn read(&mut self, id: &str) -> anyhow::Result<(u32, bool)>;
    fn peaks(&mut self) -> Vec<(String, f32)>;
}

pub(crate) fn open(kind: BackendKind, tx: Sender<AudioMsg>) -> Box<dyn Driver> {
    match kind {
        BackendKind::Mock(backend) => Box::new(backend),
        #[cfg(windows)]
        BackendKind::System => match windows::WinState::start(tx) {
            Ok(state) => Box::new(state),
            Err(e) => {
                tracing::error!("audio backend: {e:#}");
                Box::new(Failed { error: format!("{e:#}") })
            }
        },
    }
}

struct Failed {
    error: String,
}

impl Driver for Failed {
    fn scan(&mut self) -> anyhow::Result<Vec<LiveDevice>> {
        anyhow::bail!("{}", self.error)
    }
    fn set_volume(&mut self, _: &str, _: u32) -> anyhow::Result<()> {
        anyhow::bail!("{}", self.error)
    }
    fn set_mute(&mut self, _: &str, _: bool) -> anyhow::Result<()> {
        anyhow::bail!("{}", self.error)
    }
    fn set_default(&mut self, _: &str, _: bool) -> anyhow::Result<()> {
        anyhow::bail!("{}", self.error)
    }
    fn read(&mut self, _: &str) -> anyhow::Result<(u32, bool)> {
        anyhow::bail!("{}", self.error)
    }
    fn peaks(&mut self) -> Vec<(String, f32)> {
        Vec::new()
    }
}
