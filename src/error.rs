// MIT License - Copyright (c) 2021 TJForc
// Rust translation

use std::fmt;

/// Error codes returned by the Risco panel in response to commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelErrorCode {
    /// BCK2 - Callback Error
    Bck2,
    /// N01 - Error
    N01,
    /// N02 - Unknown Error
    N02,
    /// N03 - Unknown Error
    N03,
    /// N04 - CRC Error
    N04,
    /// N05 - Invalid parameter
    N05,
    /// N06 - Invalid Value
    N06,
    /// N07 - System Armed
    N07,
    /// N08 - System Alarm
    N08,
    /// N09 - Default Jumper
    N09,
    /// N10 - System Not In Prog Mode
    N10,
    /// N11 - System In Prog Mode
    N11,
    /// N12 - System Not Ready to Arm
    N12,
    /// N13 - General Error
    N13,
    /// N14 - Device Does Not Support This Operation
    N14,
    /// N15 - MS Locked
    N15,
    /// N16 - System Busy
    N16,
    /// N17 - Pin Code In Use
    N17,
    /// N18 - System In RF Allocation Mode
    N18,
    /// N19 - Device Doesn't Exist
    N19,
    /// N20 - TEOL Termination Not Supported
    N20,
    /// N21 - Unknown Error
    N21,
    /// N22 - Unknown Error
    N22,
    /// N23 - Unknown Error
    N23,
    /// N24 - System in Remote Upgrade
    N24,
    /// N25 - CW Test Failed
    N25,
}

impl PanelErrorCode {
    /// Parse an error code string from the panel (e.g., "N01", "BCK2").
    pub fn from_code(s: &str) -> Option<Self> {
        match s {
            "BCK2" => Some(Self::Bck2),
            "N01" => Some(Self::N01),
            "N02" => Some(Self::N02),
            "N03" => Some(Self::N03),
            "N04" => Some(Self::N04),
            "N05" => Some(Self::N05),
            "N06" => Some(Self::N06),
            "N07" => Some(Self::N07),
            "N08" => Some(Self::N08),
            "N09" => Some(Self::N09),
            "N10" => Some(Self::N10),
            "N11" => Some(Self::N11),
            "N12" => Some(Self::N12),
            "N13" => Some(Self::N13),
            "N14" => Some(Self::N14),
            "N15" => Some(Self::N15),
            "N16" => Some(Self::N16),
            "N17" => Some(Self::N17),
            "N18" => Some(Self::N18),
            "N19" => Some(Self::N19),
            "N20" => Some(Self::N20),
            "N21" => Some(Self::N21),
            "N22" => Some(Self::N22),
            "N23" => Some(Self::N23),
            "N24" => Some(Self::N24),
            "N25" => Some(Self::N25),
            _ => None,
        }
    }

    /// Returns true if the given string is a known panel error code.
    pub fn is_error_code(s: &str) -> bool {
        Self::from_code(s).is_some()
    }

    /// Human-readable description of the error code.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Bck2 => "Callback Error",
            Self::N01 => "Error",
            Self::N02 => "Unknown Error N02",
            Self::N03 => "Unknown Error N03",
            Self::N04 => "CRC Error",
            Self::N05 => "Invalid parameter",
            Self::N06 => "Invalid Value",
            Self::N07 => "System Armed",
            Self::N08 => "System Alarm",
            Self::N09 => "Default Jumper",
            Self::N10 => "System Not In Prog Mode",
            Self::N11 => "System In Prog Mode",
            Self::N12 => "System Not Ready to Arm",
            Self::N13 => "General Error",
            Self::N14 => "Device Does Not Support This Operation",
            Self::N15 => "MS Locked",
            Self::N16 => "System Busy",
            Self::N17 => "Pin Code In Use",
            Self::N18 => "System In RF Allocation Mode",
            Self::N19 => "Device Doesn't Exist",
            Self::N20 => "TEOL Termination Not Supported",
            Self::N21 => "Unknown Error N21",
            Self::N22 => "Unknown Error N22",
            Self::N23 => "Unknown Error N23",
            Self::N24 => "System in Remote Upgrade",
            Self::N25 => "CW Test Failed",
        }
    }

    /// The wire string representation (e.g., "N01", "BCK2").
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bck2 => "BCK2",
            Self::N01 => "N01",
            Self::N02 => "N02",
            Self::N03 => "N03",
            Self::N04 => "N04",
            Self::N05 => "N05",
            Self::N06 => "N06",
            Self::N07 => "N07",
            Self::N08 => "N08",
            Self::N09 => "N09",
            Self::N10 => "N10",
            Self::N11 => "N11",
            Self::N12 => "N12",
            Self::N13 => "N13",
            Self::N14 => "N14",
            Self::N15 => "N15",
            Self::N16 => "N16",
            Self::N17 => "N17",
            Self::N18 => "N18",
            Self::N19 => "N19",
            Self::N20 => "N20",
            Self::N21 => "N21",
            Self::N22 => "N22",
            Self::N23 => "N23",
            Self::N24 => "N24",
            Self::N25 => "N25",
        }
    }
}

impl fmt::Display for PanelErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.as_str(), self.description())
    }
}

/// All errors that can occur in the risco-lan-bridge library.
#[derive(Debug, thiserror::Error)]
pub enum RiscoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Connection timeout")]
    ConnectionTimeout,

    #[error("Command timeout: {command}")]
    CommandTimeout { command: String },

    #[error("Panel error: {0}")]
    PanelError(PanelErrorCode),

    #[error("CRC mismatch")]
    CrcMismatch,

    #[error("Too many CRC errors (exceeded limit of {limit})")]
    CrcLimitExceeded { limit: u32 },

    #[error("Bad access code: {code}")]
    BadAccessCode { code: String },

    #[error("Bad encryption key: panel_id={panel_id}")]
    BadCryptKey { panel_id: u16 },

    #[error("Socket disconnected")]
    Disconnected,

    #[error("Socket destroyed while in use")]
    SocketDestroyed,

    #[error("Discovery failed: {reason}")]
    DiscoveryFailed { reason: String },

    #[error("Invalid response: {details}")]
    InvalidResponse { details: String },

    #[error("Invalid device ID: {id} (max: {max})")]
    InvalidDeviceId { id: u32, max: u32 },

    #[error("Partition not ready for arming: id={id}")]
    PartitionNotReady { id: u32 },

    #[error("Channel closed")]
    ChannelClosed,
}

impl RiscoError {
    /// Whether this error is transient and the connection should be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            RiscoError::Io(_)
                | RiscoError::ConnectionTimeout
                | RiscoError::CommandTimeout { .. }
                | RiscoError::Disconnected
                | RiscoError::SocketDestroyed
                | RiscoError::CrcMismatch
                | RiscoError::CrcLimitExceeded { .. }
                | RiscoError::ChannelClosed
        )
    }
}

pub type Result<T> = std::result::Result<T, RiscoError>;
