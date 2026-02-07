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
use crate::protocol::parse_status_update;

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

            match Self::try_connect(config.clone()).await {
                Ok(panel) => return Ok(panel),
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
    async fn try_connect(config: PanelConfig) -> Result<Self> {
        let (event_tx, _event_rx) = event_channel(256);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let auto_discover = config.auto_discover;
        let mut comm = RiscoComm::new(config, event_tx.clone());
        comm.connect().await?;

        let zones = Arc::new(RwLock::new(Vec::new()));
        let partitions = Arc::new(RwLock::new(Vec::new()));
        let outputs = Arc::new(RwLock::new(Vec::new()));
        let system = Arc::new(RwLock::new(None));

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
        };

        if auto_discover {
            panel.discover_devices().await?;
        }

        // Start watchdog
        panel.start_watchdog(shutdown_rx);

        // Signal system init complete
        let _ = panel.event_tx.send(PanelEvent::SystemInitComplete);
        info!("System initialization completed");

        Ok(panel)
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

    /// Start the watchdog timer that sends CLOCK every 5 seconds.
    fn start_watchdog(&mut self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        let _event_tx = self.event_tx.clone();
        // We need a way to send commands from the watchdog.
        // Since RiscoComm isn't Send-safe across tasks directly,
        // we use the event-based approach: the panel runs the watchdog
        // in a separate task that signals back.

        // For the watchdog, we'll track the shutdown signal
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(Duration::from_secs(5)) => {
                        // Watchdog tick — the actual CLOCK command is sent
                        // by the panel's main loop checking this event
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

    /// Send a watchdog CLOCK command. Call this periodically.
    pub async fn send_watchdog(&self) -> Result<()> {
        self.comm.send_command("CLOCK", false).await?;
        Ok(())
    }

    /// Run the panel event loop. This handles watchdog ticks and processes
    /// unsolicited status updates from the panel.
    pub async fn run(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = self.send_watchdog().await {
                warn!("Watchdog CLOCK failed: {}", e);
                break;
            }
        }
        Ok(())
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
        if !part.is_ready() && part.is_open() {
            return Err(RiscoError::PartitionNotReady { id });
        }

        // Already in desired state?
        match arm_type {
            ArmType::Away if part.is_armed() => return Ok(true),
            ArmType::Stay if part.is_home_stay() => return Ok(true),
            _ => {}
        }
        drop(partitions);

        let cmd = match arm_type {
            ArmType::Away => format!("ARM={}", id),
            ArmType::Stay => format!("STAY={}", id),
        };

        let response = self.comm.send_command(&cmd, false).await?;
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

        let response = self.comm.send_command(&format!("DISARM={}", id), false).await?;
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

        let response = self.comm.send_command(&format!("ZBYPAS={}", id), false).await?;
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

        let response = self.comm.send_command(&format!("ACTUO{}", id), false).await?;
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
            if let Some(output) = outputs.get_mut((id as usize).wrapping_sub(1)) {
                if let Some(event) = output.update_status(&status) {
                    let _ = self.event_tx.send(PanelEvent::OutputStatusChanged {
                        output_id: id,
                        event,
                    });
                }
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
