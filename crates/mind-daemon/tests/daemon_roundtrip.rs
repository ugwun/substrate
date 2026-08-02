use mind_core::{
    config::{AppPaths, Config},
    ipc::IpcClient,
    runtime::RuntimeMetadata,
};
use std::{net::SocketAddr, time::Duration};
use tempfile::TempDir;

fn paths(temp: &TempDir) -> AppPaths {
    AppPaths {
        config_file: temp.path().join("config.toml"),
        data_dir: temp.path().join("data"),
        runtime_file: temp.path().join("data/daemon.json"),
        log_file: temp.path().join("data/mind-daemon.log"),
    }
}

#[tokio::test]
async fn serves_status_subscription_and_shutdown() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: "127.0.0.1:0".parse().unwrap(),
        tick_rate_hz: 100,
        status_publish_interval_ms: 50,
    };
    let runtime_file = paths.runtime_file.clone();
    let daemon = tokio::spawn(mind_daemon::run(config, paths));

    let metadata = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(metadata) = RuntimeMetadata::read(&runtime_file).unwrap() {
                break metadata;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let mut client = IpcClient::connect(metadata.address).await.unwrap();
    assert!(tokio::net::TcpStream::connect(metadata.mcp_address)
        .await
        .is_ok());
    let initial = client.status().await.unwrap();
    assert_eq!(initial.tick_rate_hz, 100);
    let subscription_id = client.subscribe_status().await.unwrap();
    let pulse = client.trigger_diagnostic_pulse().await.unwrap();
    assert_eq!(pulse.pulse_id, 1);
    assert_eq!(client.status().await.unwrap().last_diagnostic, Some(pulse));
    let update = client.next_status().await.unwrap();
    assert_eq!(update.subscription_id, subscription_id);
    assert!(update.status.tick >= initial.tick);
    let cpu_status = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let update = client.next_status().await.unwrap();
            if update.status.cpu_usage_percent.is_some() {
                break update.status;
            }
        }
    })
    .await
    .unwrap();
    assert!(cpu_status.cpu_usage_percent.unwrap() >= 0.0);
    assert!(client.shutdown().await.unwrap());
    drop(client);

    tokio::time::timeout(Duration::from_secs(2), daemon)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(RuntimeMetadata::read(&runtime_file).unwrap().is_none());
}

#[tokio::test]
async fn refuses_non_loopback_configuration_before_run() {
    let config = Config {
        address: SocketAddr::from(([0, 0, 0, 0], 7776)),
        ..Config::default()
    };
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn mcp_port_conflict_aborts_before_runtime_metadata() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    paths.ensure_directories().unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = Config {
        address: "127.0.0.1:0".parse().unwrap(),
        mcp_address: occupied.local_addr().unwrap(),
        ..Config::default()
    };
    let runtime_file = paths.runtime_file.clone();
    let error = mind_daemon::run(config, paths).await.unwrap_err();
    assert!(error.to_string().contains("failed to bind MCP listener"));
    assert!(RuntimeMetadata::read(&runtime_file).unwrap().is_none());
}
