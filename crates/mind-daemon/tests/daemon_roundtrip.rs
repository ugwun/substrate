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
    let initial = client.status().await.unwrap();
    assert_eq!(initial.tick_rate_hz, 100);
    let subscription_id = client.subscribe_status().await.unwrap();
    let update = client.next_status().await.unwrap();
    assert_eq!(update.subscription_id, subscription_id);
    assert!(update.status.tick >= initial.tick);
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
