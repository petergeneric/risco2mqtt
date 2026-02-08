// MIT License - Copyright (c) 2021 TJForc
// Rust translation of risco-lan-bridge
//
//! # risco-lan-bridge
//!
//! Direct TCP/IP communication with Risco alarm control panels
//! (Agility, WiComm, WiCommPro, LightSYS, ProsysPlus, GTPlus).
//!
//! This library acts as a bridge between applications and Risco panels,
//! bypassing the RiscoCloud service. No external dependencies beyond
//! tokio, thiserror, tracing, and bitflags.
//!
//! ## Quick Start
//!
//! ```no_run
//! use risco_lan_bridge::{PanelConfig, PanelType, RiscoPanel, ArmType};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = PanelConfig::builder()
//!         .panel_type(PanelType::Agility)
//!         .panel_ip("192.168.0.100")
//!         .panel_password("5678")
//!         .panel_id(1)
//!         .build();
//!
//!     let mut panel = RiscoPanel::connect(config).await?;
//!
//!     let mut events = panel.subscribe();
//!     tokio::spawn(async move {
//!         while let Ok(event) = events.recv().await {
//!             println!("Event: {:?}", event);
//!         }
//!     });
//!
//!     panel.arm_partition(1, ArmType::Away).await?;
//!
//!     tokio::signal::ctrl_c().await?;
//!     panel.disconnect().await?;
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod comm;
pub mod constants;
pub mod crypto;
pub mod devices;
pub mod error;
pub mod event;
pub mod panel;
pub mod protocol;
pub mod transport;

// Re-exports for convenience
pub use config::{ArmType, PanelConfig, PanelConfigBuilder, PanelType};
pub use error::{RiscoError, Result};
pub use event::{EventReceiver, PanelEvent};
pub use panel::RiscoPanel;
pub use devices::zone::{Zone, ZoneStatusFlags, ZoneTechnology};
pub use devices::partition::{Partition, PartitionStatusFlags};
pub use devices::output::{Output, OutputType, OutputEvent};
pub use devices::system::{MBSystem, SystemStatusFlags};
