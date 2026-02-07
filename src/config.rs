// MIT License - Copyright (c) 2021 TJForc
// Rust translation

use crate::constants::PanelHwType;

/// Panel type with its device count limits.
///
/// Replaces the 6 JS subclasses (Agility, WiComm, WiCommPro, LightSys, ProsysPlus, GTPlus).
/// Each variant only differs in max zones/partitions/outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelType {
    /// Agility 4 (RW032): 32 zones, 3 partitions, 4 outputs
    Agility4,
    /// Agility (RW132): 36 zones, 3 partitions, 4 outputs
    Agility,
    /// WiComm (RW232): 36 zones, 3 partitions, 4 outputs
    WiComm,
    /// WiCommPro (RW332): 36 zones, 3 partitions, 4 outputs
    WiCommPro,
    /// LightSys (RP432): 32-50 zones (FW dependent), 4 partitions, 14-32 outputs (FW dependent)
    LightSys,
    /// ProsysPlus (RP512): 64-128 zones (FW dependent), 32 partitions, 262 outputs
    ProsysPlus,
    /// GTPlus (RP512): same as ProsysPlus
    GTPlus,
}

impl PanelType {
    /// Get the expected hardware type string for panel type verification.
    pub fn hardware_type(&self) -> PanelHwType {
        match self {
            Self::Agility4 => PanelHwType::RW032,
            Self::Agility => PanelHwType::RW132,
            Self::WiComm => PanelHwType::RW232,
            Self::WiCommPro => PanelHwType::RW332,
            Self::LightSys => PanelHwType::RP432,
            Self::ProsysPlus | Self::GTPlus => PanelHwType::RP512,
        }
    }

    /// Returns the default PanelType for a given hardware type.
    ///
    /// Used during auto-discovery when the panel reports its hardware type
    /// via the PNLCNF command. Note that RP512 maps to ProsysPlus by default
    /// (GTPlus shares the same hardware type).
    pub fn from_hw_type(hw: PanelHwType) -> Self {
        match hw {
            PanelHwType::RW032 => Self::Agility4,
            PanelHwType::RW132 => Self::Agility,
            PanelHwType::RW232 => Self::WiComm,
            PanelHwType::RW332 => Self::WiCommPro,
            PanelHwType::RP432 => Self::LightSys,
            PanelHwType::RP512 => Self::ProsysPlus,
        }
    }
}

/// Compute the device limits for a panel type, potentially adjusted by firmware version.
///
/// Returns (max_zones, max_partitions, max_outputs).
pub fn panel_limits(panel_type: PanelType, firmware: Option<&str>) -> (u32, u32, u32) {
    match panel_type {
        PanelType::Agility4 => (32, 3, 4),
        PanelType::Agility => (36, 3, 4),
        PanelType::WiComm => (36, 3, 4),
        PanelType::WiCommPro => (36, 3, 4),
        PanelType::LightSys => {
            // FW >= 3.0: 50 zones, 32 outputs; else 32 zones, 14 outputs
            if firmware.is_some_and(|fw| compare_version(fw, "3.0") >= 0) {
                (50, 4, 32)
            } else {
                (32, 4, 14)
            }
        }
        PanelType::ProsysPlus | PanelType::GTPlus => {
            // FW >= 1.2.0.7: 128 zones; else 64 zones
            let max_zones = if firmware.is_some_and(|fw| compare_version(fw, "1.2.0.7") >= 0) {
                128
            } else {
                64
            };
            (max_zones, 32, 262)
        }
    }
}

/// Compare two version strings (e.g., "3.0" vs "2.9", "1.2.0.7" vs "1.2.0.6").
///
/// Returns: positive if v1 > v2, 0 if equal, negative if v1 < v2.
pub fn compare_version(v1: &str, v2: &str) -> i32 {
    if v1 == v2 {
        return 0;
    }
    let parts1: Vec<i32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let parts2: Vec<i32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();
    let len = parts1.len().min(parts2.len());
    for i in 0..len {
        if parts1[i] > parts2[i] {
            return 1;
        }
        if parts1[i] < parts2[i] {
            return -1;
        }
    }
    // If all compared parts are equal, longer version is greater
    (parts1.len() as i32) - (parts2.len() as i32)
}

/// Socket connection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketMode {
    /// Direct TCP connection to the panel (default port 1000)
    Direct,
    /// Proxy mode: middleman between panel and RiscoCloud
    Proxy,
}

/// Arm type for partition arming commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmType {
    /// Full/away arm
    Away,
    /// Partial/stay/home arm
    Stay,
}

/// Configuration for connecting to a Risco panel.
#[derive(Debug, Clone)]
pub struct PanelConfig {
    /// Panel type (determines device limits)
    pub panel_type: PanelType,
    /// Panel IP address
    pub panel_ip: String,
    /// Panel TCP port (default: 1000)
    pub panel_port: u16,
    /// Remote access password (default: 5678)
    pub panel_password: String,
    /// Panel encryption ID (0001-9999)
    pub panel_id: u16,
    /// Connection mode
    pub socket_mode: SocketMode,
    /// Whether to auto-discover panel ID and password if wrong
    pub discover_code: bool,
    /// Whether to auto-connect on creation
    pub auto_connect: bool,
    /// Whether to auto-discover all devices after connection
    pub auto_discover: bool,
    /// Whether to disable RiscoCloud on the panel
    pub disable_risco_cloud: bool,
    /// Whether to enable RiscoCloud on the panel
    pub enable_risco_cloud: bool,
    /// Reconnection delay in milliseconds (base delay for exponential backoff)
    pub reconnect_delay_ms: u64,
    /// Maximum number of connection retries on transient errors (0 = no retries)
    pub max_connect_retries: u32,
    /// NTP server for time sync when cloud is disabled
    pub ntp_server: String,
    /// NTP port
    pub ntp_port: String,
    /// Proxy mode: listening port for panel connections
    pub listening_port: u16,
    /// Proxy mode: cloud server port
    pub cloud_port: u16,
    /// Proxy mode: cloud server URL
    pub cloud_url: String,
    /// Watchdog CLOCK interval in milliseconds (default: 5000)
    pub watchdog_interval_ms: u64,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            panel_type: PanelType::Agility,
            panel_ip: "192.168.0.100".to_string(),
            panel_port: 1000,
            panel_password: "5678".to_string(),
            panel_id: 1,
            socket_mode: SocketMode::Direct,
            discover_code: true,
            auto_connect: true,
            auto_discover: true,
            disable_risco_cloud: true,
            enable_risco_cloud: false,
            reconnect_delay_ms: 10000,
            max_connect_retries: 3,
            ntp_server: "pool.ntp.org".to_string(),
            ntp_port: "123".to_string(),
            listening_port: 33000,
            cloud_port: 33000,
            cloud_url: "www.riscocloud.com".to_string(),
            watchdog_interval_ms: 5000,
        }
    }
}

impl PanelConfig {
    /// Create a new config builder starting from defaults.
    pub fn builder() -> PanelConfigBuilder {
        PanelConfigBuilder::default()
    }
}

/// Builder for PanelConfig.
#[derive(Debug, Clone, Default)]
pub struct PanelConfigBuilder {
    config: PanelConfig,
}

impl PanelConfigBuilder {
    pub fn panel_type(mut self, panel_type: PanelType) -> Self {
        self.config.panel_type = panel_type;
        self
    }

    pub fn panel_ip(mut self, ip: impl Into<String>) -> Self {
        self.config.panel_ip = ip.into();
        self
    }

    pub fn panel_port(mut self, port: u16) -> Self {
        self.config.panel_port = port;
        self
    }

    pub fn panel_password(mut self, password: impl Into<String>) -> Self {
        self.config.panel_password = password.into();
        self
    }

    pub fn panel_id(mut self, id: u16) -> Self {
        self.config.panel_id = id;
        self
    }

    pub fn socket_mode(mut self, mode: SocketMode) -> Self {
        self.config.socket_mode = mode;
        self
    }

    pub fn discover_code(mut self, discover: bool) -> Self {
        self.config.discover_code = discover;
        self
    }

    pub fn auto_connect(mut self, auto_connect: bool) -> Self {
        self.config.auto_connect = auto_connect;
        self
    }

    pub fn auto_discover(mut self, auto_discover: bool) -> Self {
        self.config.auto_discover = auto_discover;
        self
    }

    pub fn disable_risco_cloud(mut self, disable: bool) -> Self {
        self.config.disable_risco_cloud = disable;
        self
    }

    pub fn enable_risco_cloud(mut self, enable: bool) -> Self {
        self.config.enable_risco_cloud = enable;
        self
    }

    pub fn reconnect_delay_ms(mut self, ms: u64) -> Self {
        self.config.reconnect_delay_ms = ms;
        self
    }

    pub fn max_connect_retries(mut self, retries: u32) -> Self {
        self.config.max_connect_retries = retries;
        self
    }

    pub fn ntp_server(mut self, server: impl Into<String>) -> Self {
        self.config.ntp_server = server.into();
        self
    }

    pub fn ntp_port(mut self, port: impl Into<String>) -> Self {
        self.config.ntp_port = port.into();
        self
    }

    pub fn listening_port(mut self, port: u16) -> Self {
        self.config.listening_port = port;
        self
    }

    pub fn cloud_port(mut self, port: u16) -> Self {
        self.config.cloud_port = port;
        self
    }

    pub fn cloud_url(mut self, url: impl Into<String>) -> Self {
        self.config.cloud_url = url.into();
        self
    }

    pub fn watchdog_interval_ms(mut self, ms: u64) -> Self {
        self.config.watchdog_interval_ms = ms;
        self
    }

    pub fn build(self) -> PanelConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_version() {
        assert_eq!(compare_version("3.0", "3.0"), 0);
        assert!(compare_version("3.1", "3.0") > 0);
        assert!(compare_version("2.9", "3.0") < 0);
        assert!(compare_version("1.2.0.7", "1.2.0.6") > 0);
        assert!(compare_version("1.2.0.7", "1.2.0.7") == 0);
        assert!(compare_version("1.1", "1.2.0.7") < 0);
    }

    #[test]
    fn test_panel_limits() {
        assert_eq!(panel_limits(PanelType::Agility, None), (36, 3, 4));
        assert_eq!(panel_limits(PanelType::LightSys, Some("2.9")), (32, 4, 14));
        assert_eq!(panel_limits(PanelType::LightSys, Some("3.0")), (50, 4, 32));
        assert_eq!(panel_limits(PanelType::ProsysPlus, Some("1.2.0.6")), (64, 32, 262));
        assert_eq!(panel_limits(PanelType::ProsysPlus, Some("1.2.0.7")), (128, 32, 262));
    }

    #[test]
    fn test_panel_type_hw_type() {
        assert_eq!(PanelType::Agility.hardware_type(), PanelHwType::RW132);
        assert_eq!(PanelType::LightSys.hardware_type(), PanelHwType::RP432);
        assert_eq!(PanelType::ProsysPlus.hardware_type(), PanelHwType::RP512);
        assert_eq!(PanelType::GTPlus.hardware_type(), PanelHwType::RP512);
    }

    #[test]
    fn test_agility4_panel() {
        assert_eq!(panel_limits(PanelType::Agility4, None), (32, 3, 4));
        assert_eq!(PanelType::Agility4.hardware_type(), PanelHwType::RW032);
    }

    #[test]
    fn test_config_builder() {
        let config = PanelConfig::builder()
            .panel_type(PanelType::LightSys)
            .panel_ip("10.0.0.1")
            .panel_password("1234")
            .panel_id(5678)
            .build();

        assert_eq!(config.panel_type, PanelType::LightSys);
        assert_eq!(config.panel_ip, "10.0.0.1");
        assert_eq!(config.panel_password, "1234");
        assert_eq!(config.panel_id, 5678);
    }

    #[test]
    fn test_watchdog_interval_config() {
        let config = PanelConfig::builder()
            .watchdog_interval_ms(10000)
            .build();
        assert_eq!(config.watchdog_interval_ms, 10000);
    }

    #[test]
    fn test_watchdog_interval_default() {
        let config = PanelConfig::builder().build();
        assert_eq!(config.watchdog_interval_ms, 5000);
    }

    #[test]
    fn test_panel_type_from_hw_type() {
        assert_eq!(PanelType::from_hw_type(PanelHwType::RW032), PanelType::Agility4);
        assert_eq!(PanelType::from_hw_type(PanelHwType::RW132), PanelType::Agility);
        assert_eq!(PanelType::from_hw_type(PanelHwType::RW232), PanelType::WiComm);
        assert_eq!(PanelType::from_hw_type(PanelHwType::RW332), PanelType::WiCommPro);
        assert_eq!(PanelType::from_hw_type(PanelHwType::RP432), PanelType::LightSys);
        assert_eq!(PanelType::from_hw_type(PanelHwType::RP512), PanelType::ProsysPlus);
    }

    #[test]
    fn test_panel_type_hw_type_roundtrip() {
        // For all types except GTPlus (which shares RP512 with ProsysPlus),
        // from_hw_type(hardware_type()) should return the same variant.
        assert_eq!(PanelType::from_hw_type(PanelType::Agility4.hardware_type()), PanelType::Agility4);
        assert_eq!(PanelType::from_hw_type(PanelType::Agility.hardware_type()), PanelType::Agility);
        assert_eq!(PanelType::from_hw_type(PanelType::WiComm.hardware_type()), PanelType::WiComm);
        assert_eq!(PanelType::from_hw_type(PanelType::WiCommPro.hardware_type()), PanelType::WiCommPro);
        assert_eq!(PanelType::from_hw_type(PanelType::LightSys.hardware_type()), PanelType::LightSys);
        // GTPlus shares RP512 with ProsysPlus, so from_hw_type defaults to ProsysPlus
        assert_eq!(PanelType::from_hw_type(PanelType::GTPlus.hardware_type()), PanelType::ProsysPlus);
    }
}
