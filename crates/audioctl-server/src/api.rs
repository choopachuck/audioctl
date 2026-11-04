//! REST + WebSocket API.
//!
//! ```text
//! GET    /api/health
//! GET    /api/devices?flow=render|capture
//! GET    /api/devices/{ref}                 ref = key | id | number | name | alias
//! PATCH  /api/devices/{ref}                 {"volume": "+5", "muted": true, "default": true}
//! GET    /api/devices/{ref}/icon.png?size=72&badge=1
//! GET    /api/default                       {"output": {...}, "input": {...}}
//! PUT    /api/default/output                {"device": "Headphones"} | {"device": "next"}
//! PUT    /api/default/input                 {"device": "prev"}
//! POST   /api/command                       {"command": "out v 30"}
//! POST   /api/rescan
//! GET    /api/ws
//! ```

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::CorsLayer;

use audioctl_core::patch::DevicePatch;
use audioctl_core::{Engine, Event, Flow};

#[derive(Clone)]
struct AppState {
    engine: Engine,
    token: Option<Arc<str>>,
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}

type ApiResult<T> = Result<T, ApiError>;

pub fn router(engine: Engine, token: Option<String>) -> Router {
    let state = AppState { engine, token: token.filter(|t| !t.is_empty()).map(Into::into) };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/devices", get(list_devices))
        .route("/api/devices/{device}", get(get_device).patch(patch_device))
        .route("/api/devices/{device}/icon.png", get(device_icon))
        .route("/api/default", get(get_default))
        .route("/api/default/output", put(put_output))
        .route("/api/default/input", put(put_input))
        .route("/api/command", post(command))
        .route("/api/rescan", post(rescan))
        .route("/api/ws", get(ws_upgrade))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn auth(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let Some(token) = &s.token else {
        return next.run(req).await;
    };
    let header_ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == &**token);
    let query_ok = req.uri().query().into_iter().flat_map(|q| q.split('&')).any(|kv| kv.strip_prefix("token=") == Some(&**token));
    if header_ok || query_ok {
        next.run(req).await
    } else {
        ApiError(StatusCode::UNAUTHORIZED, "missing or invalid token".into()).into_response()
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?.map_err(Into::into)
}

fn missing(reference: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("device '{reference}' not found"))
}

async fn health() -> Json<JsonValue> {
    Json(json!({ "name": "audioctl", "version": env!("CARGO_PKG_VERSION") }))
}

#[derive(Deserialize)]
struct ListQuery {
    flow: Option<String>,
}

async fn list_devices(State(s): State<AppState>, Query(q): Query<ListQuery>) -> impl IntoResponse {
    let mut devices = s.engine.devices();
    if let Some(flow) = q.flow.as_deref() {
        let flow = match flow {
            "render" | "output" => Some(Flow::Render),
            "capture" | "input" => Some(Flow::Capture),
            _ => None,
        };
        if let Some(flow) = flow {
            devices.retain(|d| d.flow == flow);
        }
    }
    Json(devices)
}

async fn get_device(State(s): State<AppState>, Path(device): Path<String>) -> ApiResult<impl IntoResponse> {
    s.engine.resolve(&device).map(Json).ok_or_else(|| missing(&device))
}

async fn patch_device(State(s): State<AppState>, Path(device): Path<String>, Json(patch): Json<DevicePatch>) -> ApiResult<impl IntoResponse> {
    let engine = s.engine.clone();
    Ok(Json(blocking(move || engine.apply_patch(&device, &patch)).await?))
}

#[derive(Deserialize)]
struct IconQuery {
    size: Option<u32>,
    /// `1`, `true` or `yes` draws the volume number. Anything else leaves it off.
    badge: Option<String>,
}

async fn device_icon(State(s): State<AppState>, Path(device): Path<String>, Query(q): Query<IconQuery>) -> ApiResult<Response> {
    let engine = s.engine.clone();
    let size = q.size.unwrap_or(72);
    let badge = matches!(q.badge.as_deref(), Some("1" | "true" | "yes"));
    let bytes = blocking(move || engine.icon_png(&device, size, badge)).await?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-store")], bytes).into_response())
}

async fn get_default(State(s): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "output": s.engine.default_of(Flow::Render),
        "input": s.engine.default_of(Flow::Capture),
    }))
}

#[derive(Deserialize)]
struct DefaultBody {
    device: String,
}

async fn put_output(State(s): State<AppState>, Json(body): Json<DefaultBody>) -> ApiResult<impl IntoResponse> {
    put_default(s, Flow::Render, body.device).await
}

async fn put_input(State(s): State<AppState>, Json(body): Json<DefaultBody>) -> ApiResult<impl IntoResponse> {
    put_default(s, Flow::Capture, body.device).await
}

async fn put_default(s: AppState, flow: Flow, device: String) -> ApiResult<impl IntoResponse> {
    let engine = s.engine.clone();
    Ok(Json(blocking(move || engine.activate(flow, &device)).await?))
}

#[derive(Deserialize)]
struct CommandBody {
    command: String,
}

async fn command(State(s): State<AppState>, Json(body): Json<CommandBody>) -> ApiResult<impl IntoResponse> {
    let engine = s.engine.clone();
    Ok(Json(blocking(move || engine.execute(&body.command)).await?))
}

async fn rescan(State(s): State<AppState>) -> ApiResult<impl IntoResponse> {
    let engine = s.engine.clone();
    Ok(Json(blocking(move || engine.rescan()).await?))
}

async fn ws_upgrade(State(s): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| ws_session(socket, s.engine))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Command { command: String, id: Option<JsonValue> },
    Set {
        device: String,
        id: Option<JsonValue>,
        #[serde(flatten)]
        patch: DevicePatch,
    },
    Rescan { id: Option<JsonValue> },
}

async fn ws_session(socket: WebSocket, engine: Engine) {
    let (mut sink, mut stream) = socket.split();
    let mut events = engine.subscribe();
    let hello = Event::DevicesChanged { devices: engine.devices() };
    if sink.send(Message::Text(serde_json::to_string(&hello).unwrap().into())).await.is_err() {
        return;
    }
    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::channel::<JsonValue>(32);
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(ev) => {
                    if sink.send(Message::Text(serde_json::to_string(&ev).unwrap().into())).await.is_err() { break; }
                }
                Err(RecvError::Lagged(_)) => {
                    let ev = Event::DevicesChanged { devices: engine.devices() };
                    if sink.send(Message::Text(serde_json::to_string(&ev).unwrap().into())).await.is_err() { break; }
                }
                Err(RecvError::Closed) => break,
            },
            Some(reply) = reply_rx.recv() => {
                if sink.send(Message::Text(reply.to_string().into())).await.is_err() { break; }
            }
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let engine = engine.clone();
                    let tx = reply_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let _ = tx.blocking_send(handle_client(&engine, &text));
                    });
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

fn handle_client(engine: &Engine, text: &str) -> JsonValue {
    let msg: ClientMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => return json!({ "type": "error", "error": e.to_string() }),
    };
    let (id, result) = match msg {
        ClientMsg::Command { command, id } => (id, engine.execute(&command).map(|o| serde_json::to_value(o).unwrap())),
        ClientMsg::Set { device, id, patch } => (id, engine.apply_patch(&device, &patch).map(|d| serde_json::to_value(d).unwrap())),
        ClientMsg::Rescan { id } => (id, engine.rescan().map(|d| json!(d))),
    };
    match result {
        Ok(v) => json!({ "type": "result", "id": id, "ok": true, "result": v }),
        Err(e) => json!({ "type": "result", "id": id, "ok": false, "error": format!("{e:#}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use audioctl_core::backend::{BackendKind, MockBackend};
    use audioctl_core::Config;
    use tower::ServiceExt;

    fn engine() -> Engine {
        let engine = Engine::new(Config::default(), None, BackendKind::Mock(MockBackend::demo()));
        engine.rescan().unwrap();
        engine
    }

    async fn call(app: &Router, method: &str, uri: &str, body: Option<JsonValue>, token: Option<&str>) -> (StatusCode, JsonValue) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(JsonValue::Null))
    }

    #[tokio::test]
    async fn list_patch_and_command() {
        let app = router(engine(), None);
        let (st, list) = call(&app, "GET", "/api/devices", None, None).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(list.as_array().unwrap().len(), 3);

        let (st, device) = call(&app, "PATCH", "/api/devices/headphones", Some(json!({"volume": "+5", "muted": true})), None).await;
        assert_eq!(st, StatusCode::OK, "{device}");
        assert_eq!(device["volume"], 25);
        assert_eq!(device["muted"], true);

        let (st, out) = call(&app, "POST", "/api/command", Some(json!({"command": "headphones v 10 g"})), None).await;
        assert_eq!(st, StatusCode::OK, "{out}");
        assert_eq!(out["value"], 10);

        let (st, defaults) = call(&app, "PUT", "/api/default/output", Some(json!({"device": "next"})), None).await;
        assert_eq!(st, StatusCode::OK, "{defaults}");
        assert_eq!(defaults["name"], "Headphones");

        let (st, _) = call(&app, "GET", "/api/devices/nope", None, None).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn token_is_enforced() {
        let app = router(engine(), Some("s3cret".into()));
        assert_eq!(call(&app, "GET", "/api/devices", None, None).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(call(&app, "GET", "/api/devices", None, Some("bad")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(call(&app, "GET", "/api/devices", None, Some("s3cret")).await.0, StatusCode::OK);
        assert_eq!(call(&app, "GET", "/api/devices?token=s3cret", None, None).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn icon_is_a_png() {
        let app = router(engine(), None);
        let req = Request::builder().uri("/api/devices/speakers/icon.png?size=32&badge=1").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }
}
