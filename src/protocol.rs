// MIT License - Copyright (c) 2021 TJForc
// Rust translation

use crate::config::ArmType;

/// Commands that can be sent to the Risco panel.
#[derive(Debug, Clone)]
pub enum Command {
    /// RMT=<password> — Remote authentication
    Rmt { password: String },
    /// LCL — Start local encrypted session
    Lcl,
    /// DCN — Disconnect
    Dcn,
    /// ACK — Acknowledge unsolicited message
    Ack,
    /// CLOCK — Watchdog keep-alive
    Clock,
    /// PNLCNF — Query panel configuration/type
    PanelConfig,
    /// FSVER? — Query firmware version
    FirmwareVersion,
    /// CUSTLST? — Customer list (used for crypt test)
    CustomerList,
    /// CUSTLST (without ?) — Also used for crypt verification
    CustomerListVerify,
    /// ELASEN? — Query RiscoCloud enabled status
    QueryCloudEnabled,
    /// ELASEN=<0|1> — Enable/disable RiscoCloud
    SetCloudEnabled { enabled: bool },
    /// TIMEZONE? — Query timezone
    QueryTimezone,
    /// TIMEZONE=<index> — Set timezone
    SetTimezone { index: u8 },
    /// INTP? — Query NTP server
    QueryNtpServer,
    /// INTP=<server> — Set NTP server
    SetNtpServer { server: String },
    /// INTPP? — Query NTP port
    QueryNtpPort,
    /// INTPP=<port> — Set NTP port
    SetNtpPort { port: String },
    /// INTPPROT? — Query NTP protocol
    QueryNtpProtocol,
    /// INTPPROT=<1> — Set NTP protocol
    SetNtpProtocol { protocol: u8 },
    /// PROG=1 — Enter programming mode
    EnableProgMode,
    /// PROG=2 — Exit programming mode
    DisableProgMode,
    /// SYSLBL? — Query system label
    SystemLabel,
    /// SSTT? — Query system status
    SystemStatus,
    /// ZTYPE*<min>:<max>? — Query zone types (batch)
    ZoneTypes { min: u32, max: u32 },
    /// ZPART&*<min>:<max>? — Query zone partitions (batch)
    ZonePartitions { min: u32, max: u32 },
    /// ZAREA&*<min>:<max>? — Query zone areas/groups (batch)
    ZoneAreas { min: u32, max: u32 },
    /// ZLBL*<min>:<max>? — Query zone labels (batch)
    ZoneLabels { min: u32, max: u32 },
    /// ZSTT*<min>:<max>? — Query zone status (batch)
    ZoneStatus { min: u32, max: u32 },
    /// ZLNKTYP<id>? — Query zone link type (single)
    ZoneLinkType { id: u32 },
    /// OTYPE*<min>:<max>? — Query output types (batch)
    OutputTypes { min: u32, max: u32 },
    /// OLBL*<min>:<max>? — Query output labels (batch)
    OutputLabels { min: u32, max: u32 },
    /// OSTT*<min>:<max>? — Query output status (batch)
    OutputStatus { min: u32, max: u32 },
    /// OGROP*<min>:<max>? — Query output groups (batch)
    OutputGroups { min: u32, max: u32 },
    /// OPULSE<id>? — Query output pulse delay (single)
    OutputPulse { id: u32 },
    /// PLBL*<min>:<max>? — Query partition labels (batch)
    PartitionLabels { min: u32, max: u32 },
    /// PSTT*<min>:<max>? — Query partition status (batch)
    PartitionStatus { min: u32, max: u32 },
    /// ARM=<id> — Arm partition (full/away)
    ArmPartition { id: u32 },
    /// STAY=<id> — Stay/home arm partition
    StayPartition { id: u32 },
    /// DISARM=<id> — Disarm partition
    DisarmPartition { id: u32 },
    /// GARM*{group}={id} — Group arm partition (group 1-4 = A-D)
    GroupArmPartition { group: u8, id: u32 },
    /// ZBYPAS=<id> — Toggle zone bypass
    ToggleBypassZone { id: u32 },
    /// ACTUO<id> — Toggle output
    ToggleOutput { id: u32 },
    /// Raw command string (for any unlisted commands)
    Raw(String),
}

impl Command {
    /// Convert the command to its wire string representation.
    pub fn to_wire_string(&self) -> String {
        match self {
            Command::Rmt { password } => format!("RMT={}", password),
            Command::Lcl => "LCL".to_string(),
            Command::Dcn => "DCN".to_string(),
            Command::Ack => "ACK".to_string(),
            Command::Clock => "CLOCK".to_string(),
            Command::PanelConfig => "PNLCNF".to_string(),
            Command::FirmwareVersion => "FSVER?".to_string(),
            Command::CustomerList => "CUSTLST?".to_string(),
            Command::CustomerListVerify => "CUSTLST".to_string(),
            Command::QueryCloudEnabled => "ELASEN?".to_string(),
            Command::SetCloudEnabled { enabled } => {
                format!("ELASEN={}", if *enabled { 1 } else { 0 })
            }
            Command::QueryTimezone => "TIMEZONE?".to_string(),
            Command::SetTimezone { index } => format!("TIMEZONE={}", index),
            Command::QueryNtpServer => "INTP?".to_string(),
            Command::SetNtpServer { server } => format!("INTP={}", server),
            Command::QueryNtpPort => "INTPP?".to_string(),
            Command::SetNtpPort { port } => format!("INTPP={}", port),
            Command::QueryNtpProtocol => "INTPPROT?".to_string(),
            Command::SetNtpProtocol { protocol } => format!("INTPPROT={}", protocol),
            Command::EnableProgMode => "PROG=1".to_string(),
            Command::DisableProgMode => "PROG=2".to_string(),
            Command::SystemLabel => "SYSLBL?".to_string(),
            Command::SystemStatus => "SSTT?".to_string(),
            Command::ZoneTypes { min, max } => format!("ZTYPE*{}:{}?", min, max),
            Command::ZonePartitions { min, max } => format!("ZPART&*{}:{}?", min, max),
            Command::ZoneAreas { min, max } => format!("ZAREA&*{}:{}?", min, max),
            Command::ZoneLabels { min, max } => format!("ZLBL*{}:{}?", min, max),
            Command::ZoneStatus { min, max } => format!("ZSTT*{}:{}?", min, max),
            Command::ZoneLinkType { id } => format!("ZLNKTYP{}?", id),
            Command::OutputTypes { min, max } => format!("OTYPE*{}:{}?", min, max),
            Command::OutputLabels { min, max } => format!("OLBL*{}:{}?", min, max),
            Command::OutputStatus { min, max } => format!("OSTT*{}:{}?", min, max),
            Command::OutputGroups { min, max } => format!("OGROP*{}:{}?", min, max),
            Command::OutputPulse { id } => format!("OPULSE{}?", id),
            Command::PartitionLabels { min, max } => format!("PLBL*{}:{}?", min, max),
            Command::PartitionStatus { min, max } => format!("PSTT*{}:{}?", min, max),
            Command::ArmPartition { id } => format!("ARM={}", id),
            Command::StayPartition { id } => format!("STAY={}", id),
            Command::DisarmPartition { id } => format!("DISARM={}", id),
            Command::GroupArmPartition { group, id } => format!("GARM*{}={}", group, id),
            Command::ToggleBypassZone { id } => format!("ZBYPAS={}", id),
            Command::ToggleOutput { id } => format!("ACTUO{}", id),
            Command::Raw(s) => s.clone(),
        }
    }

    /// Create an arm command from an ArmType enum.
    pub fn arm(id: u32, arm_type: ArmType) -> Self {
        match arm_type {
            ArmType::Away => Command::ArmPartition { id },
            ArmType::Stay => Command::StayPartition { id },
        }
    }
}

/// Parse a response string to extract the value after '='.
/// e.g., "PNLCNF=RP432" → "RP432"
pub fn parse_value_after_eq(response: &str) -> &str {
    if let Some(pos) = response.find('=') {
        &response[pos + 1..]
    } else {
        response
    }
}

/// Parse a tab-separated response after '=' into a Vec of strings.
/// e.g., "ZTYPE*1:8=0\t1\t2\t0\t5\t0\t0\t0" → ["0", "1", "2", "0", "5", "0", "0", "0"]
pub fn parse_tab_separated(response: &str) -> Vec<String> {
    let value = parse_value_after_eq(response);
    value.split('\t').map(|s| s.to_string()).collect()
}

/// Parse a tab-separated response, trimming spaces from each entry.
pub fn parse_tab_separated_trimmed(response: &str) -> Vec<String> {
    let value = parse_value_after_eq(response);
    value
        .split('\t')
        .map(|s| s.replace(' ', ""))
        .collect()
}

/// Parse a tab-separated response preserving spaces (for labels).
pub fn parse_tab_separated_labels(response: &str) -> Vec<String> {
    let value = parse_value_after_eq(response);
    value.split('\t').map(|s| s.trim().to_string()).collect()
}

/// Check if a response string is an ACK.
pub fn is_ack(response: &str) -> bool {
    response.contains("ACK")
}

/// Extract the device ID and status from an unsolicited status update.
/// e.g., "ZSTT5=OA----------" → Some((5, "OA----------"))
pub fn parse_status_update(response: &str, prefix: &str) -> Option<(u32, String)> {
    if !response.starts_with(prefix) {
        return None;
    }
    let after_prefix = &response[prefix.len()..];
    if let Some(eq_pos) = after_prefix.find('=') {
        let id_str = &after_prefix[..eq_pos];
        let status = &after_prefix[eq_pos + 1..];
        id_str.parse::<u32>().ok().map(|id| (id, status.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_wire_strings() {
        assert_eq!(
            Command::Rmt {
                password: "5678".to_string()
            }
            .to_wire_string(),
            "RMT=5678"
        );
        assert_eq!(Command::Lcl.to_wire_string(), "LCL");
        assert_eq!(Command::Clock.to_wire_string(), "CLOCK");
        assert_eq!(
            Command::ZoneTypes { min: 1, max: 8 }.to_wire_string(),
            "ZTYPE*1:8?"
        );
        assert_eq!(
            Command::ArmPartition { id: 1 }.to_wire_string(),
            "ARM=1"
        );
        assert_eq!(
            Command::ToggleOutput { id: 5 }.to_wire_string(),
            "ACTUO5"
        );
    }

    #[test]
    fn test_group_arm_partition_wire_format() {
        assert_eq!(
            Command::GroupArmPartition { group: 1, id: 1 }.to_wire_string(),
            "GARM*1=1"
        );
        assert_eq!(
            Command::GroupArmPartition { group: 4, id: 3 }.to_wire_string(),
            "GARM*4=3"
        );
        assert_eq!(
            Command::GroupArmPartition { group: 2, id: 10 }.to_wire_string(),
            "GARM*2=10"
        );
    }

    #[test]
    fn test_parse_value_after_eq() {
        assert_eq!(parse_value_after_eq("PNLCNF=RP432"), "RP432");
        assert_eq!(parse_value_after_eq("ELASEN=1"), "1");
        assert_eq!(parse_value_after_eq("noequals"), "noequals");
    }

    #[test]
    fn test_parse_tab_separated() {
        let result = parse_tab_separated("ZTYPE*1:3=0\t1\t5");
        assert_eq!(result, vec!["0", "1", "5"]);
    }

    #[test]
    fn test_parse_status_update() {
        let result = parse_status_update("ZSTT5=OA----------", "ZSTT");
        assert_eq!(result, Some((5, "OA----------".to_string())));

        let result = parse_status_update("PSTT1=-A-----------R-----", "PSTT");
        assert_eq!(result, Some((1, "-A-----------R-----".to_string())));

        assert!(parse_status_update("CLOCK", "ZSTT").is_none());
    }

    #[test]
    fn test_is_ack() {
        assert!(is_ack("ACK"));
        assert!(!is_ack("N01"));
    }
}
