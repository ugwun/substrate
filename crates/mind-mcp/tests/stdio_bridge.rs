use std::process::Command;
use tempfile::TempDir;

#[test]
fn exits_cleanly_without_writing_protocol_noise_when_daemon_is_absent() {
    let temp = TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mind-mcp"))
        .arg("--config")
        .arg(temp.path().join("config.toml"))
        .arg("--addr")
        .arg("127.0.0.1:0")
        .env("SUBSTRATE_DATA_DIR", temp.path().join("data"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "stdout must contain only MCP frames"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mind daemon is not reachable"));
}
