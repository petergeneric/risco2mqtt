// MIT License - Copyright (c) 2021 TJForc
// Rust translation

pub mod command;
pub mod direct;
pub mod discovery;

use crate::error::Result;

/// Trait for transport implementations (direct TCP, proxy, etc.)
///
/// Not yet implemented by concrete types — serves as a future abstraction point.
#[allow(async_fn_in_trait, dead_code)]
pub(crate) trait Transport: Send + Sync {
    /// Send a command string and wait for a response.
    async fn send_command(&self, command: &str, is_prog_cmd: bool) -> Result<String>;

    /// Send an ACK for an unsolicited message.
    async fn send_ack(&self, id: &str) -> Result<()>;

    /// Disconnect from the panel.
    async fn disconnect(&self) -> Result<()>;

    /// Whether the transport is currently connected.
    fn is_connected(&self) -> bool;
}
