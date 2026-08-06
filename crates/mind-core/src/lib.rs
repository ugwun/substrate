//! Shared simulation state, configuration, and IPC contracts for Substrate.

pub mod config;
pub mod engine;
pub mod game;
pub mod ipc;
pub mod protocol;
pub mod runtime;

pub const DAEMON_BINARY: &str = "mind-daemon";
pub const PROTOCOL_VERSION: u32 = 3;
