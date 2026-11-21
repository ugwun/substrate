use crate::{
    protocol::{
        self, decode_result, method, AcknowledgeActivityParams, ActivateProfileParams,
        BaseModuleKind, CompleteOnboardingParams, ConfigureExtractorParams,
        ConstructBaseModuleParams, ConstructRelayParams, DeployExtractorParams, DeployScoutParams,
        DiagnosticPulseResult, ExtractorLoadout, NewPlaythroughResult, OnboardingCompletionResult,
        PingResult, PlanetActionResult, PlanetNotification, PlanetSnapshot, ProtocolError,
        RegionAutomationParams, ResearchProject, RpcIncoming, SetBaseModuleEnabledParams,
        StartResearchParams, StatusNotification, StatusSnapshot, SubregionParams, SubscribeResult,
        UnsubscribeParams, JSONRPC_VERSION, MAX_FRAME_BYTES,
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
    Planet(Box<PlanetNotification>),
}

impl IpcClient {
    pub async fn connect(address: SocketAddr) -> Result<Self, IpcError> {
        let (client, ping) = Self::connect_and_ping(address).await?;
        if ping.protocol_version != PROTOCOL_VERSION {
            return Err(IpcError::IncompatibleProtocol {
                client: PROTOCOL_VERSION,
                daemon: ping.protocol_version,
            });
        }
        Ok(client)
    }

    /// Probe a daemon without requiring it to use the current gameplay protocol.
    ///
    /// Lifecycle clients use this to distinguish a stale metadata file from an older
    /// Substrate daemon that still owns an endpoint.
    pub async fn probe(address: SocketAddr) -> Result<PingResult, IpcError> {
        let (_, ping) = Self::connect_and_ping(address).await?;
        Ok(ping)
    }

    /// Ask a daemon to shut down after verifying its identity with `system.ping`.
    ///
    /// Ping and graceful shutdown are stable lifecycle methods, so this deliberately
    /// works across gameplay protocol revisions.
    pub async fn request_shutdown(address: SocketAddr) -> Result<PingResult, IpcError> {
        let (mut client, ping) = Self::connect_and_ping(address).await?;
        client.shutdown().await?;
        Ok(ping)
    }

    async fn connect_and_ping(address: SocketAddr) -> Result<(Self, PingResult), IpcError> {
        let stream = TcpStream::connect(address).await?;
        let (reader, writer) = stream.into_split();
        let mut client = Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
            pending_notifications: VecDeque::new(),
        };
        let ping = client.ping().await?;
        Ok((client, ping))
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

    pub async fn start_new_playthrough(&mut self) -> Result<NewPlaythroughResult, IpcError> {
        self.call(method::START_NEW_PLAYTHROUGH, None).await
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

    pub async fn deploy_extractor(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        self.deploy_extractor_at(region_id, None::<String>).await
    }

    pub async fn deploy_extractor_at(
        &mut self,
        region_id: impl Into<String>,
        subregion_id: Option<impl Into<String>>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(DeployExtractorParams {
            region_id: region_id.into(),
            subregion_id: subregion_id.map(Into::into),
        })?;
        self.call(method::DEPLOY_EXTRACTOR, Some(params)).await
    }

    pub async fn configure_extractor(
        &mut self,
        loadout: ExtractorLoadout,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(ConfigureExtractorParams { loadout })?;
        self.call(method::CONFIGURE_EXTRACTOR, Some(params)).await
    }

    pub async fn survey_subregion(
        &mut self,
        subregion_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(SubregionParams {
            subregion_id: subregion_id.into(),
        })?;
        self.call(method::SURVEY_SUBREGION, Some(params)).await
    }

    pub async fn establish_base(
        &mut self,
        subregion_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(SubregionParams {
            subregion_id: subregion_id.into(),
        })?;
        self.call(method::ESTABLISH_BASE, Some(params)).await
    }

    pub async fn construct_base_module(
        &mut self,
        subregion_id: impl Into<String>,
        module: BaseModuleKind,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(ConstructBaseModuleParams {
            subregion_id: subregion_id.into(),
            module,
        })?;
        self.call(method::CONSTRUCT_BASE_MODULE, Some(params)).await
    }

    pub async fn set_base_module_enabled(
        &mut self,
        subregion_id: impl Into<String>,
        module: BaseModuleKind,
        enabled: bool,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(SetBaseModuleEnabledParams {
            subregion_id: subregion_id.into(),
            module,
            enabled,
        })?;
        self.call(method::SET_BASE_MODULE_ENABLED, Some(params))
            .await
    }

    pub async fn maintain_base(
        &mut self,
        subregion_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(SubregionParams {
            subregion_id: subregion_id.into(),
        })?;
        self.call(method::MAINTAIN_BASE, Some(params)).await
    }

    pub async fn construct_relay(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(ConstructRelayParams {
            region_id: region_id.into(),
        })?;
        self.call(method::CONSTRUCT_RELAY, Some(params)).await
    }

    pub async fn start_research(
        &mut self,
        project: ResearchProject,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(StartResearchParams { project })?;
        self.call(method::START_RESEARCH, Some(params)).await
    }

    pub async fn start_archive_experiment(&mut self) -> Result<PlanetActionResult, IpcError> {
        self.call(method::START_ARCHIVE_EXPERIMENT, None).await
    }

    pub async fn activate_profile(
        &mut self,
        profile: crate::protocol::OperationalProfile,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(ActivateProfileParams { profile })?;
        self.call(method::ACTIVATE_PROFILE, Some(params)).await
    }

    pub async fn queue_survey(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(RegionAutomationParams {
            region_id: region_id.into(),
        })?;
        self.call(method::QUEUE_SURVEY, Some(params)).await
    }

    pub async fn remove_queued_survey(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(RegionAutomationParams {
            region_id: region_id.into(),
        })?;
        self.call(method::REMOVE_QUEUED_SURVEY, Some(params)).await
    }

    pub async fn set_extraction_route(
        &mut self,
        region_id: impl Into<String>,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(RegionAutomationParams {
            region_id: region_id.into(),
        })?;
        self.call(method::SET_EXTRACTION_ROUTE, Some(params)).await
    }

    pub async fn clear_extraction_route(&mut self) -> Result<PlanetActionResult, IpcError> {
        self.call(method::CLEAR_EXTRACTION_ROUTE, None).await
    }

    pub async fn acknowledge_activity(
        &mut self,
        through_activity_id: u64,
    ) -> Result<PlanetActionResult, IpcError> {
        let params = serde_json::to_value(AcknowledgeActivityParams {
            through_activity_id,
        })?;
        self.call(method::ACKNOWLEDGE_ACTIVITY, Some(params)).await
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
            method::PLANET_CHANGED => Ok(Some(IpcNotification::Planet(Box::new(
                serde_json::from_value(params)?,
            )))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    #[tokio::test]
    async fn lifecycle_shutdown_accepts_an_older_daemon_protocol() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();

            let ping_request: protocol::RpcRequest =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(ping_request.method, protocol::method::PING);
            let ping = PingResult {
                protocol_version: PROTOCOL_VERSION - 1,
                daemon_version: "previous".into(),
            };
            let mut response =
                serde_json::to_vec(&protocol::success(ping_request.id, ping)).unwrap();
            response.push(b'\n');
            writer.write_all(&response).await.unwrap();

            let shutdown_request: protocol::RpcRequest =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(shutdown_request.method, protocol::method::SHUTDOWN);
            let mut response =
                serde_json::to_vec(&protocol::success(shutdown_request.id, true)).unwrap();
            response.push(b'\n');
            writer.write_all(&response).await.unwrap();
        });

        let ping = IpcClient::request_shutdown(address).await.unwrap();
        assert_eq!(ping.protocol_version, PROTOCOL_VERSION - 1);
        server.await.unwrap();
    }
}
