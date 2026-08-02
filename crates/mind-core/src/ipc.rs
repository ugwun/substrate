use crate::{
    protocol::{
        self, decode_result, method, DiagnosticPulseResult, PingResult, ProtocolError, RpcIncoming,
        StatusNotification, StatusSnapshot, SubscribeResult, UnsubscribeParams, JSONRPC_VERSION,
        MAX_FRAME_BYTES,
    },
    PROTOCOL_VERSION,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{io, net::SocketAddr};
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
}

impl IpcClient {
    pub async fn connect(address: SocketAddr) -> Result<Self, IpcError> {
        let stream = TcpStream::connect(address).await?;
        let (reader, writer) = stream.into_split();
        let mut client = Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
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

    pub async fn next_status(&mut self) -> Result<StatusNotification, IpcError> {
        loop {
            let incoming = self.read_incoming().await?;
            if incoming.jsonrpc != JSONRPC_VERSION {
                return Err(IpcError::Protocol(ProtocolError::InvalidVersion(
                    incoming.jsonrpc,
                )));
            }
            if incoming.method.as_deref() == Some(method::STATUS_CHANGED) {
                let params = incoming.params.ok_or(IpcError::MissingNotificationParams)?;
                return Ok(serde_json::from_value(params)?);
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
