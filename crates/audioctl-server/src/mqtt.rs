//! MQTT bridge with Home Assistant discovery and the HACS integration contract.
//!
//! Topics (`<base>` = `<base_topic>/<node_id>`):
//! ```text
//! <base>/status                 online / offline (LWT)
//! <base>/info                   hub description, including output and input lists
//! <base>/output/state           current output label
//! <base>/output/set             device name, key, `next` or `prev`
//! <base>/input/state
//! <base>/input/set
//! <base>/<key>/info             device description; empty payload removes it
//! <base>/<key>/state            {"volume","muted","default",...}
//! <base>/<key>/volume/set
//! <base>/<key>/mute/set         ON / OFF
//! <base>/<key>/default/set
//! <base>/command
//! <base>/rescan
//! ```

use std::collections::HashSet;
use std::time::Duration;

use rumqttc::{
    AsyncClient, Client, ConnectReturnCode, ConnectionError, Event as MqttEvent, LastWill, MqttOptions, Packet, QoS,
    RecvTimeoutError,
};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

use audioctl_core::command::Value as Volume;
use audioctl_core::config::Mqtt as MqttConfig;
use audioctl_core::model::{choice_labels, key_from_id, DeviceSnapshot, Flow};
use audioctl_core::{Engine, Event};

pub fn node_id(cfg: &MqttConfig) -> String {
    let raw = cfg.node_id.clone().filter(|s| !s.is_empty()).or_else(|| std::env::var("COMPUTERNAME").ok()).unwrap_or_else(|| "pc".into());
    key_from_id(&raw)
}

struct Bridge {
    engine: Engine,
    client: AsyncClient,
    cfg: MqttConfig,
    node: String,
    base: String,
    published: HashSet<String>,
    device_keys: HashSet<String>,
}

pub async fn run(engine: Engine, cfg: MqttConfig) -> anyhow::Result<()> {
    let node = node_id(&cfg);
    let base = format!("{}/{}", cfg.base_topic.trim_end_matches('/'), node);
    let broker = broker_endpoint(&cfg.host, cfg.port);
    let mut opts = MqttOptions::new(format!("audioctl-{node}"), broker.host.clone(), broker.port);
    if broker.tls {
        opts.set_transport(rumqttc::Transport::tls_with_default_config());
    }
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_last_will(LastWill::new(format!("{base}/status"), "offline", QoS::AtLeastOnce, true));
    if let Some(user) = cfg.username.clone().filter(|u| !u.is_empty()) {
        opts.set_credentials(user, cfg.password.clone().unwrap_or_default());
    }
    let (client, mut eventloop) = AsyncClient::new(opts, 128);
    let mut bridge = Bridge { engine: engine.clone(), client, cfg, node, base, published: HashSet::new(), device_keys: HashSet::new() };
    let mut events = engine.subscribe();

    loop {
        tokio::select! {
            ev = eventloop.poll() => match ev {
                Ok(MqttEvent::Incoming(Packet::ConnAck(_))) => {
                    let broker = broker_endpoint(&bridge.cfg.host, bridge.cfg.port);
                    info!("mqtt connected to {}:{}", broker.host, broker.port);
                    bridge.on_connect().await;
                }
                Ok(MqttEvent::Incoming(Packet::Publish(p))) => {
                    let payload = String::from_utf8_lossy(&p.payload).to_string();
                    bridge.on_message(&p.topic, &payload).await;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("mqtt: {e}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
            ev = events.recv() => match ev {
                Ok(Event::DeviceUpdated { device }) => bridge.publish_device(&device).await,
                Ok(Event::DevicesChanged { devices }) => bridge.publish_all(&devices).await,
                Ok(Event::ConfigChanged) => bridge.publish_all(&engine.devices()).await,
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => bridge.publish_all(&engine.devices()).await,
                Err(RecvError::Closed) => return Ok(()),
            },
        }
    }
}

impl Bridge {
    async fn publish(&self, topic: String, payload: impl Into<Vec<u8>>, retain: bool) {
        if let Err(e) = self.client.publish(topic, QoS::AtLeastOnce, retain, payload).await {
            warn!("mqtt publish: {e}");
        }
    }

    async fn on_connect(&mut self) {
        let b = &self.base;
        for topic in [
            format!("{b}/output/set"),
            format!("{b}/input/set"),
            format!("{b}/command"),
            format!("{b}/rescan"),
            format!("{b}/+/volume/set"),
            format!("{b}/+/mute/set"),
            format!("{b}/+/default/set"),
        ] {
            let _ = self.client.subscribe(topic, QoS::AtLeastOnce).await;
        }
        self.publish(format!("{}/status", self.base), "online", true).await;
        self.publish_all(&self.engine.devices()).await;
    }

    fn hub_device(&self) -> Value {
        json!({
            "identifiers": [format!("audioctl_{}", self.node)],
            "name": format!("audioctl ({})", self.node),
            "manufacturer": "audioctl",
            "sw_version": env!("CARGO_PKG_VERSION"),
        })
    }

    fn device_registry(&self, device: &DeviceSnapshot) -> Value {
        json!({
            "identifiers": [format!("audioctl_{}_{}", self.node, device.key)],
            "name": device.display_name,
            "manufacturer": device.adapter,
            "model": device.form_factor,
            "via_device": format!("audioctl_{}", self.node),
        })
    }

    async fn publish_all(&mut self, devices: &[DeviceSnapshot]) {
        let visible: Vec<&DeviceSnapshot> = devices.iter().filter(|d| !d.hidden).collect();
        self.publish(format!("{}/info", self.base), hub_info(&self.node, &visible).to_string(), true).await;
        self.publish(format!("{}/output/state", self.base), choice(&visible, Flow::Render), true).await;
        self.publish(format!("{}/input/state", self.base), choice(&visible, Flow::Capture), true).await;

        let mut current = HashSet::new();
        for (topic, payload) in self.hub_discovery(&visible) {
            current.insert(topic.clone());
            self.publish(topic, discovery_payload(self.cfg.discovery, &payload), true).await;
        }
        let mut keys = HashSet::new();
        for device in &visible {
            keys.insert(device.key.clone());
            for (topic, payload) in self.device_discovery(device) {
                current.insert(topic.clone());
                self.publish(topic, discovery_payload(self.cfg.discovery, &payload), true).await;
            }
            self.publish_device(device).await;
        }
        for stale in self.published.difference(&current).cloned().collect::<Vec<_>>() {
            self.publish(stale, Vec::new(), true).await;
        }
        for topic in disappeared(&self.base, &self.device_keys, &keys) {
            self.publish(topic, Vec::new(), true).await;
        }
        self.published = current;
        self.device_keys = keys;
    }

    async fn publish_device(&self, device: &DeviceSnapshot) {
        if device.hidden {
            return;
        }
        let base = format!("{}/{}", self.base, device.key);
        self.publish(format!("{base}/info"), device_info(device).to_string(), true).await;
        self.publish(format!("{base}/state"), device_state(device).to_string(), true).await;
    }

    fn hub_discovery(&self, devices: &[&DeviceSnapshot]) -> Vec<(String, Value)> {
        let p = &self.cfg.discovery_prefix;
        let n = &self.node;
        let b = &self.base;
        let avail = json!([{ "topic": format!("{b}/status") }]);
        let device = self.hub_device();
        let select = |name: &str, kind: &str, icon: &str, flow: Flow| {
            (
                format!("{p}/select/{n}/{kind}/config"),
                json!({
                    "name": name,
                    "unique_id": format!("audioctl_{n}_{kind}"),
                    "command_topic": format!("{b}/{kind}/set"),
                    "state_topic": format!("{b}/{kind}/state"),
                    "options": choice_labels_of(devices, flow),
                    "icon": icon,
                    "availability": avail,
                    "device": device,
                }),
            )
        };
        vec![
            select("Output device", "output", "mdi:speaker", Flow::Render),
            select("Input device", "input", "mdi:microphone", Flow::Capture),
            (
                format!("{p}/text/{n}/command/config"),
                json!({
                    "name": "Command", "unique_id": format!("audioctl_{n}_command"),
                    "command_topic": format!("{b}/command"), "icon": "mdi:console-line",
                    "availability": avail, "device": device,
                }),
            ),
            (
                format!("{p}/button/{n}/rescan/config"),
                json!({
                    "name": "Rescan", "unique_id": format!("audioctl_{n}_rescan"),
                    "command_topic": format!("{b}/rescan"), "icon": "mdi:refresh",
                    "availability": avail, "device": device,
                }),
            ),
        ]
    }

    fn device_discovery(&self, device: &DeviceSnapshot) -> Vec<(String, Value)> {
        let p = &self.cfg.discovery_prefix;
        let n = &self.node;
        let k = &device.key;
        let b = format!("{}/{}", self.base, k);
        let avail = json!([{ "topic": format!("{}/status", self.base) }]);
        let registry = self.device_registry(device);
        vec![
            (
                format!("{p}/number/{n}/{k}_volume/config"),
                json!({
                    "name": "Volume", "unique_id": format!("audioctl_{n}_{k}_volume"),
                    "state_topic": format!("{b}/state"), "value_template": "{{ value_json.volume }}",
                    "command_topic": format!("{b}/volume/set"),
                    "min": 0, "max": 100, "step": 1, "mode": "slider",
                    "icon": "mdi:volume-high", "availability": avail, "device": registry,
                }),
            ),
            (
                format!("{p}/switch/{n}/{k}_mute/config"),
                json!({
                    "name": "Mute", "unique_id": format!("audioctl_{n}_{k}_mute"),
                    "state_topic": format!("{b}/state"),
                    "value_template": "{{ 'ON' if value_json.muted else 'OFF' }}",
                    "command_topic": format!("{b}/mute/set"),
                    "payload_on": "ON", "payload_off": "OFF", "state_on": "ON", "state_off": "OFF",
                    "icon": "mdi:volume-mute", "availability": avail, "device": registry,
                }),
            ),
            (
                format!("{p}/button/{n}/{k}_default/config"),
                json!({
                    "name": "Make default", "unique_id": format!("audioctl_{n}_{k}_default"),
                    "command_topic": format!("{b}/default/set"), "icon": "mdi:check",
                    "availability": avail, "device": registry,
                }),
            ),
            (
                format!("{p}/binary_sensor/{n}/{k}_default/config"),
                json!({
                    "name": "Default", "unique_id": format!("audioctl_{n}_{k}_is_default"),
                    "state_topic": format!("{b}/state"),
                    "value_template": "{{ 'ON' if value_json.default else 'OFF' }}",
                    "payload_on": "ON", "payload_off": "OFF",
                    "icon": "mdi:star", "entity_category": "diagnostic",
                    "availability": avail, "device": registry,
                }),
            ),
        ]
    }

    async fn on_message(&self, topic: &str, payload: &str) {
        let Some(rest) = topic.strip_prefix(&format!("{}/", self.base)) else { return };
        let engine = self.engine.clone();
        let payload = payload.trim().to_string();
        match rest {
            "command" => spawn(move || Ok(engine.execute(&payload).map(|_| ())?)),
            "rescan" => spawn(move || Ok(engine.rescan().map(|_| ())?)),
            "output/set" => spawn(move || Ok(engine.activate(Flow::Render, &payload).map(|_| ())?)),
            "input/set" => spawn(move || Ok(engine.activate(Flow::Capture, &payload).map(|_| ())?)),
            other => {
                let mut parts = other.split('/');
                let key = parts.next().unwrap_or("").to_string();
                let field = parts.next().unwrap_or("");
                if parts.next() != Some("set") {
                    return;
                }
                match field {
                    "volume" => {
                        let Some(value) = parse_volume(&payload) else { return };
                        spawn(move || Ok(engine.set_volume(&key, value).map(|_| ())?));
                    }
                    "mute" => {
                        let muted = matches!(payload.to_ascii_uppercase().as_str(), "ON" | "1" | "TRUE");
                        spawn(move || Ok(engine.set_mute(&key, muted).map(|_| ())?));
                    }
                    "default" => spawn(move || Ok(engine.set_default(&key).map(|_| ())?)),
                    _ => {}
                }
            }
        }
    }
}

fn parse_volume(payload: &str) -> Option<Volume> {
    let s = payload.trim();
    if let Some(rest) = s.strip_prefix('+') {
        rest.parse().ok().map(Volume::Rel)
    } else if let Some(rest) = s.strip_prefix('-') {
        rest.parse::<i32>().ok().map(|n| Volume::Rel(-n))
    } else {
        s.parse::<f64>().ok().map(|n| Volume::Abs(n.round().clamp(0.0, 100.0) as u32))
    }
}

fn visible_owned(devices: &[&DeviceSnapshot]) -> Vec<DeviceSnapshot> {
    devices.iter().map(|d| (*d).clone()).collect()
}

fn choice(devices: &[&DeviceSnapshot], flow: Flow) -> String {
    let owned = visible_owned(devices);
    let labels = choice_labels(&owned, flow);
    owned.iter().find(|d| d.flow == flow && d.is_default).and_then(|d| labels.iter().find(|(id, _)| id == &d.id).map(|(_, label)| label.clone())).unwrap_or_default()
}

fn choice_labels_of(devices: &[&DeviceSnapshot], flow: Flow) -> Vec<String> {
    choice_labels(&visible_owned(devices), flow).into_iter().map(|(_, label)| label).collect()
}

fn hub_info(node: &str, devices: &[&DeviceSnapshot]) -> Value {
    let owned = visible_owned(devices);
    json!({
        "node_id": node,
        "name": format!("audioctl ({node})"),
        "sw_version": env!("CARGO_PKG_VERSION"),
        "output": choice(devices, Flow::Render),
        "input": choice(devices, Flow::Capture),
        "outputs": choice_labels_of(devices, Flow::Render),
        "inputs": choice_labels_of(devices, Flow::Capture),
        "devices": owned.iter().map(|d| d.key.clone()).collect::<Vec<_>>(),
    })
}

pub(crate) fn device_info(device: &DeviceSnapshot) -> Value {
    json!({
        "key": device.key,
        "id": device.id,
        "name": device.display_name,
        "flow": device.flow,
        "form_factor": device.form_factor,
        "adapter": device.adapter,
        "volume": device.volume,
        "muted": device.muted,
        "default": device.is_default,
    })
}

fn device_state(device: &DeviceSnapshot) -> Value {
    json!({
        "volume": device.volume,
        "muted": device.muted,
        "default": device.is_default,
        "default_communications": device.is_default_communications,
        "name": device.display_name,
        "flow": device.flow,
    })
}

fn discovery_payload(enabled: bool, body: &Value) -> Vec<u8> {
    if enabled { body.to_string().into_bytes() } else { Vec::new() }
}

fn disappeared(base: &str, previous: &HashSet<String>, current: &HashSet<String>) -> Vec<String> {
    let mut topics: Vec<String> = previous.difference(current).flat_map(|key| [format!("{base}/{key}/state"), format!("{base}/{key}/info")]).collect();
    topics.sort();
    topics
}

struct BrokerEndpoint {
    host: String,
    port: u16,
    tls: bool,
}

fn broker_endpoint(raw: &str, default_port: u16) -> BrokerEndpoint {
    let raw = raw.trim();
    let (rest, tls) = strip_broker_scheme(raw);
    let rest = rest.trim().trim_end_matches('/');
    let rest = rest.rsplit_once('@').map(|(_, host)| host).unwrap_or(rest).trim();
    let (host, port) = split_host_port(rest);
    BrokerEndpoint { host, port: port.unwrap_or(default_port), tls }
}

fn strip_broker_scheme(raw: &str) -> (&str, bool) {
    for (prefix, tls) in [("mqtts://", true), ("ssl://", true), ("mqtt://", false), ("tcp://", false)] {
        if raw.len() >= prefix.len() && raw[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return (&raw[prefix.len()..], tls);
        }
    }
    (raw, false)
}

fn split_host_port(authority: &str) -> (String, Option<u16>) {
    if let Some(rest) = authority.strip_prefix('[') {
        if let Some((host, port)) = rest.rsplit_once("]:") {
            return (host.to_string(), port.parse().ok());
        }
        if let Some(host) = rest.strip_suffix(']') {
            return (host.to_string(), None);
        }
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        if !host.is_empty() && !host.contains(':') {
            if let Ok(port) = port.parse::<u16>() {
                return (host.to_string(), Some(port));
            }
        }
    }
    (authority.to_string(), None)
}

/// Opens a short-lived connection with the given settings and closes it again.
pub fn test_connection(cfg: &MqttConfig) -> anyhow::Result<String> {
    let broker = broker_endpoint(&cfg.host, cfg.port);
    if broker.host.is_empty() {
        anyhow::bail!("Broker address is empty");
    }
    let target = format!("{}:{}", broker.host, broker.port);
    let mut opts = MqttOptions::new(format!("audioctl-test-{}", std::process::id()), broker.host.clone(), broker.port);
    opts.set_keep_alive(Duration::from_secs(10));
    if broker.tls {
        opts.set_transport(rumqttc::Transport::tls_with_default_config());
    }
    if let Some(user) = cfg.username.clone().filter(|u| !u.is_empty()) {
        opts.set_credentials(user, cfg.password.clone().unwrap_or_default());
    }
    let (client, mut connection) = Client::new(opts, 4);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    let outcome = loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break Err(anyhow::anyhow!("Timed out connecting to {target}"));
        }
        match connection.recv_timeout(left) {
            Ok(Ok(MqttEvent::Incoming(Packet::ConnAck(ack)))) => {
                break if ack.code == ConnectReturnCode::Success { Ok(()) } else { Err(anyhow::anyhow!("{}", refuse_text(ack.code, &target))) };
            }
            Ok(Ok(_)) => {}
            Ok(Err(err)) => break Err(anyhow::anyhow!("{}", explain_connection(err, &target))),
            Err(RecvTimeoutError::Timeout) => break Err(anyhow::anyhow!("Timed out connecting to {target}")),
            Err(RecvTimeoutError::Disconnected) => break Err(anyhow::anyhow!("Connection to {target} closed before the broker answered")),
        }
    };
    let _ = client.disconnect();
    let _ = connection.recv_timeout(Duration::from_millis(500));
    outcome?;
    if broker.tls { Ok(format!("Connected to {target} (TLS)")) } else { Ok(format!("Connected to {target}")) }
}

fn refuse_text(code: ConnectReturnCode, target: &str) -> String {
    let why = match code {
        ConnectReturnCode::Success => "connected",
        ConnectReturnCode::BadUserNamePassword | ConnectReturnCode::NotAuthorized => "username or password was rejected",
        ConnectReturnCode::ServiceUnavailable => "the broker is unavailable",
        ConnectReturnCode::BadClientId => "the broker rejected the client id",
        ConnectReturnCode::RefusedProtocolVersion => "the broker refused the MQTT protocol version",
    };
    format!("Broker {target} refused the connection: {why}")
}

fn explain_connection(err: ConnectionError, target: &str) -> String {
    match err {
        ConnectionError::ConnectionRefused(code) => refuse_text(code, target),
        ConnectionError::NetworkTimeout => format!("Timed out connecting to {target}"),
        ConnectionError::Tls(err) => format!("TLS handshake with {target} failed: {err}"),
        ConnectionError::Io(err) => format!("Cannot reach {target}: {err}"),
        other => format!("Cannot connect to {target}: {other}"),
    }
}

fn spawn(f: impl FnOnce() -> anyhow::Result<()> + Send + 'static) {
    tokio::task::spawn_blocking(move || {
        if let Err(e) = f() {
            warn!("mqtt command failed: {e:#}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use audioctl_core::model::FormFactor;

    fn snap() -> DeviceSnapshot {
        DeviceSnapshot {
            id: "speakers".into(),
            key: "speakers".into(),
            index: 1,
            name: "Speakers".into(),
            alias: None,
            display_name: "Speakers".into(),
            flow: Flow::Render,
            form_factor: FormFactor::Speakers,
            adapter: "Realtek".into(),
            volume: 40,
            muted: false,
            is_default: true,
            is_default_communications: true,
            hidden: false,
            in_cycle: true,
            icon_glyph: "speakers".into(),
            icon_char: "\u{E7F5}".into(),
            icon_color: "#64D2FF".into(),
            icon_file: None,
        }
    }

    #[test]
    fn empty_broker_is_rejected() {
        let mut cfg = MqttConfig::default();
        cfg.host = "   ".into();
        let err = test_connection(&cfg).unwrap_err().to_string();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn broker_accepts_plain_host_and_mqtt_url() {
        let plain = broker_endpoint(" 192.168.2.95 ", 1883);
        assert_eq!(plain.host, "192.168.2.95");
        assert_eq!(plain.port, 1883);
        assert!(!plain.tls);
        let url = broker_endpoint("mqtts://homeassistant.local:8883/", 1883);
        assert_eq!(url.host, "homeassistant.local");
        assert_eq!(url.port, 8883);
        assert!(url.tls);
    }

    #[test]
    fn device_info_has_a_plain_volume() {
        let info = device_info(&snap());
        assert_eq!(info["volume"], 40);
        assert_eq!(info["key"], "speakers");
        assert_eq!(info["default"], true);
        assert_eq!(info["flow"], "render");
    }

    #[test]
    fn discovery_off_clears_the_retained_config() {
        assert!(discovery_payload(false, &json!({"name": "Volume"})).is_empty());
        assert!(!discovery_payload(true, &json!({"name": "Volume"})).is_empty());
    }

    #[test]
    fn disappeared_devices_clear_state_and_info() {
        let previous = HashSet::from(["speakers".into(), "headphones".into()]);
        let current = HashSet::from(["speakers".into()]);
        assert_eq!(disappeared("audioctl/pc", &previous, &current), vec!["audioctl/pc/headphones/info", "audioctl/pc/headphones/state"]);
    }

    #[test]
    fn hub_info_lists_the_current_output() {
        let device = snap();
        let info = hub_info("pc", &[&device]);
        assert_eq!(info["output"], "Speakers");
        assert_eq!(info["outputs"][0], "Speakers");
        assert_eq!(info["node_id"], "pc");
    }
}
