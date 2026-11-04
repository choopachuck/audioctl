//! Network front-ends for the engine: HTTP/WebSocket API and the MQTT / Home Assistant bridge.

pub mod api;
pub mod mqtt;

use audioctl_core::{Engine, Event};
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Runs the API and MQTT bridge, restarting them whenever their configuration changes.
pub async fn supervise(engine: Engine) {
    let mut events = engine.subscribe();
    let mut running = start(&engine);
    loop {
        match events.recv().await {
            Ok(Event::ConfigChanged) => {
                let cfg = engine.config();
                if (cfg.api.clone(), cfg.mqtt.clone()) != running.config {
                    info!("network configuration changed, restarting services");
                    running.stop();
                    running = start(&engine);
                }
            }
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(_) => return,
        }
    }
}

struct Running {
    config: (audioctl_core::config::Api, audioctl_core::config::Mqtt),
    tasks: Vec<JoinHandle<()>>,
}

impl Running {
    fn stop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

fn start(engine: &Engine) -> Running {
    let cfg = engine.config();
    let mut tasks = Vec::new();
    if cfg.api.enabled {
        let app = api::router(engine.clone(), cfg.api.token.clone());
        let bind = cfg.api.bind.clone();
        tasks.push(tokio::spawn(async move {
            match tokio::net::TcpListener::bind(&bind).await {
                Ok(listener) => {
                    info!("API listening on http://{bind}");
                    if let Err(e) = axum::serve(listener, app).await {
                        error!("API server: {e}");
                    }
                }
                Err(e) => error!("cannot bind API to {bind}: {e}"),
            }
        }));
    }
    if cfg.mqtt.enabled {
        let engine = engine.clone();
        let mqtt = cfg.mqtt.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = mqtt::run(engine, mqtt).await {
                error!("MQTT bridge: {e:#}");
            }
        }));
    }
    Running { config: (cfg.api, cfg.mqtt), tasks }
}
