// MIT License - Copyright (c) 2021 TJForc
// Rust translation of command/response engine from RiscoChannels.js

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, warn};

use crate::crypto::RiscoCrypt;
use crate::error::{PanelErrorCode, RiscoError, Result};

/// Tracks pending commands and routes responses back to callers via oneshot channels.
pub struct CommandEngine {
    /// Sequence ID cycles 1-49
    sequence_id: Arc<Mutex<u8>>,
    /// Map of pending sequence IDs to their response senders
    pending: Arc<Mutex<HashMap<u8, oneshot::Sender<String>>>>,
    /// TCP writer half
    writer: Arc<Mutex<OwnedWriteHalf>>,
    /// Crypto engine
    crypt: Arc<Mutex<RiscoCrypt>>,
    /// Whether we're in programming mode
    in_prog: Arc<RwLock<bool>>,
    /// Whether we're in crypt test mode
    in_crypt_test: Arc<RwLock<bool>>,
    /// Whether we're discovering codes
    discovering: Arc<RwLock<bool>>,
    /// Bad CRC counter
    bad_crc_count: Arc<Mutex<u32>>,
    /// Time of last CRC error (for 60s reset window)
    last_crc_error: Arc<Mutex<Option<Instant>>>,
    /// Whether transport is connected
    connected: Arc<RwLock<bool>>,
    /// Socket mode (for timeout selection)
    is_proxy: bool,
    /// Last misunderstood data from bad CRC
    last_misunderstood: Arc<Mutex<Option<String>>>,
    /// Crypt key validity flag for discovery
    pub crypt_key_valid: Arc<RwLock<Option<bool>>>,
}

const BAD_CRC_LIMIT: u32 = 10;
const CRC_RESET_WINDOW: Duration = Duration::from_secs(60);

impl CommandEngine {
    pub fn new(
        writer: OwnedWriteHalf,
        crypt: RiscoCrypt,
        is_proxy: bool,
    ) -> Self {
        Self {
            sequence_id: Arc::new(Mutex::new(1)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            writer: Arc::new(Mutex::new(writer)),
            crypt: Arc::new(Mutex::new(crypt)),
            in_prog: Arc::new(RwLock::new(false)),
            in_crypt_test: Arc::new(RwLock::new(false)),
            discovering: Arc::new(RwLock::new(false)),
            bad_crc_count: Arc::new(Mutex::new(0)),
            last_crc_error: Arc::new(Mutex::new(None)),
            connected: Arc::new(RwLock::new(true)),
            is_proxy,
            last_misunderstood: Arc::new(Mutex::new(None)),
            crypt_key_valid: Arc::new(RwLock::new(None)),
        }
    }

    /// Get shared references to internal state for the reader task.
    pub fn pending_commands(&self) -> Arc<Mutex<HashMap<u8, oneshot::Sender<String>>>> {
        self.pending.clone()
    }

    pub fn crypt(&self) -> Arc<Mutex<RiscoCrypt>> {
        self.crypt.clone()
    }

    pub fn connected_flag(&self) -> Arc<RwLock<bool>> {
        self.connected.clone()
    }

    pub fn in_prog_flag(&self) -> Arc<RwLock<bool>> {
        self.in_prog.clone()
    }

    pub fn in_crypt_test_flag(&self) -> Arc<RwLock<bool>> {
        self.in_crypt_test.clone()
    }

    pub fn bad_crc_count(&self) -> Arc<Mutex<u32>> {
        self.bad_crc_count.clone()
    }

    pub fn last_crc_error(&self) -> Arc<Mutex<Option<Instant>>> {
        self.last_crc_error.clone()
    }

    pub fn last_misunderstood(&self) -> Arc<Mutex<Option<String>>> {
        self.last_misunderstood.clone()
    }

    pub fn sequence_id(&self) -> Arc<Mutex<u8>> {
        self.sequence_id.clone()
    }

    /// Set connected state.
    pub async fn set_connected(&self, connected: bool) {
        *self.connected.write().await = connected;
    }

    /// Set programming mode state.
    pub async fn set_in_prog(&self, in_prog: bool) {
        *self.in_prog.write().await = in_prog;
    }

    /// Set crypt test state.
    pub async fn set_in_crypt_test(&self, in_test: bool) {
        *self.in_crypt_test.write().await = in_test;
    }

    /// Set discovering state.
    pub async fn set_discovering(&self, discovering: bool) {
        *self.discovering.write().await = discovering;
    }

    /// Send a command and wait for a response.
    pub async fn send_command(&self, command_str: &str, is_prog_cmd: bool) -> Result<String> {
        // Wait while in prog mode and this is not a prog command
        while *self.in_prog.read().await && !is_prog_cmd {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        if !*self.connected.read().await {
            return Err(RiscoError::Disconnected);
        }

        // Determine timeout
        let timeout_duration = self.get_timeout().await;

        debug!("Sending command: {}", command_str);

        let (tx, rx) = oneshot::channel();
        let cmd_id = {
            let seq = self.sequence_id.lock().await;
            let id = *seq;
            // Register pending command
            self.pending.lock().await.insert(id, tx);
            id
        };
        let cmd_id_str = format!("{:02}", cmd_id);

        // Encrypt and send
        let encoded = {
            let mut crypt = self.crypt.lock().await;
            crypt.encode_command(command_str, &cmd_id_str, None)
        };

        {
            let mut writer = self.writer.lock().await;
            writer.write_all(&encoded).await.map_err(|e| {
                error!("Failed to write command: {}", e);
                RiscoError::Io(e)
            })?;
        }

        debug!("Command sent (seq {})", cmd_id);

        // Wait for response with timeout
        match timeout(timeout_duration, rx).await {
            Ok(Ok(response)) => {
                debug!("Received response for seq {}: {}", cmd_id, response);
                // Increment sequence ID
                self.increment_sequence_id().await;
                Ok(response)
            }
            Ok(Err(_)) => {
                // Channel closed
                self.pending.lock().await.remove(&cmd_id);
                Err(RiscoError::ChannelClosed)
            }
            Err(_) => {
                // Timeout
                self.pending.lock().await.remove(&cmd_id);
                debug!("Command timeout: {} {}", cmd_id_str, command_str);
                Err(RiscoError::CommandTimeout {
                    command: command_str.to_string(),
                })
            }
        }
    }

    /// Send an ACK for an unsolicited panel message.
    pub async fn send_ack(&self, id_str: &str) -> Result<()> {
        debug!("Sending ACK for id {}", id_str);
        let encoded = {
            let mut crypt = self.crypt.lock().await;
            crypt.encode_command("ACK", id_str, None)
        };

        let mut writer = self.writer.lock().await;
        writer.write_all(&encoded).await.map_err(|e| {
            error!("Failed to send ACK: {}", e);
            RiscoError::Io(e)
        })?;
        Ok(())
    }

    /// Send a raw command without waiting for response (e.g., DCN on disconnect).
    pub async fn send_raw(&self, command_str: &str) -> Result<()> {
        let cmd_id_str = {
            let seq = self.sequence_id.lock().await;
            format!("{:02}", *seq)
        };
        let encoded = {
            let mut crypt = self.crypt.lock().await;
            crypt.encode_command(command_str, &cmd_id_str, None)
        };

        let mut writer = self.writer.lock().await;
        writer.write_all(&encoded).await.map_err(RiscoError::Io)?;
        Ok(())
    }

    /// Increment sequence ID (wraps 1-49).
    async fn increment_sequence_id(&self) {
        let mut seq = self.sequence_id.lock().await;
        if *seq >= 49 {
            *seq = 0;
        }
        *seq += 1;
    }

    /// Get the appropriate timeout duration based on current mode.
    async fn get_timeout(&self) -> Duration {
        if *self.in_crypt_test.read().await {
            if self.is_proxy {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(500)
            }
        } else if *self.discovering.read().await && self.is_proxy {
            Duration::from_millis(100)
        } else if *self.in_prog.read().await {
            Duration::from_secs(29)
        } else {
            Duration::from_secs(5)
        }
    }

    /// Handle a bad CRC response. Returns true if the CRC limit has been exceeded.
    pub async fn handle_bad_crc(&self) -> bool {
        let mut count = self.bad_crc_count.lock().await;

        // Check if we should reset the counter (60s window)
        let mut last_error = self.last_crc_error.lock().await;
        if let Some(last) = *last_error {
            if last.elapsed() > CRC_RESET_WINDOW {
                *count = 0;
            }
        }

        *count += 1;
        *last_error = Some(Instant::now());

        if *count > BAD_CRC_LIMIT {
            error!("Too many bad CRC values ({})", *count);
            true
        } else {
            warn!("Bad CRC value (count: {})", *count);
            false
        }
    }

    /// Check if a response string is a known panel error code.
    pub fn is_error_code(data: &str) -> bool {
        PanelErrorCode::is_error_code(data)
    }

    /// Disconnect: mark as disconnected and attempt DCN command.
    pub async fn disconnect(&self) -> Result<()> {
        self.set_connected(false).await;
        // Best-effort DCN
        let _ = self.send_raw("DCN").await;
        Ok(())
    }
}
