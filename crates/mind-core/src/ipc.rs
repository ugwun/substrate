use crate::{
    protocol::{
        self, decode_result, method, CompleteOnboardingParams, DeployScoutParams,
        DiagnosticPulseResult, OnboardingCompletionResult, PingResult, PlanetActionResult,
        PlanetNotification, PlanetSnapshot, ProtocolError, RpcIncoming, StatusNotification,
        StatusSnapshot, SubscribeResult, UnsubscribeParams, JSONRPC_VERSION, MAX_FRAME_BYTES,
    },
    PROTOCOL_VERSION,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{collections::VecDeque, io, net::SocketAddr};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedReadHalf, tcp::OwnedWriteHalf, TcpStream},
};
use uuid::Uuid;

pub struct IpcClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    next_id: u64,
    pending_notifications: VecDeque<IpcNotification>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcNotification {
    Status(StatusNotification),
    Planet(PlanetNotification),
}

impl IpcClient {
    pub async fn connect(address: SocketAddr) -> Result<Self, IpcError> {
        let stream = TcpStream::connect(address).await?;
        let (reader, writer) = stream.into_split();
        let mut client = Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
            pending_notifications: VecDeque::new(),
        };
        let ping = client.ping().await?;
        if ping.protocol_version != PROTOCOL_VERSION {
            return Err(IpcError::IncompatibleProtocol {
                client: PROTOCOL_VERSION,
                daemon: ping.protocol_version,
            });
        }
        Ok(client)
    }

    pub async fn ping(&mut self) -> Result<PingResult, IpcError> {
        self.call(method::PING, None).await
    }

    pub async fn status(&mut self) -> Result<StatusSnapshot, IpcError> {
        self.call(method::GET_STATUS, None).await
    }

    pub async fn subscribe_status(&mut self) -> Result<Uuid, IpcError> {
        let result: SubscribeResult = self.call(method::SUBSCRIBE_STATUS, None).await?;
        Ok(result.subscription_id)
    }

    pub async fn unsubscribe_status(&mut self, subscription_id: Uuid) -> Result<bool, IpcError> {
        let params = serde_json::to_value(UnsubscribeParams { subscription_id })?;
        self.call(method::UNSUBSCRIBE_STATUS, Some(params)).await
    }

    pub async fn shutdown(&mut self) -> Result<bool, IpcError> {
        self.call(method::SHUTDOWN, None).await
    }

    pub async fn trigger_diagnostic_pulse(&mut self) -> Result<DiagnosticPulseResult, IpcError> {
        self.call(method::TRIGGER_DIAGNOSTIC_PULSE, None).await
    }

    pub async fn complete_onboarding(
        &mut self,
        band_positions: [u8; 3],
    ) -> Result<OnboardingCompletionResult, IpcError> {
        let params = serde_json::to_value(CompleteOnboardingParams { band_positions })?;
        self.call(method::COMPLETE_ONBOARDING, Some(params)).await
    }

    pub async fn planet(&mut self) -> Result<PlanetSnapshot, IpcError> {
        self.call(method::GET_PLANET, None).await
    }

    pub async fn subscribe_planet(&mut self) -> Result<Uuid, IpcError> {
        let result: SubscribeResult = self.call(method::SUBSCRIBE_PLANET, None).await?;
        Ok(result.subscription_id)
    }

    pub async fn unsubscribe_planet(&mut self, subscription_id: Uuid) -> Result<bool, IpcError> {
        let params = serde_json::to_value(UnsubscribeParams { subscription_id })?;
        self.call(method::UNSUBSCRIBE_PLANET, Some(params)).await
    }

    pub async fn deploy_scout(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(DeployScoutParams {
            region_id: region_id.into(),
        })?;
        self.call(method::DEPLOY_SCOUT, Some(params)).await
    }

    pub async fn recall_scout(&mut self) -> Result<PlanetActionResult, IpcError> {
        self.call(method::RECALL_SCOUT, None).await
    }

    pub async fn next_status(&mut self) -> Result<StatusNotification, IpcError> {
        loop {
            if let IpcNotification::Status(status) = self.next_notification().await? {
                return Ok(status);
            }
        }
    }

    pub async fn next_notification(&mut self) -> Result<IpcNotification, IpcError> {
        if let Some(notification) = self.pending_notifications.pop_front() {
            return Ok(notification);
        }
        loop {
            let incoming = self.read_incoming().await?;
            if incoming.jsonrpc != JSONRPC_VERSION {
                return Err(IpcError::Protocol(ProtocolError::InvalidVersion(
                    incoming.jsonrpc,
                )));
            }
            if let Some(notification) = Self::decode_notification(incoming)? {
                return Ok(notification);
            }
        }
    }

    async fn call<T: DeserializeOwned>(
        &mut self,
        method_name: &str,
        params: Option<Value>,
    ) -> Result<T, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_value(&protocol::request(id, method_name, params))
            .await?;
        loop {
            let incoming = self.read_incoming().await?;
            if incoming.id.as_ref() == Some(&Value::from(id)) {
                return decode_result(incoming).map_err(IpcError::Protocol);
            }
            if let Some(notification) = Self::decode_notification(incoming)? {
                self.pending_notifications.push_back(notification);
            }
        }
    }

    fn decode_notification(incoming: RpcIncoming) -> Result<Option<IpcNotification>, IpcError> {
        let Some(method_name) = incoming.method.as_deref() else {
            return Ok(None);
        };
        let params = incoming.params.ok_or(IpcError::MissingNotificationParams)?;
        match method_name {
            method::STATUS_CHANGED => Ok(Some(IpcNotification::Status(serde_json::from_value(
                params,
            )?))),
            method::PLANET_CHANGED => Ok(Some(IpcNotification::Planet(serde_json::from_value(
                params,
            )?))),
            _ => Ok(None),
        }
    }

    async fn write_value<T: Serialize>(&mut self, value: &T) -> Result<(), IpcError> {
        let mut encoded = serde_json::to_vec(value)?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(IpcError::FrameTooLarge(encoded.len()));
        }
        encoded.push(b'\n');
        self.writer.write_all(&encoded).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read_incoming(&mut self) -> Result<RpcIncoming, IpcError> {
        let mut frame = Vec::new();
        let read = self.reader.read_until(b'\n', &mut frame).await?;
        if read == 0 {
            return Err(IpcError::ConnectionClosed);
        }
        if frame.len() > MAX_FRAME_BYTES + 1 {
            return Err(IpcError::FrameTooLarge(frame.len()));
        }
        Ok(serde_json::from_slice(&frame)?)
    }
}

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("IPC I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid IPC JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("IPC connection closed")]
    ConnectionClosed,
    #[error("IPC frame is too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("status notification had no parameters")]
    MissingNotificationParams,
    #[error("incompatible protocol: client {client}, daemon {daemon}")]
    IncompatibleProtocol { client: u32, daemon: u32 },
}
