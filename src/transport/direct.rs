// MIT License - Copyright (c) 2021 TJForc
// Rust translation of Risco_DirectTCP_Socket from RiscoChannels.js

use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

use crate::config::PanelConfig;
use crate::constants::ETX;
use crate::crypto::RiscoCrypt;
use crate::error::{PanelErrorCode, RiscoError, Result};
use crate::event::{EventSender, PanelEvent};
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
        let command_engine = Arc::new(CommandEngine::new(writer, crypt, false));

        // Spawn reader task
        let reader_handle = spawn_reader_task(
            reader,
            command_engine.clone(),
            event_tx.clone(),
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
        let rmt_cmd = format!("RMT={}", padded_password);

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
        let lcl_response = command_engine.send_command("LCL", false).await?;
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

        // Test encryption with CUSTLST? command
        command_engine.set_in_crypt_test(true).await;
        let crypt_test_result = command_engine.send_command("CUSTLST?", false).await;
        let crypt_valid = {
            let valid = command_engine.crypt_key_valid.read().await;
            valid.unwrap_or(false)
        };

        match crypt_test_result {
            Ok(ref response) if crypt_valid && !CommandEngine::is_error_code(response) => {
                debug!("Crypt test passed");
            }
            _ => {
                if config.discover_code {
                    warn!("Bad panel ID: {}. Attempting discovery...", config.panel_id);
                    // Run panel ID discovery
                    match discovery::discover_panel_id(&command_engine).await {
                        Ok(discovered_id) => {
                            info!("Discovered panel ID: {}", discovered_id);
                        }
                        Err(e) => {
                            error!("Panel ID discovery failed: {}", e);
                            return Err(e);
                        }
                    }
                } else {
                    return Err(RiscoError::BadCryptKey {
                        panel_id: config.panel_id,
                    });
                }
            }
        }
        command_engine.set_in_crypt_test(false).await;

        info!("Connection to panel successfully established");
        let _ = event_tx.send(PanelEvent::Connected);

        Ok(transport)
    }

    /// Send a command through the transport.
    pub async fn send_command(&self, command: &str, is_prog_cmd: bool) -> Result<String> {
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
fn spawn_reader_task(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    engine: Arc<CommandEngine>,
    event_tx: EventSender,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        let mut leftover = Vec::new();

        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    // Connection closed
                    debug!("Reader: connection closed");
                    engine.set_connected(false).await;
                    let _ = event_tx.send(PanelEvent::Disconnected);
                    break;
                }
                Ok(n) => {
                    let mut data = leftover.clone();
                    data.extend_from_slice(&buf[..n]);
                    leftover.clear();

                    // Split on ETX+STX boundary (multiple messages in one read)
                    let messages = split_messages(&data, &mut leftover);

                    for msg in messages {
                        process_message(&msg, &engine, &event_tx).await;
                    }
                }
                Err(e) => {
                    error!("Reader: read error: {}", e);
                    engine.set_connected(false).await;
                    let _ = event_tx.send(PanelEvent::Disconnected);
                    break;
                }
            }
        }
    })
}

/// Split a data buffer into individual messages on ETX+STX boundaries.
fn split_messages(data: &[u8], leftover: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let mut start = 0;

    for i in 0..data.len() {
        if data[i] == ETX {
            // Check if this ETX is followed by STX (another message)
            let msg = &data[start..=i];
            messages.push(msg.to_vec());
            start = i + 1;
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
            // Unsolicited message from panel — send ACK
            debug!("Unsolicited data from panel (id={}), sending ACK", id_num);
            let _ = engine.send_ack(receive_id).await;
            // Also emit the data for processing
            emit_panel_data(event_tx, &decoded.command);
        } else {
            // Response to our command — route to pending sender
            let pending_arc = engine.pending_commands();
            let mut pending = pending_arc.lock().await;
            let seq_arc = engine.sequence_id();
            let current_seq = *seq_arc.lock().await;
            if id_num == current_seq {
                if let Some(sender) = pending.remove(&id_num) {
                    let _ = sender.send(decoded.command.clone());
                    debug!("Routed response for seq {}", id_num);
                }
            }
        }
    }

    // Whether expected or not, emit for analysis
    emit_panel_data(event_tx, &decoded.command);
}

/// Emit panel data through the event system for the comm layer to process.
fn emit_panel_data(_event_tx: &EventSender, _command: &str) {
    // The comm layer handles routing of ZSTT, PSTT, OSTT, SSTT messages.
    // This is done at the comm.rs level via subscribe() rather than
    // through the event bus, to avoid circular dependency.
    // Panel data is routed through the command response mechanism.
}
