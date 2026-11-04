//! Windows Core Audio. Every COM pointer in here is created and used on the audio thread.

#![allow(non_snake_case)]

use std::sync::mpsc::Sender;

use anyhow::Context;
use windows::core::{implement, interface, Interface, GUID, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl, IAudioMeterInformation,
};
use windows::Win32::Media::Audio::{
    eAll, eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator,
    IMMEndpoint, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
    DEVICE_STATE_ACTIVE, PKEY_AudioEndpoint_FormFactor,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Com::StructuredStorage::{PropVariantToStringAlloc, PropVariantToUInt32, PROPVARIANT};

use crate::model::{Flow, FormFactor, LiveDevice};

use super::{AudioMsg, Driver, Notice};

/// Context passed to our own volume writes so the callback can ignore the echo.
const OUR_CONTEXT: GUID = GUID::from_u128(0xa31c6e40_91b7_4d55_8c3e_6f0b9d2e4a17);
const POLICY_CLSID: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Undocumented policy client. Only `SetDefaultEndpoint` is called; the earlier slots exist so it
/// lands on the right vtable index (IUnknown + 10).
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows::core::IUnknown {
    unsafe fn reserved1(&self) -> windows::core::HRESULT;
    unsafe fn reserved2(&self) -> windows::core::HRESULT;
    unsafe fn reserved3(&self) -> windows::core::HRESULT;
    unsafe fn reserved4(&self) -> windows::core::HRESULT;
    unsafe fn reserved5(&self) -> windows::core::HRESULT;
    unsafe fn reserved6(&self) -> windows::core::HRESULT;
    unsafe fn reserved7(&self) -> windows::core::HRESULT;
    unsafe fn reserved8(&self) -> windows::core::HRESULT;
    unsafe fn reserved9(&self) -> windows::core::HRESULT;
    unsafe fn reserved10(&self) -> windows::core::HRESULT;
    unsafe fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> windows::core::HRESULT;
}

pub struct WinState {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    endpoints: Vec<Endpoint>,
    tx: Sender<AudioMsg>,
}

struct Endpoint {
    id: String,
    volume: IAudioEndpointVolume,
    meter: Option<IAudioMeterInformation>,
    callback: IAudioEndpointVolumeCallback,
}

impl WinState {
    pub fn start(tx: Sender<AudioMsg>) -> anyhow::Result<Self> {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).context("MMDeviceEnumerator")?;
            let client: IMMNotificationClient = Notify { tx: tx.clone() }.into();
            enumerator.RegisterEndpointNotificationCallback(&client).context("endpoint notifications")?;
            let mut state = Self { enumerator, client, endpoints: Vec::new(), tx };
            // Callbacks are live. The first scan is requested by the engine, but registering the
            // volume callbacks happens inside scan, so do it once now as well.
            let _ = state.scan();
            Ok(state)
        }
    }
}

impl Drop for WinState {
    fn drop(&mut self) {
        unsafe {
            let _ = self.enumerator.UnregisterEndpointNotificationCallback(&self.client);
            self.release_endpoints();
        }
    }
}

impl WinState {
    fn release_endpoints(&mut self) {
        for endpoint in self.endpoints.drain(..) {
            unsafe {
                let _ = endpoint.volume.UnregisterControlChangeNotify(&endpoint.callback);
            }
        }
    }

    fn endpoint(&self, id: &str) -> anyhow::Result<&Endpoint> {
        self.endpoints.iter().find(|e| e.id == id).ok_or_else(|| anyhow::anyhow!("device '{id}' not found"))
    }
}

impl Driver for WinState {
    fn scan(&mut self) -> anyhow::Result<Vec<LiveDevice>> {
        self.release_endpoints();
        unsafe {
            let collection = self.enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)?;
            let count = collection.GetCount()?;
            let mut devices = Vec::with_capacity(count as usize);
            for i in 0..count {
                let device = collection.Item(i)?;
                let id = pwstr_string(device.GetId()?);
                if id.is_empty() {
                    continue;
                }
                let (name, adapter, form) = describe(&device);
                let flow = match device.cast::<IMMEndpoint>() {
                    Ok(endpoint) => match endpoint.GetDataFlow() {
                        Ok(flow) if flow == eCapture => Flow::Capture,
                        Ok(_) => Flow::Render,
                        Err(e) => {
                            tracing::warn!("{id}: data flow: {e}");
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!("{id}: endpoint: {e}");
                        continue;
                    }
                };
                let volume: IAudioEndpointVolume = match device.Activate(CLSCTX_ALL, None) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("{name}: volume: {e}");
                        continue;
                    }
                };
                let (level, muted) = read_volume(&volume).unwrap_or((0, false));
                let meter = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None).ok();
                let callback: IAudioEndpointVolumeCallback =
                    VolumeWatch { id: id.clone(), tx: self.tx.clone() }.into();
                if let Err(e) = volume.RegisterControlChangeNotify(&callback) {
                    tracing::warn!("{name}: volume notify: {e}");
                }
                self.endpoints.push(Endpoint { id: id.clone(), volume, meter, callback });
                devices.push(LiveDevice {
                    id,
                    name,
                    adapter,
                    flow,
                    form_factor: form,
                    volume: level,
                    muted,
                    is_default: false,
                    is_default_communications: false,
                });
            }
            mark_default(&self.enumerator, &mut devices, eRender);
            mark_default(&self.enumerator, &mut devices, eCapture);
            Ok(devices)
        }
    }

    fn set_volume(&mut self, id: &str, volume: u32) -> anyhow::Result<()> {
        let endpoint = self.endpoint(id)?;
        unsafe {
            endpoint.volume.SetMasterVolumeLevelScalar(volume.min(100) as f32 / 100.0, &OUR_CONTEXT)?;
        }
        Ok(())
    }

    fn set_mute(&mut self, id: &str, muted: bool) -> anyhow::Result<()> {
        let endpoint = self.endpoint(id)?;
        unsafe { endpoint.volume.SetMute(muted, &OUR_CONTEXT)? }
        Ok(())
    }

    fn set_default(&mut self, id: &str, communications: bool) -> anyhow::Result<()> {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let policy: IPolicyConfig = CoCreateInstance(&POLICY_CLSID, None, CLSCTX_ALL).context("IPolicyConfig")?;
            let name = PCWSTR(wide.as_ptr());
            policy.SetDefaultEndpoint(name, eConsole).ok().context("set console endpoint")?;
            policy.SetDefaultEndpoint(name, eMultimedia).ok().context("set multimedia endpoint")?;
            if communications {
                policy.SetDefaultEndpoint(name, eCommunications).ok().context("set communications endpoint")?;
            }
        }
        Ok(())
    }

    fn read(&mut self, id: &str) -> anyhow::Result<(u32, bool)> {
        let endpoint = self.endpoint(id)?;
        unsafe { read_volume(&endpoint.volume) }
    }

    fn peaks(&mut self) -> Vec<(String, f32)> {
        self.endpoints
            .iter()
            .filter_map(|e| {
                let meter = e.meter.as_ref()?;
                let peak = unsafe { meter.GetPeakValue() }.ok()?;
                Some((e.id.clone(), peak.clamp(0.0, 1.0)))
            })
            .collect()
    }
}

unsafe fn read_volume(volume: &IAudioEndpointVolume) -> anyhow::Result<(u32, bool)> {
    let level = (volume.GetMasterVolumeLevelScalar()? * 100.0).round().clamp(0.0, 100.0) as u32;
    let muted = volume.GetMute()? != false;
    Ok((level, muted))
}

fn mark_default(enumerator: &IMMDeviceEnumerator, devices: &mut [LiveDevice], flow: EDataFlow) {
    unsafe {
        if let Ok(id) = default_id(enumerator, flow, eConsole) {
            if let Some(device) = devices.iter_mut().find(|d| d.id.eq_ignore_ascii_case(&id)) {
                device.is_default = true;
            }
        }
        if let Ok(id) = default_id(enumerator, flow, eCommunications) {
            if let Some(device) = devices.iter_mut().find(|d| d.id.eq_ignore_ascii_case(&id)) {
                device.is_default_communications = true;
            }
        }
    }
}

unsafe fn default_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> anyhow::Result<String> {
    let device = enumerator.GetDefaultAudioEndpoint(flow, role)?;
    Ok(pwstr_string(device.GetId()?))
}

fn describe(device: &IMMDevice) -> (String, String, FormFactor) {
    unsafe {
        let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
            return ("Audio device".into(), String::new(), FormFactor::Unknown);
        };
        let name = prop_string(&store, &PKEY_Device_FriendlyName).unwrap_or_else(|| "Audio device".into());
        let adapter = prop_string(&store, &PKEY_Device_DeviceDesc).unwrap_or_default();
        let form = match store.GetValue(&PKEY_AudioEndpoint_FormFactor) {
            Ok(value) => form_factor(PropVariantToUInt32(&value).unwrap_or(10)),
            Err(_) => FormFactor::Unknown,
        };
        (name, adapter, form)
    }
}

fn prop_string(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore, key: &windows::Win32::Foundation::PROPERTYKEY) -> Option<String> {
    unsafe {
        let value: PROPVARIANT = store.GetValue(key).ok()?;
        let pw = PropVariantToStringAlloc(&value).ok()?;
        let text = pw.to_string().ok();
        CoTaskMemFree(Some(pw.0 as *const _));
        text.filter(|s| !s.is_empty())
    }
}

fn form_factor(value: u32) -> FormFactor {
    match value {
        0 => FormFactor::Network,
        1 => FormFactor::Speakers,
        2 => FormFactor::Line,
        3 => FormFactor::Headphones,
        4 => FormFactor::Microphone,
        5 => FormFactor::Headset,
        6 => FormFactor::Handset,
        8 => FormFactor::Spdif,
        7 | 9 => FormFactor::Digital,
        _ => FormFactor::Unknown,
    }
}

fn pwstr_string(pw: PWSTR) -> String {
    let text = unsafe { pw.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(pw.0 as *const _)) };
    text
}

#[implement(IMMNotificationClient)]
struct Notify {
    tx: Sender<AudioMsg>,
}

impl IMMNotificationClient_Impl for Notify_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: windows::Win32::Media::Audio::DEVICE_STATE) -> windows::core::Result<()> {
        self.poke(Notice::Refresh)
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.poke(Notice::Refresh)
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.poke(Notice::Refresh)
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, id: &PCWSTR) -> windows::core::Result<()> {
        let flow = if flow == eCapture { Flow::Capture } else { Flow::Render };
        let id = unsafe { id.to_string() }.unwrap_or_default();
        self.poke(Notice::Default { flow, id, communications: role == eCommunications })
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &windows::Win32::Foundation::PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

impl Notify_Impl {
    fn poke(&self, notice: Notice) -> windows::core::Result<()> {
        let _ = self.tx.send(AudioMsg::Notice(notice));
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeWatch {
    id: String,
    tx: Sender<AudioMsg>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeWatch_Impl {
    fn OnNotify(&self, notify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if notify.is_null() {
            return Ok(());
        }
        let data = unsafe { &*notify };
        if data.guidEventContext == OUR_CONTEXT {
            return Ok(());
        }
        let volume = (data.fMasterVolume * 100.0).round().clamp(0.0, 100.0) as u32;
        let muted = data.bMuted != false;
        let _ = self.tx.send(AudioMsg::Notice(Notice::Volume { id: self.id.clone(), volume, muted }));
        Ok(())
    }
}
