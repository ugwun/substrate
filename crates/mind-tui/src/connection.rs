use mind_core::{
    ipc::{IpcClient, IpcNotification},
    protocol::{
        DiagnosticPulseResult, OnboardingCompletionResult, PlanetActionResult, PlanetSnapshot,
        StatusSnapshot,
    },
};
use std::{net::SocketAddr, time::Duration};
use tokio::{sync::mpsc, time};
use uuid::Uuid;

#[derive(Debug)]
pub enum ConnectionCommand {
    DiagnosticPulse,
    CompleteOnboarding([u8; 3]),
    DeployScout(String),
    RecallScout,
    DeployExtractor(String),
    ConstructRelay(String),
    Retry,
    Detach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanetActionKind {
    Deploy,
    Recall,
    Extract,
    Construct,
}

#[derive(Debug)]
pub enum ConnectionUpdate {
    Connecting,
    Connected,
    Snapshot(StatusSnapshot),
    Planet(Box<PlanetSnapshot>),
    Disconnected(String),
    DiagnosticCompleted(DiagnosticPulseResult),
    DiagnosticFailed(String),
    OnboardingCompleted(OnboardingCompletionResult),
    PlanetActionCompleted(PlanetActionKind, PlanetActionResult),
}

pub async fn run(
    address: SocketAddr,
    mut commands: mpsc::Receiver<ConnectionCommand>,
    updates: mpsc::Sender<ConnectionUpdate>,
) {
    loop {
        if updates.send(ConnectionUpdate::Connecting).await.is_err() {
            return;
        }
        let connection = time::timeout(Duration::from_secs(1), IpcClient::connect(address)).await;
        let mut client = match connection {
            Ok(Ok(client)) => client,
            Ok(Err(error)) => {
                if !wait_to_retry(&mut commands, &updates, error.to_string()).await {
                    return;
                }
                continue;
            }
            Err(_) => {
                if !wait_to_retry(
                    &mut commands,
                    &updates,
                    format!("connection to {address} timed out"),
                )
                .await
                {
                    return;
                }
                continue;
            }
        };

        let status_subscription = match client.subscribe_status().await {
            Ok(id) => id,
            Err(error) => {
                if !wait_to_retry(&mut commands, &updates, error.to_string()).await {
                    return;
                }
                continue;
            }
        };
        if updates.send(ConnectionUpdate::Connected).await.is_err() {
            return;
        }
        let initial_status = match client.status().await {
            Ok(status) => status,
            Err(error) => {
                if !wait_to_retry(&mut commands, &updates, error.to_string()).await {
                    return;
                }
                continue;
            }
        };
        let onboarding_complete = initial_status.onboarding.complete;
        if updates
            .send(ConnectionUpdate::Snapshot(initial_status))
            .await
            .is_err()
        {
            return;
        }
        let mut planet_subscription = if onboarding_complete {
            match attach_planet(&mut client, &updates).await {
                Ok(id) => Some(id),
                Err(error) => {
                    if !wait_to_retry(&mut commands, &updates, error).await {
                        return;
                    }
                    continue;
                }
            }
        } else {
            None
        };

        let mut retry_immediately = false;
        let disconnect_reason = loop {
            tokio::select! {
                notification = client.next_notification() => {
                    match notification {
                        Ok(IpcNotification::Status(notification))
                            if notification.subscription_id == status_subscription =>
                        {
                            let completed = notification.status.onboarding.complete;
                            if updates.send(ConnectionUpdate::Snapshot(notification.status)).await.is_err() {
                                return;
                            }
                            if completed && planet_subscription.is_none() {
                                match attach_planet(&mut client, &updates).await {
                                    Ok(id) => planet_subscription = Some(id),
                                    Err(error) => break error,
                                }
                            }
                        }
                        Ok(IpcNotification::Planet(notification))
                            if Some(notification.subscription_id) == planet_subscription =>
                        {
                            if updates.send(ConnectionUpdate::Planet(Box::new(notification.planet))).await.is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(error) => break error.to_string(),
                    }
                }
                command = commands.recv() => {
                    match command {
                        Some(ConnectionCommand::DiagnosticPulse) => {
                            match client.trigger_diagnostic_pulse().await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::DiagnosticCompleted(result)).await.is_err() {
                                        return;
                                    }
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    let _ = updates.send(ConnectionUpdate::DiagnosticFailed(message.clone())).await;
                                    break message;
                                }
                            }
                        }
                        Some(ConnectionCommand::CompleteOnboarding(positions)) => {
                            match client.complete_onboarding(positions).await {
                                Ok(result) => {
                                    let completed = result.ok;
                                    if updates.send(ConnectionUpdate::OnboardingCompleted(result)).await.is_err() {
                                        return;
                                    }
                                    match client.status().await {
                                        Ok(status) => {
                                            if updates.send(ConnectionUpdate::Snapshot(status)).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(error) => break error.to_string(),
                                    }
                                    if completed && planet_subscription.is_none() {
                                        match attach_planet(&mut client, &updates).await {
                                            Ok(id) => planet_subscription = Some(id),
                                            Err(error) => break error,
                                        }
                                    }
                                }
                                Err(error) => break error.to_string(),
                            }
                        }
                        Some(ConnectionCommand::DeployScout(region_id)) => {
                            match client.deploy_scout(region_id).await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::PlanetActionCompleted(PlanetActionKind::Deploy, result)).await.is_err() {
                                        return;
                                    }
                                    match client.planet().await {
                                        Ok(planet) => {
                                            if updates.send(ConnectionUpdate::Planet(Box::new(planet))).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(error) => break error.to_string(),
                                    }
                                }
                                Err(error) => break error.to_string(),
                            }
                        }
                        Some(ConnectionCommand::RecallScout) => {
                            match client.recall_scout().await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::PlanetActionCompleted(PlanetActionKind::Recall, result)).await.is_err() {
                                        return;
                                    }
                                    match client.planet().await {
                                        Ok(planet) => {
                                            if updates.send(ConnectionUpdate::Planet(Box::new(planet))).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(error) => break error.to_string(),
                                    }
                                }
                                Err(error) => break error.to_string(),
                            }
                        }
                        Some(ConnectionCommand::DeployExtractor(region_id)) => {
                            match client.deploy_extractor(region_id).await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::PlanetActionCompleted(PlanetActionKind::Extract, result)).await.is_err() {
                                        return;
                                    }
                                    match client.planet().await {
                                        Ok(planet) => {
                                            if updates.send(ConnectionUpdate::Planet(Box::new(planet))).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(error) => break error.to_string(),
                                    }
                                }
                                Err(error) => break error.to_string(),
                            }
                        }
                        Some(ConnectionCommand::ConstructRelay(region_id)) => {
                            match client.construct_relay(region_id).await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::PlanetActionCompleted(PlanetActionKind::Construct, result)).await.is_err() {
                                        return;
                                    }
                                    match client.planet().await {
                                        Ok(planet) => {
                                            if updates.send(ConnectionUpdate::Planet(Box::new(planet))).await.is_err() {
                                                return;
                                            }
                                        }
                                        Err(error) => break error.to_string(),
                                    }
                                }
                                Err(error) => break error.to_string(),
                            }
                        }
                        Some(ConnectionCommand::Retry) => {
                            retry_immediately = true;
                            break "manual reconnect requested".to_owned();
                        }
                        Some(ConnectionCommand::Detach) | None => {
                            let _ = client.unsubscribe_status(status_subscription).await;
                            if let Some(id) = planet_subscription {
                                let _ = client.unsubscribe_planet(id).await;
                            }
                            return;
                        }
                    }
                }
            }
        };

        if retry_immediately {
            continue;
        }
        if !wait_to_retry(&mut commands, &updates, disconnect_reason).await {
            return;
        }
    }
}

async fn attach_planet(
    client: &mut IpcClient,
    updates: &mpsc::Sender<ConnectionUpdate>,
) -> Result<Uuid, String> {
    let subscription_id = client
        .subscribe_planet()
        .await
        .map_err(|error| error.to_string())?;
    let planet = client.planet().await.map_err(|error| error.to_string())?;
    updates
        .send(ConnectionUpdate::Planet(Box::new(planet)))
        .await
        .map_err(|_| "TUI update channel closed".to_owned())?;
    Ok(subscription_id)
}

async fn wait_to_retry(
    commands: &mut mpsc::Receiver<ConnectionCommand>,
    updates: &mpsc::Sender<ConnectionUpdate>,
    reason: String,
) -> bool {
    if updates
        .send(ConnectionUpdate::Disconnected(reason))
        .await
        .is_err()
    {
        return false;
    }
    tokio::select! {
        _ = time::sleep(Duration::from_secs(1)) => true,
        command = commands.recv() => !matches!(command, Some(ConnectionCommand::Detach) | None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_failed_attachment_and_accepts_detach() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (command_tx, command_rx) = mpsc::channel(2);
        let (update_tx, mut update_rx) = mpsc::channel(4);
        let worker = tokio::spawn(run(address, command_rx, update_tx));

        assert!(matches!(
            update_rx.recv().await,
            Some(ConnectionUpdate::Connecting)
        ));
        assert!(matches!(
            update_rx.recv().await,
            Some(ConnectionUpdate::Disconnected(_))
        ));
        command_tx.send(ConnectionCommand::Detach).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap();
    }
}
