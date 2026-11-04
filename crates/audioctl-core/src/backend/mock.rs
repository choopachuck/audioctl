use anyhow::bail;

use crate::model::{Flow, FormFactor, LiveDevice};

use super::Driver;

#[derive(Clone)]
pub struct MockBackend {
    devices: Vec<LiveDevice>,
}

impl MockBackend {
    pub fn new(devices: Vec<LiveDevice>) -> Self {
        Self { devices }
    }

    pub fn demo() -> Self {
        Self::new(vec![
            live("speakers", "Speakers", Flow::Render, FormFactor::Speakers, 50, true),
            live("headphones", "Headphones", Flow::Render, FormFactor::Headphones, 20, false),
            live("mic", "Mic", Flow::Capture, FormFactor::Microphone, 80, true),
        ])
    }

    fn get_mut(&mut self, id: &str) -> anyhow::Result<&mut LiveDevice> {
        self.devices.iter_mut().find(|d| d.id == id).ok_or_else(|| anyhow::anyhow!("device '{id}' not found"))
    }
}

fn live(id: &str, name: &str, flow: Flow, form: FormFactor, volume: u32, default: bool) -> LiveDevice {
    LiveDevice {
        id: id.into(),
        name: name.into(),
        adapter: "Mock".into(),
        flow,
        form_factor: form,
        volume,
        muted: false,
        is_default: default,
        is_default_communications: default,
    }
}

impl Driver for MockBackend {
    fn scan(&mut self) -> anyhow::Result<Vec<LiveDevice>> {
        Ok(self.devices.clone())
    }

    fn set_volume(&mut self, id: &str, volume: u32) -> anyhow::Result<()> {
        self.get_mut(id)?.volume = volume.min(100);
        Ok(())
    }

    fn set_mute(&mut self, id: &str, muted: bool) -> anyhow::Result<()> {
        self.get_mut(id)?.muted = muted;
        Ok(())
    }

    fn set_default(&mut self, id: &str, communications: bool) -> anyhow::Result<()> {
        let flow = self.devices.iter().find(|d| d.id == id).map(|d| d.flow).ok_or_else(|| anyhow::anyhow!("device '{id}' not found"))?;
        let mut found = false;
        for device in &mut self.devices {
            if device.flow != flow {
                continue;
            }
            let mine = device.id == id;
            found |= mine;
            device.is_default = mine;
            if communications {
                device.is_default_communications = mine;
            }
        }
        if !found {
            bail!("device '{id}' not found");
        }
        Ok(())
    }

    fn read(&mut self, id: &str) -> anyhow::Result<(u32, bool)> {
        let device = self.get_mut(id)?;
        Ok((device.volume, device.muted))
    }

    fn peaks(&mut self) -> Vec<(String, f32)> {
        self.devices.iter().map(|d| (d.id.clone(), 0.0)).collect()
    }
}
