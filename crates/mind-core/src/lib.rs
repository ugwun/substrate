//! Shared simulation state, configuration, and IPC contracts for Substrate.

pub mod campaign;
pub mod config;
pub mod engine;
pub mod game;
pub mod ipc;
pub mod protocol;
pub mod runtime;

pub const DAEMON_BINARY: &str = "mind-daemon";
pub const TUI_BINARY: &str = "mind-tui";
pub const PROTOCOL_VERSION: u32 = 14;
