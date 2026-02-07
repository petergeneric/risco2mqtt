// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/RiscoComm.js

use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

use crate::config::{panel_limits, PanelConfig, PanelType, SocketMode};
use crate::constants::{PanelHwType, timezone_index_for_offset};
use crate::devices::output::{Output, OutputType};
use crate::devices::partition::Partition;
use crate::devices::system::MBSystem;
use crate::devices::zone::{Zone, ZoneTechnology};
use crate::error::{PanelErrorCode, RiscoError, Result};
use crate::event::EventSender;
use crate::protocol::{is_ack, parse_tab_separated_labels, parse_tab_separated_trimmed, parse_value_after_eq};
use crate::transport::command::CommandEngine;
use crate::transport::direct::DirectTcpTransport;
use crate::transport::proxy::ProxyTcpTransport;

/// High-level communication handler.
///
/// Manages the connection lifecycle, panel verification, device discovery,
/// and real-time status updates.
pub struct RiscoComm {
    config: PanelConfig,
    direct_transport: Option<DirectTcpTransport>,
    proxy_transport: Option<ProxyTcpTransport>,
    event_tx: EventSender,
    firmware_version: Option<String>,
    max_zones: u32,
    max_parts: u32,
    max_outputs: u32,
}

impl RiscoComm {
    pub fn new(config: PanelConfig, event_tx: EventSender) -> Self {
        let (mz, mp, mo) = panel_limits(config.panel_type, None);
        Self {
            config,
            direct_transport: None,
            proxy_transport: None,
            event_tx,
            firmware_version: None,
            max_zones: mz,
            max_parts: mp,
            max_outputs: mo,
        }
    }

    /// Initialize the connection to the panel.
    pub async fn connect(&mut self) -> Result<()> {
        info!("Starting connection to panel");

        match self.config.socket_mode {
            SocketMode::Direct => {
                let transport =
                    DirectTcpTransport::connect(&self.config, self.event_tx.clone()).await?;
                self.direct_transport = Some(transport);
            }
            SocketMode::Proxy => {
                let transport =
                    ProxyTcpTransport::connect(&self.config, self.event_tx.clone()).await?;
                self.proxy_transport = Some(transport);
            }
        }

        // Panel connected — verify type and configure
        self.verify_panel_type().await?;
        self.get_firmware_version().await?;

        // Update limits based on firmware
        let (mz, mp, mo) =
            panel_limits(self.config.panel_type, self.firmware_version.as_deref());
        self.max_zones = mz;
        self.max_parts = mp;
        self.max_outputs = mo;

        // Verify/modify panel configuration
        let commands = self.verify_panel_configuration().await?;
        if !commands.is_empty() {
            self.modify_panel_config(&commands).await?;
        }

        // Communication is ready
        info!("Panel communication ready");

        Ok(())
    }

    /// Send a command through whichever transport is active.
    pub async fn send_command(&self, command: &str, is_prog_cmd: bool) -> Result<String> {
        if let Some(ref transport) = self.direct_transport {
            transport.send_command(command, is_prog_cmd).await
        } else if let Some(ref transport) = self.proxy_transport {
            transport.send_command(command, is_prog_cmd).await
        } else {
            Err(RiscoError::Disconnected)
        }
    }

    /// Get the command engine for direct operations.
    fn engine(&self) -> Result<&Arc<CommandEngine>> {
        if let Some(ref t) = self.direct_transport {
            Ok(t.engine())
        } else if let Some(ref t) = self.proxy_transport {
            Ok(t.engine())
        } else {
            Err(RiscoError::Disconnected)
        }
    }

    /// Verify and auto-discover the connected panel type.
    ///
    /// Queries the panel's hardware type via PNLCNF. If the detected type
    /// differs from the configured one, a warning is logged and the config
    /// is updated to match the actual panel. This prevents disconnection
    /// when the user misconfigures (or omits) the panel type.
    async fn verify_panel_type(&mut self) -> Result<()> {
        let response = self.send_command("PNLCNF", false).await?;
        let panel_type_str = parse_value_after_eq(&response);
        debug!("Connected panel type: {}", panel_type_str);

        match PanelHwType::from_name(panel_type_str) {
            Some(hw) => {
                let detected_type = PanelType::from_hw_type(hw);
                let expected_hw = self.config.panel_type.hardware_type();
                if hw != expected_hw {
                    warn!(
                        "Panel type mismatch: configured {} but detected {}. Using detected type.",
                        expected_hw.as_str(),
                        hw.as_str()
                    );
                    self.config.panel_type = detected_type;
                } else {
                    debug!("Panel type matches: {}", hw.as_str());
                }
                Ok(())
            }
            None => {
                warn!(
                    "Unknown panel hardware type: {}. Keeping configured type.",
                    panel_type_str
                );
                Ok(())
            }
        }
    }

    /// Get the firmware version (only for LightSys and ProsysPlus/GTPlus).
    async fn get_firmware_version(&mut self) -> Result<()> {
        match self.config.panel_type {
            PanelType::LightSys | PanelType::ProsysPlus | PanelType::GTPlus => {
                match self.send_command("FSVER?", false).await {
                    Ok(response) => {
                        let version_full = parse_value_after_eq(&response);
                        // Extract version before space (e.g., "3.0 build 123" → "3.0")
                        let version = version_full
                            .split_whitespace()
                            .next()
                            .unwrap_or(version_full);
                        self.firmware_version = Some(version.to_string());
                        debug!("Panel firmware version: {}", version);
                    }
                    Err(e) => {
                        warn!("Cannot retrieve firmware version: {}", e);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Check panel configuration and return commands needed to fix it.
    async fn verify_panel_configuration(&self) -> Result<Vec<String>> {
        let mut commands = Vec::new();
        debug!("Checking panel configuration");

        if self.config.disable_risco_cloud && !self.config.enable_risco_cloud {
            // Check RiscoCloud status
            let response = self.send_command("ELASEN?", false).await?;
            let cloud_enabled = parse_value_after_eq(&response).trim() == "1";

            if cloud_enabled {
                commands.push("ELASEN=0".to_string());
                debug!("Prepare panel for disabling RiscoCloud");
            }

            // Check timezone
            let tz_response = self.send_command("TIMEZONE?", false).await?;
            let panel_tz_idx: usize = parse_value_after_eq(&tz_response)
                .trim()
                .parse()
                .unwrap_or(14);

            // Get local timezone offset
            let local_tz = local_gmt_offset();
            if let Some(idx) = timezone_index_for_offset(&local_tz) {
                if panel_tz_idx != idx as usize {
                    commands.push(format!("TIMEZONE={}", idx));
                    debug!("Prepare panel for updating timezone");
                }
            }

            // Check NTP server
            let ntp_response = self.send_command("INTP?", false).await?;
            let panel_ntp = parse_value_after_eq(&ntp_response).trim().to_string();
            if panel_ntp != self.config.ntp_server {
                commands.push(format!("INTP={}", self.config.ntp_server));
                debug!("Prepare panel for updating NTP server");
            }

            // Check NTP port
            let ntp_port_response = self.send_command("INTPP?", false).await?;
            let panel_ntp_port = parse_value_after_eq(&ntp_port_response).trim().to_string();
            if panel_ntp_port != self.config.ntp_port {
                commands.push(format!("INTPP={}", self.config.ntp_port));
                debug!("Prepare panel for updating NTP port");
            }

            // Check NTP protocol
            let ntp_proto_response = self.send_command("INTPPROT?", false).await?;
            let panel_ntp_proto = parse_value_after_eq(&ntp_proto_response).trim().to_string();
            if panel_ntp_proto != "1" {
                commands.push("INTPPROT=1".to_string());
                debug!("Prepare panel for enabling NTP");
            }
        } else if self.config.enable_risco_cloud && !self.config.disable_risco_cloud {
            let response = self.send_command("ELASEN?", false).await?;
            let cloud_enabled = parse_value_after_eq(&response).trim() == "1";
            if !cloud_enabled {
                commands.push("ELASEN=1".to_string());
                debug!("Enabling RiscoCloud");
            }
        }

        Ok(commands)
    }

    /// Enter programming mode, execute commands, exit programming mode.
    async fn modify_panel_config(&self, commands: &[String]) -> Result<()> {
        debug!("Modifying panel configuration ({} commands)", commands.len());

        // Enter prog mode
        let response = self.send_command("PROG=1", true).await?;
        if !is_ack(&response) {
            warn!("Cannot enter programming mode");
            return Ok(());
        }

        if let Ok(engine) = self.engine() {
            engine.set_in_prog(true).await;
        }
        debug!("Entered programming mode");

        // Execute each command
        for cmd in commands {
            match self.send_command(cmd, true).await {
                Ok(resp) if is_ack(&resp) => {
                    debug!("Config command successful: {}", cmd);
                }
                Ok(resp) => {
                    warn!("Config command failed: {} -> {}", cmd, resp);
                }
                Err(e) => {
                    warn!("Config command error: {} -> {}", cmd, e);
                }
            }
        }

        // Exit prog mode
        let response = self.send_command("PROG=2", true).await?;
        if is_ack(&response) {
            debug!("Exited programming mode");
        }

        // Wait for prog mode to actually end
        // (system status update will clear InProg)
        sleep(Duration::from_secs(5)).await;

        if let Ok(engine) = self.engine() {
            engine.set_in_prog(false).await;
        }

        Ok(())
    }

    /// Query all zone data from the panel.
    ///
    /// Queries zones in batches of 8. If the panel returns an N19 error
    /// ("Device Doesn't Exist") for a batch, discovery stops early and
    /// only the zones discovered so far are returned. This handles panels
    /// where `max_zones` exceeds the actual physical zone count.
    pub async fn get_all_zones(&self) -> Result<Vec<Zone>> {
        debug!("Retrieving zone configuration");
        let mut zones: Vec<Zone> = (1..=self.max_zones).map(Zone::new).collect();
        let mut actual_count = self.max_zones as usize;

        'batch: for batch_start in (0..self.max_zones).step_by(8) {
            let min = batch_start + 1;
            let max = (batch_start + 8).min(self.max_zones);

            let ztypes = match self.send_command(&format!("ZTYPE*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Zone slot {} doesn't exist, stopping zone discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let ztypes = parse_tab_separated_trimmed(&ztypes);

            let zparts = match self.send_command(&format!("ZPART&*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Zone slot {} doesn't exist, stopping zone discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let zparts = parse_tab_separated_trimmed(&zparts);

            let zareas = match self.send_command(&format!("ZAREA&*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Zone slot {} doesn't exist, stopping zone discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let zareas = parse_tab_separated_trimmed(&zareas);

            let zlabels = match self.send_command(&format!("ZLBL*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Zone slot {} doesn't exist, stopping zone discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let zlabels = parse_tab_separated_labels(&zlabels);

            let zstatus = match self.send_command(&format!("ZSTT*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Zone slot {} doesn't exist, stopping zone discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let zstatus = parse_tab_separated_trimmed(&zstatus);

            // Zone link types are queried individually
            let mut ztechnos = Vec::new();
            for id in min..=max {
                let resp = self
                    .send_command(&format!("ZLNKTYP{}?", id), false)
                    .await;
                match resp {
                    Ok(r) => {
                        let val = parse_value_after_eq(&r);
                        if val.starts_with('N') {
                            ztechnos.push("N".to_string());
                        } else {
                            ztechnos.push(val.to_string());
                        }
                    }
                    Err(_) => ztechnos.push("N".to_string()),
                }
            }

            for j in 0..(max - min + 1) as usize {
                let idx = (min as usize - 1) + j;
                if idx < zones.len() {
                    let zone = &mut zones[idx];
                    if let Some(label) = zlabels.get(j) {
                        if !label.is_empty() {
                            zone.label = label.clone();
                        }
                    }
                    if let Some(t) = ztypes.get(j) {
                        if let Ok(type_num) = t.parse::<u8>() {
                            zone.zone_type = crate::constants::ZoneType::from_u8(type_num)
                                .unwrap_or(crate::constants::ZoneType::NotUsed);
                        }
                    }
                    if let Some(t) = ztechnos.get(j) {
                        zone.technology = ZoneTechnology::from_code(t);
                    }
                    if let Some(p) = zparts.get(j) {
                        zone.partitions = Zone::parse_partitions(p);
                    }
                    if let Some(g) = zareas.get(j) {
                        zone.groups = Zone::parse_groups(g);
                    }
                    if let Some(s) = zstatus.get(j) {
                        zone.update_status(s);
                    }
                }
            }
        }

        zones.truncate(actual_count);
        debug!("Zone discovery complete: {} zones found", zones.len());
        Ok(zones)
    }

    /// Query all output data from the panel.
    ///
    /// Queries outputs in batches of 8. If the panel returns an N19 error
    /// ("Device Doesn't Exist") for a batch, discovery stops early and
    /// only the outputs discovered so far are returned. This handles panels
    /// where `max_outputs` exceeds the actual physical output count (e.g.,
    /// ProsysPlus with 262 max outputs but only 16 physical outputs).
    pub async fn get_all_outputs(&self) -> Result<Vec<Output>> {
        debug!("Retrieving output configuration");
        let mut outputs: Vec<Output> = (1..=self.max_outputs).map(Output::new).collect();
        let mut actual_count = self.max_outputs as usize;

        'batch: for batch_start in (0..self.max_outputs).step_by(8) {
            let min = batch_start + 1;
            let max = (batch_start + 8).min(self.max_outputs);

            let otypes = match self.send_command(&format!("OTYPE*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Output slot {} doesn't exist, stopping output discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let otypes = parse_tab_separated_trimmed(&otypes);

            let olabels = match self.send_command(&format!("OLBL*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Output slot {} doesn't exist, stopping output discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let olabels = parse_tab_separated_labels(&olabels);

            let ostatus = match self.send_command(&format!("OSTT*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Output slot {} doesn't exist, stopping output discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let ostatus = parse_tab_separated_trimmed(&ostatus);

            let ogroups = match self.send_command(&format!("OGROP*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Output slot {} doesn't exist, stopping output discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let ogroups = parse_tab_separated_trimmed(&ogroups);

            for j in 0..(max - min + 1) as usize {
                let idx = (min as usize - 1) + j;
                if idx < outputs.len() {
                    let output = &mut outputs[idx];
                    output.label = olabels.get(j).cloned().unwrap_or_default();

                    if let Some(t) = otypes.get(j) {
                        let type_num: u8 = t.parse().unwrap_or(1);
                        output.output_type = OutputType::from_value(type_num);

                        // Query pulse delay for pulsed outputs
                        if type_num % 2 == 0 {
                            let pulse_resp = self
                                .send_command(&format!("OPULSE{}?", min as usize + j), false)
                                .await;
                            if let Ok(pr) = pulse_resp {
                                let val = parse_value_after_eq(&pr)
                                    .replace(' ', "")
                                    .parse::<u64>()
                                    .unwrap_or(0);
                                output.pulse_delay_ms = val * 1000;
                            }
                        }
                    }

                    if let Some(s) = ostatus.get(j) {
                        output.update_status(s);
                    }

                    if let Some(g) = ogroups.get(j) {
                        output.user_usable = g == "4";
                    }
                }
            }
        }

        outputs.truncate(actual_count);
        debug!("Output discovery complete: {} outputs found", outputs.len());
        Ok(outputs)
    }

    /// Query all partition data from the panel.
    ///
    /// Queries partitions in batches of 8. If the panel returns an N19 error
    /// ("Device Doesn't Exist") for a batch, discovery stops early and
    /// only the partitions discovered so far are returned.
    pub async fn get_all_partitions(&self) -> Result<Vec<Partition>> {
        debug!("Retrieving partition configuration");
        let mut partitions: Vec<Partition> = (1..=self.max_parts).map(Partition::new).collect();
        let mut actual_count = self.max_parts as usize;

        'batch: for batch_start in (0..self.max_parts).step_by(8) {
            let min = batch_start + 1;
            let max = (batch_start + 8).min(self.max_parts);

            let plabels = match self.send_command(&format!("PLBL*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Partition slot {} doesn't exist, stopping partition discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let plabels = parse_tab_separated_labels(&plabels);

            let pstatus = match self.send_command(&format!("PSTT*{}:{}?", min, max), false).await {
                Ok(resp) if is_n19(&resp) => {
                    debug!("Partition slot {} doesn't exist, stopping partition discovery", min);
                    actual_count = (min - 1) as usize;
                    break 'batch;
                }
                Ok(resp) => resp,
                Err(e) => return Err(e),
            };
            let pstatus = parse_tab_separated_trimmed(&pstatus);

            for j in 0..(max - min + 1) as usize {
                let idx = (min as usize - 1) + j;
                if idx < partitions.len() {
                    let part = &mut partitions[idx];
                    part.label = plabels.get(j).cloned().unwrap_or_default();
                    if let Some(s) = pstatus.get(j) {
                        part.update_status(s);
                    }
                }
            }
        }

        partitions.truncate(actual_count);
        debug!("Partition discovery complete: {} partitions found", partitions.len());
        Ok(partitions)
    }

    /// Query system data from the panel.
    pub async fn get_system_data(&self) -> Result<MBSystem> {
        debug!("Retrieving system information");
        let label_resp = self.send_command("SYSLBL?", false).await?;
        let label = parse_value_after_eq(&label_resp).trim().to_string();

        let status_resp = self.send_command("SSTT?", false).await?;
        let status_str = parse_value_after_eq(&status_resp);

        Ok(MBSystem::new(label, status_str))
    }

    /// Disconnect from the panel.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(ref transport) = self.direct_transport {
            transport.disconnect().await?;
        }
        if let Some(ref transport) = self.proxy_transport {
            transport.disconnect().await?;
        }
        self.direct_transport = None;
        self.proxy_transport = None;
        Ok(())
    }

    pub fn max_zones(&self) -> u32 {
        self.max_zones
    }

    pub fn max_partitions(&self) -> u32 {
        self.max_parts
    }

    pub fn max_outputs(&self) -> u32 {
        self.max_outputs
    }

    pub fn firmware_version(&self) -> Option<&str> {
        self.firmware_version.as_deref()
    }
}

/// Check if a panel response indicates "Device Doesn't Exist" (N19 error).
///
/// When querying device slots beyond the physical device count, the panel
/// returns an N19 error code. This is used during discovery to stop early
/// rather than failing the entire discovery process.
fn is_n19(response: &str) -> bool {
    let trimmed = response.trim();
    PanelErrorCode::from_code(trimmed) == Some(PanelErrorCode::N19)
}

/// Get the local GMT timezone offset string (e.g., "+02:00").
fn local_gmt_offset() -> String {
    // Use chrono-free approach: calculate from system timezone
    // For now, return UTC as default. In production, use system timezone.
    let _now = std::time::SystemTime::now();
    // Simple implementation: UTC offset
    "+00:00".to_string()
}
