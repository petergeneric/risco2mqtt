// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/RiscoComm.js

use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

use crate::config::{panel_limits, PanelConfig, PanelType};
use crate::constants::{PanelHwType, timezone_index_for_offset};
use crate::devices::output::{Output, OutputType};
use crate::devices::partition::Partition;
use crate::devices::system::MBSystem;
use crate::devices::zone::{Zone, ZoneTechnology};
use crate::error::{PanelErrorCode, RiscoError, Result};
use crate::event::EventSender;
use crate::protocol::{is_ack, parse_tab_separated_labels, parse_tab_separated_trimmed, parse_value_after_eq, Command};
use crate::transport::command::CommandEngine;
use crate::transport::direct::DirectTcpTransport;

/// High-level communication handler.
///
/// Manages the connection lifecycle, panel verification, device discovery,
/// and real-time status updates.
pub struct RiscoComm {
    config: PanelConfig,
    direct_transport: Option<DirectTcpTransport>,
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

        let transport =
            DirectTcpTransport::connect(&self.config, self.event_tx.clone()).await?;
        self.direct_transport = Some(transport);

        // Propagate any discovered panel ID from the crypto engine back into
        // the config so it persists across reconnection attempts.
        if let Ok(engine) = self.engine() {
            let crypt_arc = engine.crypt();
            let crypt = crypt_arc.lock().await;
            let actual_panel_id = crypt.panel_id();
            if actual_panel_id != self.config.panel_id {
                info!(
                    "Panel ID updated via discovery: {} -> {}",
                    self.config.panel_id, actual_panel_id
                );
                self.config.panel_id = actual_panel_id;
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
    pub async fn send_command(&self, command: &Command, is_prog_cmd: bool) -> Result<String> {
        if let Some(ref transport) = self.direct_transport {
            transport.send_command(command, is_prog_cmd).await
        } else {
            Err(RiscoError::Disconnected)
        }
    }

    /// Get the command engine for direct operations.
    pub(crate) fn engine(&self) -> Result<&Arc<CommandEngine>> {
        if let Some(ref t) = self.direct_transport {
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
        let response = self.send_command(&Command::PanelConfig, false).await?;
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
            PanelType::LightSys | PanelType::LightSysPlus | PanelType::ProsysPlus | PanelType::GTPlus => {
                match self.send_command(&Command::FirmwareVersion, false).await {
                    Ok(response) => {
                        let version_full = parse_value_after_eq(&response);
                        // Extract version before space (e.g., "3.0 build 123" → "3.0")
                        let version = version_full
                            .split_whitespace()
                            .next()
                            .unwrap_or(version_full);
                        self.firmware_version = Some(version.to_string());
                        info!("Panel firmware version: {}", version);
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
    async fn verify_panel_configuration(&self) -> Result<Vec<Command>> {
        let mut commands = Vec::new();
        debug!("Checking panel configuration");

        if self.config.disable_risco_cloud {
            // Check RiscoCloud status
            let response = self.send_command(&Command::QueryCloudEnabled, false).await?;
            let cloud_enabled = parse_value_after_eq(&response).trim() == "1";

            if cloud_enabled {
                commands.push(Command::SetCloudEnabled { enabled: false });
                debug!("Prepare panel for disabling RiscoCloud");
            }

            // Check timezone
            let tz_response = self.send_command(&Command::QueryTimezone, false).await?;
            let panel_tz_idx: usize = parse_value_after_eq(&tz_response)
                .trim()
                .parse()
                .unwrap_or(14);

            // Get local timezone offset
            let local_tz = local_gmt_offset();
            if let Some(idx) = timezone_index_for_offset(&local_tz)
                && panel_tz_idx != idx as usize
            {
                commands.push(Command::SetTimezone { index: idx });
                debug!("Prepare panel for updating timezone");
            }

            // Check NTP server
            let ntp_response = self.send_command(&Command::QueryNtpServer, false).await?;
            let panel_ntp = parse_value_after_eq(&ntp_response).trim().to_string();
            if panel_ntp != self.config.ntp_server {
                commands.push(Command::SetNtpServer { server: self.config.ntp_server.clone() });
                debug!("Prepare panel for updating NTP server");
            }

            // Check NTP port
            let ntp_port_response = self.send_command(&Command::QueryNtpPort, false).await?;
            let panel_ntp_port = parse_value_after_eq(&ntp_port_response).trim().to_string();
            if panel_ntp_port != self.config.ntp_port {
                commands.push(Command::SetNtpPort { port: self.config.ntp_port.clone() });
                debug!("Prepare panel for updating NTP port");
            }

            // Check NTP protocol
            let ntp_proto_response = self.send_command(&Command::QueryNtpProtocol, false).await?;
            let panel_ntp_proto = parse_value_after_eq(&ntp_proto_response).trim().to_string();
            if panel_ntp_proto != "1" {
                commands.push(Command::SetNtpProtocol { protocol: 1 });
                debug!("Prepare panel for enabling NTP");
            }
        }

        Ok(commands)
    }

    /// Enter programming mode, execute commands, exit programming mode.
    async fn modify_panel_config(&self, commands: &[Command]) -> Result<()> {
        debug!("Modifying panel configuration ({} commands)", commands.len());

        // Enter prog mode
        let response = self.send_command(&Command::EnableProgMode, true).await?;
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
                    debug!("Config command successful: {:?}", cmd);
                }
                Ok(resp) => {
                    warn!("Config command failed: {:?} -> {}", cmd, resp);
                }
                Err(e) => {
                    warn!("Config command error: {:?} -> {}", cmd, e);
                }
            }
        }

        // Exit prog mode
        let response = self.send_command(&Command::DisableProgMode, true).await?;
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

            let Some(ztypes) = self.send_batch_query(&Command::ZoneTypes { min, max }, "Zone", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let ztypes = parse_tab_separated_trimmed(&ztypes);

            let Some(zparts) = self.send_batch_query(&Command::ZonePartitions { min, max }, "Zone", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let zparts = parse_tab_separated_trimmed(&zparts);

            let Some(zareas) = self.send_batch_query(&Command::ZoneAreas { min, max }, "Zone", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let zareas = parse_tab_separated_trimmed(&zareas);

            let Some(zlabels) = self.send_batch_query(&Command::ZoneLabels { min, max }, "Zone", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let zlabels = parse_tab_separated_labels(&zlabels);

            let Some(zstatus) = self.send_batch_query(&Command::ZoneStatus { min, max }, "Zone", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let zstatus = parse_tab_separated_trimmed(&zstatus);

            // Zone link types are queried individually
            let mut ztechnos = Vec::new();
            for id in min..=max {
                let resp = self
                    .send_command(&Command::ZoneLinkType { id }, false)
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
                    if let Some(label) = zlabels.get(j)
                        && !label.is_empty()
                    {
                        zone.label = label.clone();
                    }
                    if let Some(t) = ztypes.get(j)
                        && let Ok(type_num) = t.parse::<u8>()
                    {
                        zone.zone_type = crate::constants::ZoneType::from_u8(type_num)
                            .unwrap_or(crate::constants::ZoneType::NotUsed);
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

            let Some(otypes) = self.send_batch_query(&Command::OutputTypes { min, max }, "Output", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let otypes = parse_tab_separated_trimmed(&otypes);

            let Some(olabels) = self.send_batch_query(&Command::OutputLabels { min, max }, "Output", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let olabels = parse_tab_separated_labels(&olabels);

            let Some(ostatus) = self.send_batch_query(&Command::OutputStatus { min, max }, "Output", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let ostatus = parse_tab_separated_trimmed(&ostatus);

            let Some(ogroups) = self.send_batch_query(&Command::OutputGroups { min, max }, "Output", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
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
                        if type_num.is_multiple_of(2) {
                            let pulse_resp = self
                                .send_command(&Command::OutputPulse { id: min + j as u32 }, false)
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

            let Some(plabels) = self.send_batch_query(&Command::PartitionLabels { min, max }, "Partition", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
            };
            let plabels = parse_tab_separated_labels(&plabels);

            let Some(pstatus) = self.send_batch_query(&Command::PartitionStatus { min, max }, "Partition", min).await? else {
                actual_count = (min - 1) as usize;
                break 'batch;
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
        let label_resp = self.send_command(&Command::SystemLabel, false).await?;
        let label = parse_value_after_eq(&label_resp).trim().to_string();

        let status_resp = self.send_command(&Command::SystemStatus, false).await?;
        let status_str = parse_value_after_eq(&status_resp);

        Ok(MBSystem::new(label, status_str))
    }

    /// Disconnect from the panel.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(ref transport) = self.direct_transport {
            transport.disconnect().await?;
        }
        self.direct_transport = None;
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

    /// Get the current (potentially updated) panel configuration.
    ///
    /// The config may have been updated during connection if:
    /// - Panel type was auto-detected via `verify_panel_type`
    /// - Panel ID was discovered via brute-force discovery
    pub fn config(&self) -> &PanelConfig {
        &self.config
    }
}

impl RiscoComm {
    /// Send a batch query command, returning `None` if the panel reports N19
    /// ("Device Doesn't Exist") for the requested slot range.
    async fn send_batch_query(
        &self,
        command: &Command,
        device_kind: &str,
        min: u32,
    ) -> Result<Option<String>> {
        match self.send_command(command, false).await {
            Ok(resp) if is_n19(&resp) => {
                debug!("{} slot {} doesn't exist, stopping {} discovery", device_kind, min, device_kind.to_lowercase());
                Ok(None)
            }
            Ok(resp) => Ok(Some(resp)),
            Err(e) => Err(e),
        }
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
    use chrono::Offset;
    let offset = chrono::Local::now().offset().fix();
    let secs = offset.local_minus_utc();
    let hours = secs / 3600;
    let mins = (secs.abs() % 3600) / 60;
    format!("{:+03}:{:02}", hours, mins)
}
