// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/Devices/Zones.js

use bitflags::bitflags;
use crate::constants::ZoneType;

bitflags! {
    /// Zone status flags parsed from the 12-character status string.
    ///
    /// Each position in the status string corresponds to a flag letter:
    /// `O A a T R L B Y C S H N`
    /// A `-` in any position means that flag is not set.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ZoneStatusFlags: u16 {
        /// O - Zone is open
        const OPEN         = 0b0000_0000_0001;
        /// A - Zone is armed
        const ARMED        = 0b0000_0000_0010;
        /// a - Zone in alarm
        const ALARM        = 0b0000_0000_0100;
        /// T - Tamper detected
        const TAMPER       = 0b0000_0000_1000;
        /// R - Trouble
        const TROUBLE      = 0b0000_0001_0000;
        /// L - Lost (supervision)
        const LOST         = 0b0000_0010_0000;
        /// B - Low battery
        const LOW_BATTERY  = 0b0000_0100_0000;
        /// Y - Bypassed
        const BYPASS       = 0b0000_1000_0000;
        /// C - Communication trouble
        const COMM_TROUBLE = 0b0001_0000_0000;
        /// S - Soak test
        const SOAK_TEST    = 0b0010_0000_0000;
        /// H - 24 hour zone
        const HOURS_24     = 0b0100_0000_0000;
        /// N - Not used
        const NOT_USED     = 0b1000_0000_0000;
    }
}

/// The flag characters in order, matching the status string positions.
const ZONE_FLAG_CHARS: [(char, ZoneStatusFlags); 12] = [
    ('O', ZoneStatusFlags::OPEN),
    ('A', ZoneStatusFlags::ARMED),
    ('a', ZoneStatusFlags::ALARM),
    ('T', ZoneStatusFlags::TAMPER),
    ('R', ZoneStatusFlags::TROUBLE),
    ('L', ZoneStatusFlags::LOST),
    ('B', ZoneStatusFlags::LOW_BATTERY),
    ('Y', ZoneStatusFlags::BYPASS),
    ('C', ZoneStatusFlags::COMM_TROUBLE),
    ('S', ZoneStatusFlags::SOAK_TEST),
    ('H', ZoneStatusFlags::HOURS_24),
    ('N', ZoneStatusFlags::NOT_USED),
];

impl ZoneStatusFlags {
    /// Parse a zone status string (e.g., "OA----------") into flags.
    ///
    /// The string should be 12 characters. Each position corresponds
    /// to a flag character. A `-` means that flag is not set.
    pub fn from_status_str(s: &str) -> Self {
        let mut flags = Self::empty();
        for (ch, flag) in &ZONE_FLAG_CHARS {
            if s.contains(*ch) {
                flags |= *flag;
            }
        }
        flags
    }

    /// Get the flags that changed between old and new status.
    pub fn changed(old: Self, new: Self) -> Self {
        old ^ new
    }

    /// Get human-readable event names for flags that changed to set.
    pub fn set_event_names(changed: Self, new: Self) -> Vec<&'static str> {
        let mut events = Vec::new();
        let became_set = changed & new;
        if became_set.contains(Self::OPEN) { events.push("Open"); }
        if became_set.contains(Self::ARMED) { events.push("Armed"); }
        if became_set.contains(Self::ALARM) { events.push("Alarm"); }
        if became_set.contains(Self::TAMPER) { events.push("Tamper"); }
        if became_set.contains(Self::TROUBLE) { events.push("Trouble"); }
        if became_set.contains(Self::LOST) { events.push("Lost"); }
        if became_set.contains(Self::LOW_BATTERY) { events.push("LowBattery"); }
        if became_set.contains(Self::BYPASS) { events.push("Bypassed"); }
        if became_set.contains(Self::COMM_TROUBLE) { events.push("CommTrouble"); }
        if became_set.contains(Self::SOAK_TEST) { events.push("SoakTest"); }
        if became_set.contains(Self::HOURS_24) { events.push("24HoursZone"); }
        if became_set.contains(Self::NOT_USED) { events.push("ZoneNotUsed"); }
        events
    }

    /// Get human-readable event names for flags that changed to unset.
    pub fn unset_event_names(changed: Self, new: Self) -> Vec<&'static str> {
        let mut events = Vec::new();
        let became_unset = changed & !new;
        if became_unset.contains(Self::OPEN) { events.push("Closed"); }
        if became_unset.contains(Self::ARMED) { events.push("Disarmed"); }
        if became_unset.contains(Self::ALARM) { events.push("StandBy"); }
        if became_unset.contains(Self::TAMPER) { events.push("Hold"); }
        if became_unset.contains(Self::TROUBLE) { events.push("Sureness"); }
        if became_unset.contains(Self::LOST) { events.push("Located"); }
        if became_unset.contains(Self::LOW_BATTERY) { events.push("BatteryOk"); }
        if became_unset.contains(Self::BYPASS) { events.push("UnBypassed"); }
        if became_unset.contains(Self::COMM_TROUBLE) { events.push("CommOk"); }
        if became_unset.contains(Self::SOAK_TEST) { events.push("ExitSoakTest"); }
        if became_unset.contains(Self::HOURS_24) { events.push("NormalZone"); }
        if became_unset.contains(Self::NOT_USED) { events.push("ZoneUsed"); }
        events
    }
}

/// Zone technology/link type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneTechnology {
    /// 'E' - Wired zone
    Wired,
    /// 'B' or 'I' - Bus zone
    Bus,
    /// 'W' - Wireless zone
    Wireless,
    /// 'N' - None (not used)
    None,
}

impl ZoneTechnology {
    /// Parse from the ZLNKTYP response character.
    pub fn from_char(c: char) -> Self {
        match c {
            'E' => Self::Wired,
            'B' | 'I' => Self::Bus,
            'W' => Self::Wireless,
            _ => Self::None,
        }
    }

    /// Parse from a string (takes first char).
    pub fn from_code(s: &str) -> Self {
        s.chars().next().map_or(Self::None, Self::from_char)
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Wired => "Wired Zone",
            Self::Bus => "Bus Zone",
            Self::Wireless => "Wireless Zone",
            Self::None => "None",
        }
    }

    pub fn is_used(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// A single alarm zone.
#[derive(Debug, Clone)]
pub struct Zone {
    pub id: u32,
    pub label: String,
    pub zone_type: ZoneType,
    pub technology: ZoneTechnology,
    pub partitions: Vec<u32>,
    pub groups: Vec<char>,
    pub status: ZoneStatusFlags,
    pub first_status: bool,
    pub need_update_config: bool,
}

impl Zone {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            label: String::new(),
            zone_type: ZoneType::NotUsed,
            technology: ZoneTechnology::None,
            partitions: vec![1],
            groups: Vec::new(),
            status: ZoneStatusFlags::empty(),
            first_status: true,
            need_update_config: false,
        }
    }

    /// Parse partition hex bitmask.
    ///
    /// For simple panels (single hex digit): bit 0=part 1, bit 1=part 2, etc.
    /// For ProsysPlus/GTPlus (multi-digit): each hex digit maps 4 partitions.
    pub fn parse_partitions(hex_str: &str) -> Vec<u32> {
        let mut parts = Vec::new();
        if hex_str.len() == 1 {
            let nibble = u8::from_str_radix(hex_str, 16).unwrap_or(0);
            if nibble & 1 != 0 { parts.push(1); }
            if nibble & 2 != 0 { parts.push(2); }
            if nibble & 4 != 0 { parts.push(3); }
            if nibble & 8 != 0 { parts.push(4); }
        } else {
            for (i, ch) in hex_str.chars().enumerate() {
                let nibble = u8::from_str_radix(&ch.to_string(), 16).unwrap_or(0);
                let base = (i as u32) * 4;
                if nibble & 1 != 0 { parts.push(base + 1); }
                if nibble & 2 != 0 { parts.push(base + 2); }
                if nibble & 4 != 0 { parts.push(base + 3); }
                if nibble & 8 != 0 { parts.push(base + 4); }
            }
        }
        parts
    }

    /// Parse group hex value into group letters (A=1, B=2, C=4, D=8).
    pub fn parse_groups(hex_str: &str) -> Vec<char> {
        let val = u8::from_str_radix(hex_str, 16).unwrap_or(0);
        let mut groups = Vec::new();
        if val & 1 != 0 { groups.push('A'); }
        if val & 2 != 0 { groups.push('B'); }
        if val & 4 != 0 { groups.push('C'); }
        if val & 8 != 0 { groups.push('D'); }
        groups
    }

    /// Update status from a status string. Returns the changed flags.
    pub fn update_status(&mut self, status_str: &str) -> ZoneStatusFlags {
        let new_status = ZoneStatusFlags::from_status_str(status_str);
        let changed = ZoneStatusFlags::changed(self.status, new_status);
        self.status = new_status;
        if self.first_status {
            self.first_status = false;
            return ZoneStatusFlags::empty(); // No events on first status
        }
        changed
    }

    // Convenience accessors matching the JS properties
    pub fn is_open(&self) -> bool { self.status.contains(ZoneStatusFlags::OPEN) }
    pub fn is_armed(&self) -> bool { self.status.contains(ZoneStatusFlags::ARMED) }
    pub fn is_alarm(&self) -> bool { self.status.contains(ZoneStatusFlags::ALARM) }
    pub fn is_tamper(&self) -> bool { self.status.contains(ZoneStatusFlags::TAMPER) }
    pub fn is_trouble(&self) -> bool { self.status.contains(ZoneStatusFlags::TROUBLE) }
    pub fn is_lost(&self) -> bool { self.status.contains(ZoneStatusFlags::LOST) }
    pub fn is_low_battery(&self) -> bool { self.status.contains(ZoneStatusFlags::LOW_BATTERY) }
    pub fn is_bypassed(&self) -> bool { self.status.contains(ZoneStatusFlags::BYPASS) }
    pub fn is_comm_trouble(&self) -> bool { self.status.contains(ZoneStatusFlags::COMM_TROUBLE) }
    pub fn is_soak_test(&self) -> bool { self.status.contains(ZoneStatusFlags::SOAK_TEST) }
    pub fn is_24hours(&self) -> bool { self.status.contains(ZoneStatusFlags::HOURS_24) }
    pub fn is_not_used(&self) -> bool { !self.technology.is_used() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_status_from_str() {
        let flags = ZoneStatusFlags::from_status_str("OA----------");
        assert!(flags.contains(ZoneStatusFlags::OPEN));
        assert!(flags.contains(ZoneStatusFlags::ARMED));
        assert!(!flags.contains(ZoneStatusFlags::ALARM));
        assert!(!flags.contains(ZoneStatusFlags::NOT_USED));
    }

    #[test]
    fn test_zone_status_all_set() {
        let flags = ZoneStatusFlags::from_status_str("OAaTRLBYCSHN");
        assert_eq!(flags, ZoneStatusFlags::all());
    }

    #[test]
    fn test_zone_status_none_set() {
        let flags = ZoneStatusFlags::from_status_str("------------");
        assert_eq!(flags, ZoneStatusFlags::empty());
    }

    #[test]
    fn test_zone_changed_flags() {
        let old = ZoneStatusFlags::from_status_str("OA----------");
        let new = ZoneStatusFlags::from_status_str("-A-T--------");
        let changed = ZoneStatusFlags::changed(old, new);
        assert!(changed.contains(ZoneStatusFlags::OPEN));
        assert!(changed.contains(ZoneStatusFlags::TAMPER));
        assert!(!changed.contains(ZoneStatusFlags::ARMED));
    }

    #[test]
    fn test_parse_partitions_single_digit() {
        assert_eq!(Zone::parse_partitions("F"), vec![1, 2, 3, 4]);
        assert_eq!(Zone::parse_partitions("1"), vec![1]);
        assert_eq!(Zone::parse_partitions("3"), vec![1, 2]);
        assert_eq!(Zone::parse_partitions("0"), Vec::<u32>::new());
    }

    #[test]
    fn test_parse_partitions_multi_digit() {
        // "31" = first digit 3 (part 1,2), second digit 1 (part 5)
        assert_eq!(Zone::parse_partitions("31"), vec![1, 2, 5]);
    }

    #[test]
    fn test_parse_groups() {
        assert_eq!(Zone::parse_groups("F"), vec!['A', 'B', 'C', 'D']);
        assert_eq!(Zone::parse_groups("1"), vec!['A']);
        assert_eq!(Zone::parse_groups("0"), Vec::<char>::new());
    }

    #[test]
    fn test_zone_technology() {
        assert_eq!(ZoneTechnology::from_char('E'), ZoneTechnology::Wired);
        assert_eq!(ZoneTechnology::from_char('B'), ZoneTechnology::Bus);
        assert_eq!(ZoneTechnology::from_char('I'), ZoneTechnology::Bus);
        assert_eq!(ZoneTechnology::from_char('W'), ZoneTechnology::Wireless);
        assert_eq!(ZoneTechnology::from_char('N'), ZoneTechnology::None);
        assert!(ZoneTechnology::Wired.is_used());
        assert!(!ZoneTechnology::None.is_used());
    }

    #[test]
    fn test_zone_update_status() {
        let mut zone = Zone::new(1);
        // First status should return empty (no events)
        let changed = zone.update_status("OA----------");
        assert!(changed.is_empty());
        assert!(zone.is_open());
        assert!(zone.is_armed());

        // Second status should return changed flags
        let changed = zone.update_status("-A-T--------");
        assert!(changed.contains(ZoneStatusFlags::OPEN));
        assert!(changed.contains(ZoneStatusFlags::TAMPER));
        assert!(!zone.is_open());
        assert!(zone.is_tamper());
    }
}
