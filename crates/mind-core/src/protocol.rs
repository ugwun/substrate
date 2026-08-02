use crate::PROTOCOL_VERSION;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const JSONRPC_VERSION: &str = "2.0";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

pub mod method {
    pub const PING: &str = "system.ping";
    pub const GET_STATUS: &str = "mind.get_status";
    pub const SUBSCRIBE_STATUS: &str = "mind.subscribe_status";
    pub const UNSUBSCRIBE_STATUS: &str = "mind.unsubscribe_status";
    pub const SHUTDOWN: &str = "daemon.shutdown";
    pub const TRIGGER_DIAGNOSTIC_PULSE: &str = "mind.trigger_diagnostic_pulse";
    pub const STATUS_CHANGED: &str = "mind.status_changed";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Ready,
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct StatusSnapshot {
    pub protocol_version: u32,
    pub daemon_version: String,
    pub pid: u32,
    pub health: HealthState,
    pub started_at: DateTime<Utc>,
    pub uptime_ms: u64,
    pub tick: u64,
    pub tick_rate_hz: u32,
    #[serde(default)]
    pub cpu_usage_percent: Option<f32>,
    pub connected_clients: u32,
    pub mcp_url: String,
    pub last_diagnostic: Option<DiagnosticPulseResult>,
    pub recent_events: Vec<EngineEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineEventKind {
    DaemonStarted,
    DiagnosticPulse,
    ShutdownRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EngineEvent {
    pub event_id: u64,
    pub occurred_at: DateTime<Utc>,
    pub tick: u64,
    pub kind: EngineEventKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiagnosticPulseResult {
    pub pulse_id: u64,
    pub emitted_at: DateTime<Utc>,
    pub tick: u64,
    pub health: HealthState,
    pub components_checked: u32,
    pub paths_traversed: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PingResult {
    pub protocol_version: u32,
    pub daemon_version: String,
}

impl PingResult {
    pub fn current() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeResult {
    pub subscription_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsubscribeParams {
    pub subscription_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusNotification {
    pub subscription_id: Uuid,
    pub status: StatusSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcIncoming {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
}

pub fn request(id: u64, method: &str, params: Option<Value>) -> Value {
    let mut value = serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "method": method,
    });
    if let Some(params) = params {
        value["params"] = params;
    }
    value
}

pub fn success<T: Serialize>(id: Value, result: T) -> Value {
    serde_json::json!({"jsonrpc": JSONRPC_VERSION, "id": id, "result": result})
}

pub fn failure(id: Value, code: i32, message: impl Into<String>) -> Value {
    serde_json::json!({
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {"code": code, "message": message.into()}
    })
}

pub fn notification<T: Serialize>(method: &str, params: T) -> Value {
    serde_json::json!({"jsonrpc": JSONRPC_VERSION, "method": method, "params": params})
}

pub fn decode_result<T: DeserializeOwned>(incoming: RpcIncoming) -> Result<T, ProtocolError> {
    if incoming.jsonrpc != JSONRPC_VERSION {
        return Err(ProtocolError::InvalidVersion(incoming.jsonrpc));
    }
    if let Some(error) = incoming.error {
        return Err(ProtocolError::Remote(error));
    }
    let result = incoming.result.ok_or(ProtocolError::MissingResult)?;
    serde_json::from_value(result).map_err(ProtocolError::Json)
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid JSON-RPC version: {0}")]
    InvalidVersion(String),
    #[error("response did not contain a result")]
    MissingResult,
    #[error("remote error {0:?}")]
    Remote(RpcError),
    #[error("invalid JSON: {0}")]
    Json(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_omits_absent_params() {
        let value = request(7, method::PING, None);
        assert_eq!(value["id"], 7);
        assert!(value.get("params").is_none());
    }

    #[test]
    fn status_notification_round_trips() {
        let params = StatusNotification {
            subscription_id: Uuid::nil(),
            status: StatusSnapshot {
                protocol_version: 1,
                daemon_version: "0.1.0".into(),
                pid: 1,
                health: HealthState::Ready,
                started_at: Utc::now(),
                uptime_ms: 5,
                tick: 2,
                tick_rate_hz: 20,
                cpu_usage_percent: Some(1.5),
                connected_clients: 1,
                mcp_url: "http://127.0.0.1:7777/mcp".into(),
                last_diagnostic: None,
                recent_events: Vec::new(),
            },
        };
        let wire = notification(method::STATUS_CHANGED, &params);
        let decoded: StatusNotification = serde_json::from_value(wire["params"].clone()).unwrap();
        assert_eq!(decoded, params);
    }

    #[test]
    fn status_from_older_protocol_one_daemon_defaults_cpu_to_unavailable() {
        let value = serde_json::json!({
            "protocol_version": 1,
            "daemon_version": "0.1.0",
            "pid": 1,
            "health": "ready",
            "started_at": Utc::now(),
            "uptime_ms": 5,
            "tick": 2,
            "tick_rate_hz": 20,
            "connected_clients": 1,
            "mcp_url": "http://127.0.0.1:7777/mcp",
            "last_diagnostic": null,
            "recent_events": []
        });
        let status: StatusSnapshot = serde_json::from_value(value).unwrap();
        assert_eq!(status.cpu_usage_percent, None);
    }
}
