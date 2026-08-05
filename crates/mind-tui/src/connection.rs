use mind_core::{
    ipc::IpcClient,
    protocol::{CircuitGraphSnapshot, DiagnosticPulseResult, DslExecutionResult, StatusSnapshot},
};
use std::{net::SocketAddr, time::Duration};
use tokio::{sync::mpsc, time};

#[derive(Debug)]
pub enum ConnectionCommand {
    DiagnosticPulse,
    ExecuteDsl(String),
    Retry,
    Detach,
}

#[derive(Debug)]
pub enum ConnectionUpdate {
    Connecting,
    Connected,
    Snapshot(StatusSnapshot),
    Disconnected(String),
    DiagnosticCompleted(DiagnosticPulseResult),
    DiagnosticFailed(String),
    DslCompleted(DslExecutionResult),
    Graph(CircuitGraphSnapshot),
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

        let subscription_id = match client.subscribe_status().await {
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
        match client.status().await {
            Ok(status) => {
                if updates
                    .send(ConnectionUpdate::Snapshot(status))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                if !wait_to_retry(&mut commands, &updates, error.to_string()).await {
                    return;
                }
                continue;
            }
        }
        let mut graph_revision = match client.circuit_graph().await {
            Ok(graph) => {
                let revision = graph.revision;
                if updates.send(ConnectionUpdate::Graph(graph)).await.is_err() {
                    return;
                }
                revision
            }
            Err(error) => {
                if !wait_to_retry(&mut commands, &updates, error.to_string()).await {
                    return;
                }
                continue;
            }
        };

        let mut retry_immediately = false;
        let disconnect_reason = loop {
            tokio::select! {
                notification = client.next_status() => {
                    match notification {
                        Ok(notification) if notification.subscription_id == subscription_id => {
                            let next_graph_revision = notification.status.circuit.revision;
                            if updates.send(ConnectionUpdate::Snapshot(notification.status)).await.is_err() {
                                return;
                            }
                            if next_graph_revision != graph_revision {
                                match client.circuit_graph().await {
                                    Ok(graph) => {
                                        graph_revision = graph.revision;
                                        if updates.send(ConnectionUpdate::Graph(graph)).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(error) => break error.to_string(),
                                }
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
                        Some(ConnectionCommand::ExecuteDsl(command)) => {
                            match client.execute_dsl(command).await {
                                Ok(result) => {
                                    if updates.send(ConnectionUpdate::DslCompleted(result)).await.is_err() {
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
                                    match client.circuit_graph().await {
                                        Ok(graph) => {
                                            graph_revision = graph.revision;
                                            if updates.send(ConnectionUpdate::Graph(graph)).await.is_err() {
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
                            let _ = client.unsubscribe_status(subscription_id).await;
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
