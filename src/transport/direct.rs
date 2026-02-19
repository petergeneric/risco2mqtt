// MIT License - Copyright (c) 2021 TJForc
// Rust translation of Risco_DirectTCP_Socket from RiscoChannels.js

use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use crate::config::PanelConfig;
use crate::constants::{DLE, ETX};
use crate::crypto::RiscoCrypt;
use crate::error::{PanelErrorCode, RiscoError, Result};
use crate::event::{EventSender, PanelEvent};
use crate::protocol::Command;
use crate::transport::command::CommandEngine;
use crate::transport::discovery;

/// Direct TCP transport — connects directly to the panel on its TCP port.
pub struct DirectTcpTransport {
    pub command_engine: Arc<CommandEngine>,
    event_tx: EventSender,
    reader_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DirectTcpTransport {
    /// Connect to the panel and establish an authenticated, encrypted session.
    ///
    /// Sequence: TCP connect → 10s delay → RMT auth → LCL → crypt test → PanelConnected
    pub async fn connect(config: &PanelConfig, event_tx: EventSender) -> Result<Self> {
        info!(
            "Connecting to panel at {}:{}",
            config.panel_ip, config.panel_port
        );

        let stream = TcpStream::connect(format!("{}:{}", config.panel_ip, config.panel_port))
            .await
            .map_err(|e| {
                error!("TCP connect failed: {}", e);
                RiscoError::Io(e)
            })?;

        debug!("TCP socket connected");

        let (reader, writer) = stream.into_split();
        let crypt = RiscoCrypt::new(config.panel_id);
        let command_engine = Arc::new(CommandEngine::new(writer, crypt));

        // Spawn reader task with socket-level read timeout
        let socket_timeout = Duration::from_millis(config.socket_timeout_ms.max(1000));
        let reader_handle = spawn_reader_task(
            reader,
            command_engine.clone(),
            event_tx.clone(),
            socket_timeout,
        );

        let transport = Self {
            command_engine: command_engine.clone(),
            event_tx: event_tx.clone(),
            reader_handle: Some(reader_handle),
        };

        // Mandatory 10-second delay before sending RMT
        info!("Connection establishment will start in 10 seconds...");
        sleep(Duration::from_secs(10)).await;

        // Authenticate
        let password = &config.panel_password;
        let code_len = password.len().max(4);
        let padded_password = format!("{:0>width$}", password, width = code_len);
        let rmt_cmd = Command::Rmt { password: padded_password.clone() };

        debug!("Sending RMT authentication");
        let response = command_engine.send_command(&rmt_cmd, false).await?;

        if !response.contains("ACK") {
            if PanelErrorCode::is_error_code(&response) {
                if config.discover_code {
                    // Password discovery would happen here
                    error!("Bad access code: {}. Discovery not yet implemented in connect flow.", password);
                }
                return Err(RiscoError::BadAccessCode {
                    code: password.clone(),
                });
            }
            return Err(RiscoError::InvalidResponse {
                details: format!("RMT response: {}", response),
            });
        }
        debug!("RMT authentication successful");

        // Start encrypted session
        let lcl_response = command_engine.send_command(&Command::Lcl, false).await?;
        if !lcl_response.contains("ACK") {
            return Err(RiscoError::InvalidResponse {
                details: format!("LCL response: {}", lcl_response),
            });
        }
        debug!("LCL accepted, enabling encryption");

        // Enable encryption
        {
            let crypt_arc = command_engine.crypt();
            let mut crypt = crypt_arc.lock().await;
            crypt.set_crypt_enabled(true);
        }

        sleep(Duration::from_millis(1000)).await;

        // Test encryption with CUSTLST? command (retry up to 3 times)
        command_engine.set_in_crypt_test(true).await;
        let mut crypt_test_passed = false;

        for attempt in 1..=3u8 {
            // Reset validity flag before each attempt
            *command_engine.crypt_key_valid.write().await = None;

            let crypt_test_result = command_engine.send_command(&Command::CustomerList, false).await;
            let crypt_valid = {
                let valid = command_engine.crypt_key_valid.read().await;
                valid.unwrap_or(false)
            };

            match crypt_test_result {
                Ok(ref response) if crypt_valid && !CommandEngine::is_error_code(response) => {
                    debug!("Crypt test passed (attempt {})", attempt);
                    crypt_test_passed = true;
                    break;
                }
                Ok(ref response) => {
                    warn!("Crypt test failed (attempt {}/3): response={}", attempt, response);
                }
                Err(ref e) => {
                    warn!("Crypt test error (attempt {}/3): {}", attempt, e);
                }
            }

            if attempt < 3 {
                sleep(Duration::from_millis(500)).await;
            }
        }

        if !crypt_test_passed {
            if config.discover_code {
                warn!("Bad panel ID: {}. Attempting discovery...", config.panel_id);
                match discovery::discover_panel_id(&command_engine).await {
                    Ok(discovered_id) => {
                        warn!("Discovered panel ID: {}. Set panel_id to this value in your config to skip discovery on future connections.", discovered_id);
                    }
                    Err(e) => {
                        error!("Panel ID discovery failed: {}", e);
                        command_engine.set_in_crypt_test(false).await;
                        return Err(e);
                    }
                }
            } else {
                command_engine.set_in_crypt_test(false).await;
                return Err(RiscoError::BadCryptKey {
                    panel_id: config.panel_id,
                });
            }
        }
        command_engine.set_in_crypt_test(false).await;

        info!("Connection to panel successfully established");
        let _ = event_tx.send(PanelEvent::Connected);

        Ok(transport)
    }

    /// Send a command through the transport.
    pub async fn send_command(&self, command: &Command, is_prog_cmd: bool) -> Result<String> {
        self.command_engine.send_command(command, is_prog_cmd).await
    }

    /// Disconnect from the panel.
    pub async fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from panel");
        let _ = self.event_tx.send(PanelEvent::Disconnected);
        self.command_engine.disconnect().await?;
        Ok(())
    }

    /// Whether the transport is connected.
    pub async fn is_connected(&self) -> bool {
        *self.command_engine.connected_flag().read().await
    }

    /// Get the command engine for programming mode operations.
    pub fn engine(&self) -> &Arc<CommandEngine> {
        &self.command_engine
    }
}

impl Drop for DirectTcpTransport {
    fn drop(&mut self) {
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }
    }
}

/// Spawn the reader task that processes incoming data from the panel.
///
/// A read timeout ensures that stale connections (where the panel silently
/// stops sending data without closing the TCP socket) are detected and
/// trigger a reconnection. This matches the JS implementation's 30-second
/// `socket.setTimeout()` behaviour.
fn spawn_reader_task(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    engine: Arc<CommandEngine>,
    event_tx: EventSender,
    socket_timeout: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        let mut leftover = Vec::new();

        loop {
            match tokio::time::timeout(socket_timeout, reader.read(&mut buf)).await {
                Ok(Ok(0)) => {
                    // Connection closed
                    debug!("Reader: connection closed");
                    engine.set_connected(false).await;
                    let _ = event_tx.send(PanelEvent::Disconnected);
                    break;
                }
                Ok(Ok(n)) => {
                    let mut data = leftover.clone();
                    data.extend_from_slice(&buf[..n]);
                    leftover.clear();

                    // Split on ETX+STX boundary (multiple messages in one read)
                    let messages = split_messages(&data, &mut leftover);

                    for msg in messages {
                        process_message(&msg, &engine, &event_tx).await;
                    }
                }
                Ok(Err(e)) => {
                    error!("Reader: read error: {}", e);
                    engine.set_connected(false).await;
                    let _ = event_tx.send(PanelEvent::Disconnected);
                    break;
                }
                Err(_) => {
                    // Socket read timeout — no data received within the timeout period.
                    // The connection is likely stale; trigger reconnection.
                    warn!(
                        "Reader: no data received for {:.0}s, connection appears stale",
                        socket_timeout.as_secs_f64()
                    );
                    engine.set_connected(false).await;
                    let _ = event_tx.send(PanelEvent::Disconnected);
                    break;
                }
            }
        }
    })
}

/// Split a data buffer into individual messages on ETX boundaries,
/// respecting DLE byte-stuffing (escaped ETX bytes are not frame delimiters).
///
/// The Risco protocol uses DLE-stuffing: any encrypted byte that equals
/// STX (0x02), ETX (0x03), or DLE (0x10) is preceded by a DLE byte on the wire.
/// Only a bare ETX (not preceded by DLE) marks the end of a frame.
fn split_messages(data: &[u8], leftover: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < data.len() {
        if data[i] == DLE && i + 1 < data.len() {
            // DLE escape sequence — skip both the DLE and the following byte
            // so that an escaped ETX (DLE+ETX) is not treated as a frame end.
            i += 2;
        } else if data[i] == ETX {
            let msg = &data[start..=i];
            messages.push(msg.to_vec());
            start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    // If there's remaining data that doesn't end with ETX, save as leftover
    if start < data.len() {
        leftover.extend_from_slice(&data[start..]);
    }

    messages
}

/// Process a single decoded message from the panel.
async fn process_message(
    data: &[u8],
    engine: &Arc<CommandEngine>,
    event_tx: &EventSender,
) {
    // Store raw encrypted bytes before decoding (needed for offline panel ID discovery)
    *engine.last_received_buffer().lock().await = Some(data.to_vec());

    let decoded = {
        let crypt_arc = engine.crypt();
        let mut crypt = crypt_arc.lock().await;
        crypt.decode_message(data)
    };

    if !decoded.crc_valid {
        let in_crypt_test = *engine.in_crypt_test_flag().read().await;
        if !in_crypt_test {
            // Bad CRC in normal mode
            if engine.handle_bad_crc().await {
                // Too many bad CRCs, disconnect
                engine.set_connected(false).await;
                let _ = event_tx.send(PanelEvent::Disconnected);
                return;
            }
            // Store misunderstood data
            *engine.last_misunderstood().lock().await = Some(decoded.command.clone());
        } else {
            // Bad CRC during crypt test = wrong key
            *engine.crypt_key_valid.write().await = Some(false);
            *engine.last_misunderstood().lock().await = Some(decoded.command.clone());
        }
        return;
    }

    // Valid CRC
    if *engine.in_crypt_test_flag().read().await {
        *engine.crypt_key_valid.write().await = Some(true);
    }

    let receive_id = &decoded.cmd_id;

    if receive_id.is_empty() && CommandEngine::is_error_code(&decoded.command) {
        // Error response with no command ID
        *engine.last_misunderstood().lock().await = Some(decoded.command.clone());
        return;
    }

    if let Ok(id_num) = receive_id.parse::<u8>() {
        if id_num >= 50 {
            // Unsolicited message from panel — always send ACK
            let _ = engine.send_ack(receive_id).await;

            // Check for duplicate message ID
            let last_id_arc = engine.last_received_unsolicited_id();
            let mut last_id = last_id_arc.lock().await;
            if *last_id == Some(id_num) {
                debug!("Duplicate unsolicited message (id={}), skipping", id_num);
            } else {
                *last_id = Some(id_num);
                debug!("Unsolicited data from panel (id={})", id_num);
                emit_panel_data(event_tx, &decoded.command);
            }
        } else {
            // Response to our command — route to matching pending command by ID
            let pending_arc = engine.pending_commands();
            let mut pending = pending_arc.lock().await;
            if let Some(sender) = pending.remove(&id_num) {
                let _ = sender.send(decoded.command.clone());
                debug!("Routed response for seq {}", id_num);
            } else {
                debug!("No pending command for seq {} (possibly already timed out)", id_num);
            }
        }
    }
}

/// Emit panel data through the event system so the panel can update cached state.
fn emit_panel_data(event_tx: &EventSender, command: &str) {
    // Only emit status update messages that affect cached device state.
    if command.starts_with("ZSTT")
        || command.starts_with("PSTT")
        || command.starts_with("OSTT")
        || command.starts_with("SSTT")
    {
        let _ = event_tx.send(PanelEvent::PanelData(command.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{DLE, ETX, STX, CRYPT};

    #[test]
    fn test_split_single_message() {
        let data = [STX, 0x41, 0x42, ETX];
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], data);
        assert!(leftover.is_empty());
    }

    #[test]
    fn test_split_two_messages() {
        let data = [STX, 0x41, ETX, STX, 0x42, ETX];
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], [STX, 0x41, ETX]);
        assert_eq!(msgs[1], [STX, 0x42, ETX]);
    }

    #[test]
    fn test_split_dle_escaped_etx_not_treated_as_boundary() {
        // Encrypted payload contains DLE+ETX (an escaped 0x03 data byte).
        // This must NOT split the message.
        let data = [STX, CRYPT, 0x41, DLE, ETX, 0x42, ETX];
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 1, "DLE-escaped ETX should not split the message");
        assert_eq!(msgs[0], data);
    }

    #[test]
    fn test_split_dle_escaped_etx_with_multiple_messages() {
        // First message has DLE-escaped ETX in payload, second is normal.
        let msg1 = [STX, CRYPT, 0x41, DLE, ETX, 0x42, ETX];
        let msg2 = [STX, 0x43, ETX];
        let mut data = Vec::new();
        data.extend_from_slice(&msg1);
        data.extend_from_slice(&msg2);
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], msg1);
        assert_eq!(msgs[1], msg2.to_vec());
    }

    #[test]
    fn test_split_dle_escaped_dle() {
        // DLE+DLE is an escaped DLE byte, not related to ETX.
        let data = [STX, CRYPT, DLE, DLE, 0x41, ETX];
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], data);
    }

    #[test]
    fn test_split_incomplete_message_goes_to_leftover() {
        let data = [STX, 0x41, 0x42]; // no ETX
        let mut leftover = Vec::new();
        let msgs = split_messages(&data, &mut leftover);
        assert_eq!(msgs.len(), 0);
        assert_eq!(leftover, data);
    }

    #[test]
    fn test_split_roundtrip_with_crypto() {
        // Encode a command with encryption, verify split_messages keeps it intact.
        use crate::crypto::RiscoCrypt;
        for panel_id in [1, 100, 1234, 5678, 9999] {
            let mut crypt = RiscoCrypt::new(panel_id);
            crypt.set_crypt_enabled(true);
            let encoded = crypt.encode_command("CUSTLST?", "01", None);
            let mut leftover = Vec::new();
            let msgs = split_messages(&encoded, &mut leftover);
            assert_eq!(
                msgs.len(),
                1,
                "Encoded message for panel_id {} was incorrectly split",
                panel_id
            );
            assert_eq!(msgs[0], encoded);
            assert!(leftover.is_empty());
        }
    }
}
