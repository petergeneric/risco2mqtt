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
use crate::protocol::Command;

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
    /// Last misunderstood data from bad CRC
    last_misunderstood: Arc<Mutex<Option<String>>>,
    /// Raw bytes of the last received message (before decryption)
    last_received_buffer: Arc<Mutex<Option<Vec<u8>>>>,
    /// Crypt key validity flag for discovery
    pub crypt_key_valid: Arc<RwLock<Option<bool>>>,
    /// Last received unsolicited message ID (for dedup)
    last_received_unsolicited_id: Arc<Mutex<Option<u8>>>,
}

const BAD_CRC_LIMIT: u32 = 10;
const CRC_RESET_WINDOW: Duration = Duration::from_secs(60);

impl CommandEngine {
    pub fn new(
        writer: OwnedWriteHalf,
        crypt: RiscoCrypt,
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
            last_misunderstood: Arc::new(Mutex::new(None)),
            last_received_buffer: Arc::new(Mutex::new(None)),
            crypt_key_valid: Arc::new(RwLock::new(None)),
            last_received_unsolicited_id: Arc::new(Mutex::new(None)),
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

    pub fn last_received_buffer(&self) -> Arc<Mutex<Option<Vec<u8>>>> {
        self.last_received_buffer.clone()
    }

    pub fn last_received_unsolicited_id(&self) -> Arc<Mutex<Option<u8>>> {
        self.last_received_unsolicited_id.clone()
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
    ///
    /// Retries up to 2 times on timeout with a fresh sequence ID.
    pub async fn send_command(&self, command: &Command, is_prog_cmd: bool) -> Result<String> {
        let command_str = command.to_wire_string();

        // Wait while in prog mode and this is not a prog command
        while *self.in_prog.read().await && !is_prog_cmd {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        let max_attempts = 3u8;

        for attempt in 1..=max_attempts {
            if !*self.connected.read().await {
                return Err(RiscoError::Disconnected);
            }

            let timeout_duration = self.get_timeout().await;

            if attempt > 1 {
                debug!("Retrying command (attempt {}): {}", attempt, command_str);
            } else {
                debug!("Sending command: {}", command_str);
            }

            let (tx, rx) = oneshot::channel();
            let cmd_id = {
                let seq = self.sequence_id.lock().await;
                let id = *seq;
                self.pending.lock().await.insert(id, tx);
                id
            };
            let cmd_id_str = format!("{:02}", cmd_id);

            // Encrypt and send
            let encoded = {
                let mut crypt = self.crypt.lock().await;
                crypt.encode_command(&command_str, &cmd_id_str, None)
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
                    self.increment_sequence_id().await;
                    return Ok(response);
                }
                Ok(Err(_)) => {
                    // Channel closed — no point retrying
                    self.pending.lock().await.remove(&cmd_id);
                    return Err(RiscoError::ChannelClosed);
                }
                Err(_) => {
                    // Timeout — clean up and maybe retry
                    self.pending.lock().await.remove(&cmd_id);
                    self.increment_sequence_id().await;
                    if attempt == max_attempts {
                        debug!("Command timeout after {} attempts: {} {}", max_attempts, cmd_id_str, command_str);
                        return Err(RiscoError::CommandTimeout {
                            command: command_str.clone(),
                        });
                    }
                    warn!("Command timeout (attempt {}/{}): {}", attempt, max_attempts, command_str);
                }
            }
        }

        // Should not reach here, but just in case
        Err(RiscoError::CommandTimeout {
            command: command_str,
        })
    }

    /// Send an ACK for an unsolicited panel message.
    pub async fn send_ack(&self, id_str: &str) -> Result<()> {
        debug!("Sending ACK for id {}", id_str);
        let ack_str = Command::Ack.to_wire_string();
        let encoded = {
            let mut crypt = self.crypt.lock().await;
            crypt.encode_command(&ack_str, id_str, None)
        };

        let mut writer = self.writer.lock().await;
        writer.write_all(&encoded).await.map_err(|e| {
            error!("Failed to send ACK: {}", e);
            RiscoError::Io(e)
        })?;
        Ok(())
    }

    /// Send a raw command without waiting for response (e.g., DCN on disconnect).
    ///
    /// `force_crypt` overrides the current encryption state when encoding:
    /// - `Some(false)` forces unencrypted (used for DCN when crypt key may be wrong)
    /// - `Some(true)` forces encrypted
    /// - `None` uses the current crypt state
    pub async fn send_raw(&self, command: &Command, force_crypt: Option<bool>) -> Result<()> {
        if !*self.connected.read().await {
            return Err(RiscoError::Disconnected);
        }

        let command_str = command.to_wire_string();
        let cmd_id_str = {
            let seq = self.sequence_id.lock().await;
            format!("{:02}", *seq)
        };
        let encoded = {
            let mut crypt = self.crypt.lock().await;
            crypt.encode_command(&command_str, &cmd_id_str, force_crypt)
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
            Duration::from_millis(500)
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
        if let Some(last) = *last_error
            && last.elapsed() > CRC_RESET_WINDOW
        {
            *count = 0;
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

    /// Disconnect: send unencrypted DCN then mark as disconnected.
    ///
    /// DCN is sent fully unencrypted so the panel can understand it even if the
    /// encryption key was wrong or partially established. We disable the XOR
    /// cipher before sending, since `force_crypt` only controls the CRYPT
    /// indicator byte — not the actual XOR cipher which is governed by
    /// `crypt_enabled`. Since we're disconnecting, we don't need to restore it.
    /// The DCN is sent before setting `connected = false` so that the
    /// disconnected guard in `send_raw` doesn't block it.
    pub async fn disconnect(&self) -> Result<()> {
        // Send DCN fully unencrypted so the panel can understand it even
        // if the encryption key was wrong or partially established.
        {
            let mut crypt = self.crypt.lock().await;
            crypt.set_crypt_enabled(false);
        }
        let _ = self.send_raw(&Command::Dcn, Some(false)).await;
        self.set_connected(false).await;
        Ok(())
    }
}
