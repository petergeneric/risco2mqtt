// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/RiscoPanel.js

use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

use crate::comm::RiscoComm;
use crate::config::{ArmType, PanelConfig};
use crate::devices::output::Output;
use crate::devices::partition::Partition;
use crate::devices::system::{MBSystem, SystemStatusFlags};
use crate::devices::zone::Zone;
use crate::error::{RiscoError, Result};
use crate::event::{event_channel, EventReceiver, EventSender, PanelEvent};
use crate::protocol::{parse_status_update, parse_tab_separated_trimmed, Command};

/// Cached device data from a previous successful discovery.
///
/// When a connection drops and is retried, this allows skipping the
/// expensive device re-discovery phase (which queries every zone,
/// partition, output, and system device individually). The cached
/// devices are populated into the new panel and will be kept in sync
/// via unsolicited status updates from the panel.
#[derive(Clone)]
struct CachedDevices {
    zones: Vec<Zone>,
    partitions: Vec<Partition>,
    outputs: Vec<Output>,
    system: Option<MBSystem>,
}

/// The main public API for interacting with a Risco alarm panel.
///
/// # Example
///
/// ```no_run
/// use risco_lan_bridge::{PanelConfig, PanelType, RiscoPanel};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = PanelConfig::builder()
///         .panel_type(PanelType::Agility)
///         .panel_ip("192.168.0.100")
///         .panel_password("5678")
///         .panel_id(1)
///         .build();
///
///     let mut panel = RiscoPanel::connect(config).await?;
///
///     // Subscribe to events
///     let mut events = panel.subscribe();
///     tokio::spawn(async move {
///         while let Ok(event) = events.recv().await {
///             println!("Event: {:?}", event);
///         }
///     });
///
///     // Access devices
///     let zones = panel.zones().await;
///     for zone in &zones {
///         if !zone.is_not_used() {
///             println!("Zone {}: {} (open={})", zone.id, zone.label, zone.is_open());
///         }
///     }
///
///     // Arm a partition
///     panel.arm_partition(1, risco_lan_bridge::ArmType::Away).await?;
///
///     // Keep running
///     tokio::signal::ctrl_c().await?;
///     panel.disconnect().await?;
///     Ok(())
/// }
/// ```
pub struct RiscoPanel {
    comm: RiscoComm,
    event_tx: EventSender,
    zones: Arc<RwLock<Vec<Zone>>>,
    partitions: Arc<RwLock<Vec<Partition>>>,
    outputs: Arc<RwLock<Vec<Output>>>,
    system: Arc<RwLock<Option<MBSystem>>>,
    watchdog_handle: Option<tokio::task::JoinHandle<()>>,
    data_listener_handle: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    watchdog_interval_ms: u64,
}

impl RiscoPanel {
    /// Connect to a panel with the given configuration and initialize all devices.
    ///
    /// Retries on transient errors (disconnects, timeouts, I/O errors) with
    /// exponential backoff. The base delay is `reconnect_delay_ms` from the config
    /// and the maximum number of retries is `max_connect_retries`.
    pub async fn connect(config: PanelConfig) -> Result<Self> {
        let max_retries = config.max_connect_retries;
        let base_delay_ms = config.reconnect_delay_ms;
        let mut config = config; // Make mutable for memorizing discovered values
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay_ms = base_delay_ms * (1 << (attempt - 1).min(4));
                warn!(
                    "Connection attempt {} failed, retrying in {:.1}s...",
                    attempt,
                    delay_ms as f64 / 1000.0
                );
                sleep(Duration::from_millis(delay_ms)).await;
            }

            match Self::try_connect(config.clone(), None).await {
                Ok(panel) => {
                    // Memorize any discovered values for future reconnections.
                    // RiscoComm may have updated panel_id (via discovery) or
                    // panel_type (via verify_panel_type) during connection.
                    let updated_config = panel.comm.config();
                    if updated_config.panel_id != config.panel_id {
                        info!(
                            "Memorizing discovered panel ID: {}",
                            updated_config.panel_id
                        );
                        config.panel_id = updated_config.panel_id;
                    }
                    if updated_config.panel_type != config.panel_type {
                        info!(
                            "Memorizing discovered panel type: {:?}",
                            updated_config.panel_type
                        );
                        config.panel_type = updated_config.panel_type;
                    }

                    return Ok(panel);
                }
                Err(e) => {
                    if !e.is_retryable() || attempt == max_retries {
                        return Err(e);
                    }
                    warn!("Connection error (attempt {}): {}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(RiscoError::Disconnected))
    }

    /// Single connection attempt without retries.
    ///
    /// If `cached_devices` is provided, device discovery is skipped and the
    /// cached data is used instead. This avoids the expensive re-discovery
    /// phase on reconnection when devices have already been discovered.
    async fn try_connect(
        config: PanelConfig,
        cached_devices: Option<CachedDevices>,
    ) -> Result<Self> {
        let (event_tx, _event_rx) = event_channel(256);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let auto_discover = config.auto_discover;
        let watchdog_interval_ms = config.watchdog_interval_ms.max(1000);
        let mut comm = RiscoComm::new(config, event_tx.clone());
        comm.connect().await?;

        let zones;
        let partitions;
        let outputs;
        let system;

        if let Some(cached) = cached_devices {
            // Reuse previously discovered devices instead of re-querying
            info!("Skipping device discovery, using cached devices ({} zones, {} partitions, {} outputs)",
                cached.zones.len(), cached.partitions.len(), cached.outputs.len());
            zones = Arc::new(RwLock::new(cached.zones));
            partitions = Arc::new(RwLock::new(cached.partitions));
            outputs = Arc::new(RwLock::new(cached.outputs));
            system = Arc::new(RwLock::new(cached.system));
        } else {
            zones = Arc::new(RwLock::new(Vec::new()));
            partitions = Arc::new(RwLock::new(Vec::new()));
            outputs = Arc::new(RwLock::new(Vec::new()));
            system = Arc::new(RwLock::new(None));
        }

        let mut panel = Self {
            comm,
            event_tx,
            zones,
            partitions,
            outputs,
            system,
            watchdog_handle: None,
            data_listener_handle: None,
            shutdown_tx,
            watchdog_interval_ms,
        };

        // Only run discovery if auto_discover is enabled and no cached devices
        if auto_discover && !panel.has_discovered_devices().await {
            panel.discover_devices().await?;
        }

        // Start watchdog
        panel.start_watchdog(shutdown_rx);

        // Signal system init complete
        let _ = panel.event_tx.send(PanelEvent::SystemInitComplete);
        info!("System initialization completed");

        Ok(panel)
    }

    /// Reconnect to a panel, reusing previously discovered devices.
    ///
    /// Makes a single connection attempt without retries. The caller is
    /// responsible for retry logic (e.g., exponential backoff in main.rs).
    /// Previously discovered devices are passed in to skip the expensive
    /// device re-discovery phase on reconnection.
    ///
    /// # Arguments
    /// * `config` - The panel configuration (ideally with memorized panel_id/type
    ///   from a previous successful connection).
    /// * `zones` - Previously discovered zones.
    /// * `partitions` - Previously discovered partitions.
    /// * `outputs` - Previously discovered outputs.
    /// * `system` - Previously discovered system status.
    pub async fn reconnect(
        config: PanelConfig,
        zones: Vec<Zone>,
        partitions: Vec<Partition>,
        outputs: Vec<Output>,
        system: Option<MBSystem>,
    ) -> Result<Self> {
        let cached = CachedDevices {
            zones,
            partitions,
            outputs,
            system,
        };

        Self::try_connect(config, Some(cached)).await
    }

    /// Check whether devices have already been discovered (i.e. at least
    /// zones or partitions are populated, indicating cached data was loaded).
    async fn has_discovered_devices(&self) -> bool {
        !self.zones.read().await.is_empty() || !self.partitions.read().await.is_empty()
    }

    /// Get the current panel configuration.
    ///
    /// The config may have been updated during connection if panel_id or
    /// panel_type were auto-discovered.
    pub fn config(&self) -> &PanelConfig {
        self.comm.config()
    }

    /// Subscribe to panel events.
    pub fn subscribe(&self) -> EventReceiver {
        self.event_tx.subscribe()
    }

    /// Discover all devices from the panel.
    async fn discover_devices(&mut self) -> Result<()> {
        debug!("Beginning device discovery");

        // System
        match self.comm.get_system_data().await {
            Ok(sys) => {
                *self.system.write().await = Some(sys);
            }
            Err(e) => {
                warn!("Failed to get system data: {}", e);
                *self.system.write().await = Some(MBSystem::new(String::new(), "---------------------"));
            }
        }

        // Zones
        match self.comm.get_all_zones().await {
            Ok(z) => *self.zones.write().await = z,
            Err(e) => warn!("Failed to get zones: {}", e),
        }

        // Outputs
        match self.comm.get_all_outputs().await {
            Ok(o) => *self.outputs.write().await = o,
            Err(e) => warn!("Failed to get outputs: {}", e),
        }

        // Partitions
        match self.comm.get_all_partitions().await {
            Ok(p) => *self.partitions.write().await = p,
            Err(e) => warn!("Failed to get partitions: {}", e),
        }

        debug!("Device discovery completed");
        Ok(())
    }

    /// Start the watchdog task that sends CLOCK at the configured interval.
    ///
    /// The full protocol idle-polls a broader set of commands
    /// (`LCL, PNLCNF, ZALOC, IOALOC, SSTT, PSTT 1-3, ZSTT 1-36, CLOCK,
    /// UOSTT 1-4, BOOTRES`). We use a simpler CLOCK-only keepalive, relying
    /// on unsolicited panel pushes for real-time status updates. The CLOCK
    /// command has a 1200ms timeout.
    ///
    /// The watchdog is resilient to command errors — it logs them and continues
    /// sending CLOCK. Only an explicit shutdown signal stops the task. This
    /// matches the JS implementation where the watchdog catches errors and
    /// reschedules itself regardless.
    fn start_watchdog(&mut self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        let engine = self.comm.engine().expect("transport not connected").clone();
        let watchdog_ms = self.watchdog_interval_ms;
        let event_tx = self.event_tx.clone();
        let handle = tokio::spawn(async move {
            let mut consecutive_failures: u32 = 0;
            loop {
                tokio::select! {
                    _ = sleep(Duration::from_millis(watchdog_ms)) => {
                        match engine.send_command(&Command::Clock, false).await {
                            Ok(_) => {
                                consecutive_failures = 0;
                            }
                            Err(RiscoError::CommandTimeout { .. }) => {
                                consecutive_failures += 1;
                                warn!("Watchdog CLOCK timed out ({} consecutive failures)", consecutive_failures);
                            }
                            Err(e) => {
                                consecutive_failures += 1;
                                warn!("Watchdog CLOCK failed ({} consecutive failures): {}", consecutive_failures, e);
                            }
                        }
                        // If CLOCK has failed many times in a row, the connection is
                        // likely dead. Trigger a reconnection rather than spinning
                        // forever on a broken socket.
                        if consecutive_failures >= 6 {
                            warn!("Watchdog: {} consecutive CLOCK failures, triggering reconnect", consecutive_failures);
                            let _ = event_tx.send(PanelEvent::Disconnected);
                            break;
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Watchdog shutting down");
                            break;
                        }
                    }
                }
            }
        });

        self.watchdog_handle = Some(handle);
    }

    // --- Device Accessors ---

    /// Get a snapshot of all zones.
    pub async fn zones(&self) -> Vec<Zone> {
        self.zones.read().await.clone()
    }

    /// Get a specific zone by ID (1-indexed).
    pub async fn zone(&self, id: u32) -> Option<Zone> {
        let zones = self.zones.read().await;
        zones.get((id as usize).wrapping_sub(1)).cloned()
    }

    /// Get a snapshot of all partitions.
    pub async fn partitions(&self) -> Vec<Partition> {
        self.partitions.read().await.clone()
    }

    /// Get a specific partition by ID (1-indexed).
    pub async fn partition(&self, id: u32) -> Option<Partition> {
        let parts = self.partitions.read().await;
        parts.get((id as usize).wrapping_sub(1)).cloned()
    }

    /// Get a snapshot of all outputs.
    pub async fn outputs(&self) -> Vec<Output> {
        self.outputs.read().await.clone()
    }

    /// Get a specific output by ID (1-indexed).
    pub async fn output(&self, id: u32) -> Option<Output> {
        let outputs = self.outputs.read().await;
        outputs.get((id as usize).wrapping_sub(1)).cloned()
    }

    /// Get a snapshot of the system status.
    pub async fn system(&self) -> Option<MBSystem> {
        self.system.read().await.clone()
    }

    // --- Commands ---

    /// Arm a partition.
    pub async fn arm_partition(&self, id: u32, arm_type: ArmType) -> Result<bool> {
        debug!("Arming partition {} ({:?})", id, arm_type);
        let partitions = self.partitions.read().await;
        let max = partitions.len() as u32;
        if id == 0 || id > max {
            return Err(RiscoError::InvalidDeviceId { id, max });
        }

        let part = &partitions[(id - 1) as usize];
        if !part.is_ready() || part.is_open() {
            return Err(RiscoError::PartitionNotReady { id });
        }

        // Already in desired state?
        match arm_type {
            ArmType::Away if part.is_armed() => return Ok(true),
            ArmType::Stay if part.is_home_stay() => return Ok(true),
            _ => {}
        }
        drop(partitions);

        let cmd = Command::arm(id, arm_type);

        let response = self.comm.send_command(&cmd, false).await?;
        Ok(response == "ACK")
    }

    /// Group arm a partition (groups 1-4 correspond to A-D).
    pub async fn group_arm_partition(&self, id: u32, group: u8) -> Result<bool> {
        debug!("Group arming partition {} group {}", id, group);
        if !(1..=4).contains(&group) {
            return Err(RiscoError::InvalidGroupId { group });
        }
        let partitions = self.partitions.read().await;
        let max = partitions.len() as u32;
        if id == 0 || id > max {
            return Err(RiscoError::InvalidDeviceId { id, max });
        }

        let part = &partitions[(id - 1) as usize];
        if !part.is_ready() || part.is_open() {
            return Err(RiscoError::PartitionNotReady { id });
        }

        if part.is_armed() || part.is_home_stay() {
            return Ok(true); // Already armed
        }
        drop(partitions);

        let response = self
            .comm
            .send_command(&Command::GroupArmPartition { group, id }, false)
            .await?;
        Ok(response == "ACK")
    }

    /// Disarm a partition.
    pub async fn disarm_partition(&self, id: u32) -> Result<bool> {
        debug!("Disarming partition {}", id);
        let partitions = self.partitions.read().await;
        let max = partitions.len() as u32;
        if id == 0 || id > max {
            return Err(RiscoError::InvalidDeviceId { id, max });
        }

        let part = &partitions[(id - 1) as usize];
        if !part.is_armed() && !part.is_home_stay() {
            return Ok(true); // Already disarmed
        }
        drop(partitions);

        let response = self.comm.send_command(&Command::DisarmPartition { id }, false).await?;
        Ok(response == "ACK")
    }

    /// Toggle bypass on a zone.
    pub async fn toggle_bypass_zone(&self, id: u32) -> Result<bool> {
        debug!("Toggle bypass zone {}", id);
        let zones = self.zones.read().await;
        let max = zones.len() as u32;
        if id == 0 || id > max {
            return Err(RiscoError::InvalidDeviceId { id, max });
        }
        drop(zones);

        let response = self.comm.send_command(&Command::ToggleBypassZone { id }, false).await?;
        Ok(response == "ACK")
    }

    /// Toggle an output.
    pub async fn toggle_output(&self, id: u32) -> Result<bool> {
        debug!("Toggle output {}", id);
        let outputs = self.outputs.read().await;
        let max = outputs.len() as u32;
        if id == 0 || id > max {
            return Err(RiscoError::InvalidDeviceId { id, max });
        }
        drop(outputs);

        let response = self.comm.send_command(&Command::ToggleOutput { id }, false).await?;
        Ok(response == "ACK")
    }

    /// Update a zone's status from an unsolicited panel message.
    pub async fn handle_zone_status(&self, data: &str) {
        if let Some((id, status)) = parse_status_update(data, "ZSTT") {
            let mut zones = self.zones.write().await;
            if let Some(zone) = zones.get_mut((id as usize).wrapping_sub(1)) {
                let old_status = zone.status;
                let changed = zone.update_status(&status);
                if !changed.is_empty() {
                    let _ = self.event_tx.send(PanelEvent::ZoneStatusChanged {
                        zone_id: id,
                        old_status,
                        new_status: zone.status,
                        changed,
                    });
                }
            }
        }
    }

    /// Update a partition's status from an unsolicited panel message.
    pub async fn handle_partition_status(&self, data: &str) {
        if let Some((id, status)) = parse_status_update(data, "PSTT") {
            let mut partitions = self.partitions.write().await;
            if let Some(part) = partitions.get_mut((id as usize).wrapping_sub(1)) {
                let old_status = part.status;
                let changed = part.update_status(&status);
                if !changed.is_empty() {
                    let _ = self.event_tx.send(PanelEvent::PartitionStatusChanged {
                        partition_id: id,
                        old_status,
                        new_status: part.status,
                        changed,
                    });
                }
            }
        }
    }

    /// Update an output's status from an unsolicited panel message.
    pub async fn handle_output_status(&self, data: &str) {
        if let Some((id, status)) = parse_status_update(data, "OSTT") {
            let mut outputs = self.outputs.write().await;
            if let Some(output) = outputs.get_mut((id as usize).wrapping_sub(1))
                && let Some(event) = output.update_status(&status)
            {
                let _ = self.event_tx.send(PanelEvent::OutputStatusChanged {
                    output_id: id,
                    event,
                });
            }
        }
    }

    /// Update system status from an unsolicited panel message.
    pub async fn handle_system_status(&self, data: &str) {
        if let Some(eq_pos) = data.find('=') {
            let status_str = &data[eq_pos + 1..];
            let mut system = self.system.write().await;
            if let Some(ref mut sys) = *system {
                let old_status = sys.status;
                let changed = sys.update_status(status_str);

                // Check if prog mode changed
                if changed.contains(SystemStatusFlags::PROG_MODE) {
                    let prog_active = sys.is_prog_mode();
                    let _ = self.event_tx.send(PanelEvent::ProgModeChanged {
                        active: prog_active,
                    });
                }

                if !changed.is_empty() {
                    let _ = self.event_tx.send(PanelEvent::SystemStatusChanged {
                        old_status,
                        new_status: sys.status,
                        changed,
                    });
                }
            }
        }
    }

    /// Route a raw panel data string to the appropriate status handler.
    ///
    /// Called when the transport layer receives an unsolicited status message
    /// (ZSTT, PSTT, OSTT, SSTT) so that cached device state stays in sync.
    ///
    /// `BOOTRES` (panel reboot notification) is handled at the transport layer
    /// in [`emit_panel_data`](crate::transport::direct) by triggering a
    /// disconnect/reconnect cycle, since all session state becomes invalid.
    pub async fn route_panel_data(&self, data: &str) {
        if data.starts_with("ZSTT") {
            self.handle_zone_status(data).await;
        } else if data.starts_with("PSTT") {
            self.handle_partition_status(data).await;
        } else if data.starts_with("OSTT") {
            self.handle_output_status(data).await;
        } else if data.starts_with("SSTT") {
            self.handle_system_status(data).await;
        }
    }

    /// Poll the panel for current zone and partition status, updating the
    /// local cache and emitting change events for any differences.
    ///
    /// This is a lightweight alternative to full device discovery — it only
    /// queries status (ZSTT/PSTT), not labels, types, or configuration.
    /// Use this to keep the cache fresh between unsolicited panel updates.
    pub async fn refresh_status(&self) -> Result<()> {
        let num_zones = self.zones.read().await.len() as u32;
        let num_parts = self.partitions.read().await.len() as u32;

        // Refresh zone status in batches of 8
        for batch_start in (0..num_zones).step_by(8) {
            let min = batch_start + 1;
            let max = (batch_start + 8).min(num_zones);
            let response = self.comm.send_command(
                &Command::ZoneStatus { min, max },
                false,
            ).await?;
            let statuses = parse_tab_separated_trimmed(&response);

            let mut zones = self.zones.write().await;
            for (j, status_str) in statuses.iter().enumerate() {
                let idx = (min as usize - 1) + j;
                if let Some(zone) = zones.get_mut(idx) {
                    let old_status = zone.status;
                    let changed = zone.update_status(status_str);
                    if !changed.is_empty() {
                        let _ = self.event_tx.send(PanelEvent::ZoneStatusChanged {
                            zone_id: zone.id,
                            old_status,
                            new_status: zone.status,
                            changed,
                        });
                    }
                }
            }
        }

        // Refresh partition status in batches of 8
        for batch_start in (0..num_parts).step_by(8) {
            let min = batch_start + 1;
            let max = (batch_start + 8).min(num_parts);
            let response = self.comm.send_command(
                &Command::PartitionStatus { min, max },
                false,
            ).await?;
            let statuses = parse_tab_separated_trimmed(&response);

            let mut partitions = self.partitions.write().await;
            for (j, status_str) in statuses.iter().enumerate() {
                let idx = (min as usize - 1) + j;
                if let Some(part) = partitions.get_mut(idx) {
                    let old_status = part.status;
                    let changed = part.update_status(status_str);
                    if !changed.is_empty() {
                        let _ = self.event_tx.send(PanelEvent::PartitionStatusChanged {
                            partition_id: part.id,
                            old_status,
                            new_status: part.status,
                            changed,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Disconnect from the panel and clean up.
    pub async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from panel");
        // Signal shutdown
        let _ = self.shutdown_tx.send(true);

        // Abort tasks
        if let Some(h) = self.watchdog_handle.take() {
            h.abort();
        }
        if let Some(h) = self.data_listener_handle.take() {
            h.abort();
        }

        self.comm.disconnect().await?;

        // Clear devices
        *self.zones.write().await = Vec::new();
        *self.partitions.write().await = Vec::new();
        *self.outputs.write().await = Vec::new();
        *self.system.write().await = None;

        Ok(())
    }
}

impl Drop for RiscoPanel {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(h) = self.watchdog_handle.take() {
            h.abort();
        }
        if let Some(h) = self.data_listener_handle.take() {
            h.abort();
        }
    }
}
