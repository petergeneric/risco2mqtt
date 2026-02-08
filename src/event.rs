// MIT License - Copyright (c) 2021 TJForc
// Rust translation

use crate::devices::{
    output::OutputEvent,
    partition::PartitionStatusFlags,
    system::SystemStatusFlags,
    zone::ZoneStatusFlags,
};

/// All events that can be emitted by the panel.
///
/// Users subscribe via `panel.subscribe()` to receive a
/// `tokio::sync::broadcast::Receiver<PanelEvent>`.
#[derive(Debug, Clone)]
pub enum PanelEvent {
    /// TCP connection to panel established
    Connected,
    /// TCP connection lost
    Disconnected,
    /// All devices discovered, panel fully initialized and ready
    SystemInitComplete,
    /// Zone status changed
    ZoneStatusChanged {
        zone_id: u32,
        old_status: ZoneStatusFlags,
        new_status: ZoneStatusFlags,
        changed: ZoneStatusFlags,
    },
    /// Partition status changed
    PartitionStatusChanged {
        partition_id: u32,
        old_status: PartitionStatusFlags,
        new_status: PartitionStatusFlags,
        changed: PartitionStatusFlags,
    },
    /// Output status changed
    OutputStatusChanged {
        output_id: u32,
        event: OutputEvent,
    },
    /// System status changed
    SystemStatusChanged {
        old_status: SystemStatusFlags,
        new_status: SystemStatusFlags,
        changed: SystemStatusFlags,
    },
    /// Programming mode changed
    ProgModeChanged {
        active: bool,
    },
    /// Raw unsolicited data from the panel (ZSTT, PSTT, OSTT, SSTT messages).
    /// The transport layer emits this so the panel can update cached device state.
    PanelData(String),
}

/// Type alias for the broadcast sender.
pub type EventSender = tokio::sync::broadcast::Sender<PanelEvent>;

/// Type alias for the broadcast receiver.
pub type EventReceiver = tokio::sync::broadcast::Receiver<PanelEvent>;

/// Create a new event channel with the given capacity.
pub fn event_channel(capacity: usize) -> (EventSender, EventReceiver) {
    tokio::sync::broadcast::channel(capacity)
}
