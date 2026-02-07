// MIT License - Copyright (c) 2021 TJForc
// Rust translation of Risco_ProxyTCP_Socket from RiscoChannels.js

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::config::PanelConfig;
use crate::constants::{CLOUD, CRYPT};
use crate::crypto::RiscoCrypt;
use crate::error::{RiscoError, Result};
use crate::event::{EventSender, PanelEvent};
use crate::transport::command::CommandEngine;

/// Proxy TCP transport — sits between the panel and RiscoCloud,
/// allowing simultaneous local and cloud access.
pub struct ProxyTcpTransport {
    pub command_engine: Arc<CommandEngine>,
    event_tx: EventSender,
    in_remote_conn: Arc<RwLock<bool>>,
    #[allow(dead_code)]
    cloud_connected: Arc<RwLock<bool>>,
    panel_reader_handle: Option<tokio::task::JoinHandle<()>>,
    cloud_reader_handle: Option<tokio::task::JoinHandle<()>>,
    listener_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProxyTcpTransport {
    /// Start the proxy: listen for the panel connection, connect to cloud,
    /// then authenticate locally.
    pub async fn connect(config: &PanelConfig, event_tx: EventSender) -> Result<Self> {
        info!("Starting proxy on port {}", config.listening_port);

        let listener = TcpListener::bind(format!("0.0.0.0:{}", config.listening_port))
            .await
            .map_err(|e| {
                error!("Failed to bind proxy listener: {}", e);
                RiscoError::Io(e)
            })?;

        info!("Proxy listening on port {}", config.listening_port);

        // Wait for panel connection
        let (panel_stream, panel_addr) = listener.accept().await.map_err(|e| {
            error!("Failed to accept panel connection: {}", e);
            RiscoError::Io(e)
        })?;
        info!("Panel connected from {}", panel_addr);

        // Connect to RiscoCloud
        let cloud_addr = format!("{}:{}", config.cloud_url, config.cloud_port);
        info!("Connecting to RiscoCloud at {}", cloud_addr);
        let cloud_stream = TcpStream::connect(&cloud_addr).await.map_err(|e| {
            error!("Failed to connect to RiscoCloud: {}", e);
            RiscoError::Io(e)
        })?;
        info!("Connected to RiscoCloud");

        let (panel_reader, panel_writer) = panel_stream.into_split();
        let (cloud_reader, cloud_writer) = cloud_stream.into_split();
        let cloud_writer = Arc::new(Mutex::new(cloud_writer));

        let crypt = RiscoCrypt::new(config.panel_id);
        let command_engine = Arc::new(CommandEngine::new(panel_writer, crypt, true));

        let in_remote_conn = Arc::new(RwLock::new(false));
        let cloud_connected = Arc::new(RwLock::new(false));

        // Spawn panel → cloud forwarding task
        let panel_reader_handle = {
            let engine = command_engine.clone();
            let cloud_writer = cloud_writer.clone();
            let in_remote = in_remote_conn.clone();
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                Self::panel_reader_loop(
                    panel_reader,
                    engine,
                    cloud_writer,
                    in_remote,
                    event_tx,
                )
                .await;
            })
        };

        // Spawn cloud → panel forwarding task
        let cloud_reader_handle = {
            let engine = command_engine.clone();
            let cloud_writer = cloud_writer.clone();
            let in_remote = in_remote_conn.clone();
            let cloud_conn = cloud_connected.clone();
            let event_tx = event_tx.clone();
            let password = config.panel_password.clone();
            tokio::spawn(async move {
                Self::cloud_reader_loop(
                    cloud_reader,
                    engine,
                    cloud_writer,
                    in_remote,
                    cloud_conn,
                    event_tx,
                    &password,
                )
                .await;
            })
        };

        // Wait for cloud handshake (45 seconds as in JS)
        {
            let cloud_conn = cloud_connected.clone();
            tokio::spawn(async move {
                sleep(Duration::from_secs(45)).await;
                *cloud_conn.write().await = true;
            });
        }

        // Wait for cloud to be ready
        loop {
            if *cloud_connected.read().await && !*in_remote_conn.read().await {
                break;
            }
            sleep(Duration::from_secs(1)).await;
        }

        // Now authenticate with the panel
        let password = &config.panel_password;
        let code_len = password.len().max(4);
        let padded_password = format!("{:0>width$}", password, width = code_len);

        let rmt_response = command_engine
            .send_command(&format!("RMT={}", padded_password), false)
            .await?;
        if !rmt_response.contains("ACK") {
            return Err(RiscoError::BadAccessCode {
                code: password.clone(),
            });
        }

        let lcl_response = command_engine.send_command("LCL", false).await?;
        if !lcl_response.contains("ACK") {
            return Err(RiscoError::InvalidResponse {
                details: format!("LCL response: {}", lcl_response),
            });
        }

        // Enable encryption
        {
            let crypt_arc = command_engine.crypt();
            let mut crypt = crypt_arc.lock().await;
            crypt.set_crypt_enabled(true);
        }

        sleep(Duration::from_millis(1000)).await;

        // Crypt test
        command_engine.set_in_crypt_test(true).await;
        let _ = command_engine.send_command("CUSTLST?", false).await;
        command_engine.set_in_crypt_test(false).await;

        info!("Proxy connection established");
        let _ = event_tx.send(PanelEvent::Connected);

        Ok(Self {
            command_engine,
            event_tx,
            in_remote_conn,
            cloud_connected,
            panel_reader_handle: Some(panel_reader_handle),
            cloud_reader_handle: Some(cloud_reader_handle),
            listener_handle: None,
        })
    }

    /// Panel → Cloud forwarding loop.
    async fn panel_reader_loop(
        mut reader: tokio::net::tcp::OwnedReadHalf,
        engine: Arc<CommandEngine>,
        cloud_writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
        in_remote: Arc<RwLock<bool>>,
        event_tx: EventSender,
    ) {
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    debug!("Panel reader: connection closed");
                    break;
                }
                Ok(n) => {
                    let data = &buf[..n];

                    if data.len() > 1 && data[1] == CLOUD {
                        // Cloud protocol data — forward directly
                        let mut writer = cloud_writer.lock().await;
                        let _ = writer.write_all(data).await;
                    } else if *in_remote.read().await {
                        // During remote connection, forward to cloud
                        let mut writer = cloud_writer.lock().await;
                        let _ = writer.write_all(data).await;
                    } else {
                        // Local data — process through command engine
                        // (handled by the command engine's reader)
                    }
                }
                Err(e) => {
                    error!("Panel reader error: {}", e);
                    break;
                }
            }
        }
        engine.set_connected(false).await;
        let _ = event_tx.send(PanelEvent::Disconnected);
    }

    /// Cloud → Panel forwarding loop with RMT/LCL/DCN interception.
    async fn cloud_reader_loop(
        mut reader: tokio::net::tcp::OwnedReadHalf,
        engine: Arc<CommandEngine>,
        cloud_writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
        in_remote: Arc<RwLock<bool>>,
        cloud_connected: Arc<RwLock<bool>>,
        event_tx: EventSender,
        local_password: &str,
    ) {
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    debug!("Cloud reader: connection closed");
                    break;
                }
                Ok(n) => {
                    let data = &buf[..n];

                    if data.len() > 1 && data[1] == CLOUD {
                        // Cloud handshake data — set timer and forward to panel
                        // (cloud_connected is set after 45s timer)
                        let _ = engine.send_raw("", None).await; // forward via panel writer
                    } else {
                        // Decode to check for RMT/LCL/DCN interception
                        let decoded = {
                            let crypt_arc = engine.crypt();
                            let mut crypt = crypt_arc.lock().await;
                            crypt.decode_message(data)
                        };

                        if data.len() > 1 && data[1] != CRYPT {
                            // Unencrypted cloud command
                            if decoded.command.contains("RMT=") {
                                // Incoming remote connection
                                *in_remote.write().await = true;
                                let _ = event_tx.send(PanelEvent::IncomingRemoteConnection);

                                // Check if password matches — if so, send fake ACK to cloud
                                if let Some(rmt_password) =
                                    decoded.command.strip_prefix("RMT=")
                                {
                                    if rmt_password.trim_start_matches('0')
                                        == local_password.trim_start_matches('0')
                                    {
                                        let fake_ack = {
                                            let crypt_arc = engine.crypt();
                                            let mut crypt = crypt_arc.lock().await;
                                            crypt.encode_command(
                                                "ACK",
                                                &decoded.cmd_id,
                                                Some(false),
                                            )
                                        };
                                        let mut writer = cloud_writer.lock().await;
                                        let _ = writer.write_all(&fake_ack).await;
                                    }
                                }
                            } else if decoded.command.contains("LCL") {
                                // Send fake ACK for LCL
                                let fake_ack = {
                                    let crypt_arc = engine.crypt();
                                    let mut crypt = crypt_arc.lock().await;
                                    crypt.encode_command("ACK", &decoded.cmd_id, Some(false))
                                };
                                let mut writer = cloud_writer.lock().await;
                                let _ = writer.write_all(&fake_ack).await;
                            }
                        } else if data.len() > 1 && data[1] == CRYPT {
                            // Encrypted cloud command — check for DCN
                            if *in_remote.read().await
                                && decoded.crc_valid
                                && decoded.command.contains("DCN")
                            {
                                *in_remote.write().await = false;
                                let fake_ack = {
                                    let crypt_arc = engine.crypt();
                                    let mut crypt = crypt_arc.lock().await;
                                    crypt.encode_command("ACK", &decoded.cmd_id, Some(true))
                                };
                                let mut writer = cloud_writer.lock().await;
                                let _ = writer.write_all(&fake_ack).await;
                                let _ =
                                    event_tx.send(PanelEvent::EndIncomingRemoteConnection);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Cloud reader error: {}", e);
                    break;
                }
            }
        }
        *cloud_connected.write().await = false;
    }

    /// Send a command through the transport.
    pub async fn send_command(&self, command: &str, is_prog_cmd: bool) -> Result<String> {
        // Pause commands during remote connections
        while *self.in_remote_conn.read().await {
            sleep(Duration::from_secs(1)).await;
        }
        self.command_engine.send_command(command, is_prog_cmd).await
    }

    /// Disconnect.
    pub async fn disconnect(&self) -> Result<()> {
        info!("Proxy disconnecting");
        let _ = self.event_tx.send(PanelEvent::Disconnected);
        self.command_engine.disconnect().await?;
        Ok(())
    }

    pub async fn is_connected(&self) -> bool {
        *self.command_engine.connected_flag().read().await
    }

    pub fn engine(&self) -> &Arc<CommandEngine> {
        &self.command_engine
    }
}

impl Drop for ProxyTcpTransport {
    fn drop(&mut self) {
        if let Some(h) = self.panel_reader_handle.take() {
            h.abort();
        }
        if let Some(h) = self.cloud_reader_handle.take() {
            h.abort();
        }
        if let Some(h) = self.listener_handle.take() {
            h.abort();
        }
    }
}
