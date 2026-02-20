// MIT License - Copyright (c) 2021 TJForc
// Rust translation

use std::fmt;

/// Error codes returned by the Risco panel in response to commands.
///
/// Codes N19, N20, N24, N25 were added in v5/v6 firmware (LightSYS-specific);
/// older Agility v1 firmware does not return these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelErrorCode {
    /// BCK2 (`ERROR_CALLBACK`) — Callback error
    Bck2,
    /// N01 (`ERROR01`) — General error
    N01,
    /// N02 — Unknown error
    N02,
    /// N03 — Unknown error
    N03,
    /// N04 (`ERROR_CRC`) — CRC mismatch
    N04,
    /// N05 (`ERROR_PARAM_NOT_VALID`) — Invalid parameter
    N05,
    /// N06 (`ERROR_VALUE_NOT_VALID`) — Invalid value
    N06,
    /// N07 (`ERROR_SYSTEM_ARMED`) — System is armed; command rejected
    N07,
    /// N08 (`ERROR_SYSTEM_ALARM`) — System is in alarm state
    N08,
    /// N09 (`ERROR_DEFAULT_JUMPER`) — Default jumper is present
    N09,
    /// N10 (`ERROR_SYSTEM_NOT_PROG_MODE`) — Not in programming mode
    N10,
    /// N11 (`ERROR_SYSTEM_IN_PROG_MODE`) — Already in programming mode
    N11,
    /// N12 (`ERROR_SYSTEM_NOT_READY_TO_ARM`) — System not ready to arm
    N12,
    /// N13 (`ERROR_GENERAL_ERROR`) — General system error
    N13,
    /// N14 (`INCORRECT_UO_TYPE`) — Incorrect utility output type for this operation
    N14,
    /// N15 (`ERROR_MSLOCK`) — Master siren locked
    N15,
    /// N16 (`ERROR_BUSY`) — Device busy
    N16,
    /// N17 (`PIN_CODE_INUSE`) — PIN code already in use
    N17,
    /// N18 (`SYSTEM_IN_RF_ALLOCATION_MODE`) — RF allocation mode active
    N18,
    /// N19 (`DEVICE_DOES_NOT_EXISTS`) — Device not found (v5+ firmware)
    N19,
    /// N20 (`TEOL_TERMINATION_NOT_SUPPORTED`) — TEOL termination not supported (v5+ firmware)
    N20,
    /// N21 — Unknown error
    N21,
    /// N22 — Unknown error
    N22,
    /// N23 — Unknown error
    N23,
    /// N24 (`ERROR_N24_SYSTEM_IN_REMOTE_UPGRADE_STATE`) — Remote upgrade in progress (v5+ firmware)
    N24,
    /// N25 (`ERROR_N25_CW_TEST_FAILED`) — CW test failed (v5+ firmware)
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
            Self::Bck2 => "Callback error",
            Self::N01 => "General error",
            Self::N02 => "Unknown error N02",
            Self::N03 => "Unknown error N03",
            Self::N04 => "CRC mismatch",
            Self::N05 => "Invalid parameter",
            Self::N06 => "Invalid value",
            Self::N07 => "System armed",
            Self::N08 => "System in alarm",
            Self::N09 => "Default jumper present",
            Self::N10 => "Not in programming mode",
            Self::N11 => "Already in programming mode",
            Self::N12 => "System not ready to arm",
            Self::N13 => "General system error",
            Self::N14 => "Incorrect utility output type",
            Self::N15 => "Master siren locked",
            Self::N16 => "Device busy",
            Self::N17 => "PIN code already in use",
            Self::N18 => "RF allocation mode active",
            Self::N19 => "Device does not exist",
            Self::N20 => "TEOL termination not supported",
            Self::N21 => "Unknown error N21",
            Self::N22 => "Unknown error N22",
            Self::N23 => "Unknown error N23",
            Self::N24 => "Remote upgrade in progress",
            Self::N25 => "CW test failed",
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

    #[error("Invalid group ID: {group} (must be 1-4)")]
    InvalidGroupId { group: u8 },

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
