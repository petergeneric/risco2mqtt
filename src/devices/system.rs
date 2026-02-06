// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/Devices/System.js

use bitflags::bitflags;

bitflags! {
    /// System status flags parsed from the 21-character status string.
    ///
    /// Flag positions: `B A P C D 1 2 3 X J I L M W U R S F E Y V T Z Q`
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SystemStatusFlags: u32 {
        /// B - Low battery trouble
        const LOW_BATTERY        = 1 << 0;
        /// A - AC trouble
        const AC_TROUBLE         = 1 << 1;
        /// P - Phone line trouble
        const PHONE_LINE_TROUBLE = 1 << 2;
        /// C - Clock trouble
        const CLOCK_TROUBLE      = 1 << 3;
        /// D - Default switch
        const DEFAULT_SWITCH     = 1 << 4;
        /// 1 - MS1 report trouble
        const MS1_REPORT_TROUBLE = 1 << 5;
        /// 2 - MS2 report trouble
        const MS2_REPORT_TROUBLE = 1 << 6;
        /// 3 - MS3 report trouble
        const MS3_REPORT_TROUBLE = 1 << 7;
        /// X - Box tamper
        const BOX_TAMPER         = 1 << 8;
        /// J - Jamming trouble
        const JAMMING_TROUBLE    = 1 << 9;
        /// I - Programming mode
        const PROG_MODE          = 1 << 10;
        /// L - Learn mode
        const LEARN_MODE         = 1 << 11;
        /// M - Three minute bypass
        const THREE_MIN_BYPASS   = 1 << 12;
        /// W - Walk test
        const WALK_TEST          = 1 << 13;
        /// U - Aux trouble
        const AUX_TROUBLE        = 1 << 14;
        /// R - RS-485 bus trouble
        const RS485_BUS_TROUBLE  = 1 << 15;
        /// S - LS switch
        const LS_SWITCH          = 1 << 16;
        /// F - Bell switch
        const BELL_SWITCH        = 1 << 17;
        /// E - Bell trouble
        const BELL_TROUBLE       = 1 << 18;
        /// Y - Bell tamper
        const BELL_TAMPER        = 1 << 19;
        /// V - Service expired
        const SERVICE_EXPIRED    = 1 << 20;
        /// T - Payment expired
        const PAYMENT_EXPIRED    = 1 << 21;
        /// Z - Service mode
        const SERVICE_MODE       = 1 << 22;
        /// Q - Dual path
        const DUAL_PATH          = 1 << 23;
    }
}

const SYSTEM_FLAG_CHARS: [(char, SystemStatusFlags); 24] = [
    ('B', SystemStatusFlags::LOW_BATTERY),
    ('A', SystemStatusFlags::AC_TROUBLE),
    ('P', SystemStatusFlags::PHONE_LINE_TROUBLE),
    ('C', SystemStatusFlags::CLOCK_TROUBLE),
    ('D', SystemStatusFlags::DEFAULT_SWITCH),
    ('1', SystemStatusFlags::MS1_REPORT_TROUBLE),
    ('2', SystemStatusFlags::MS2_REPORT_TROUBLE),
    ('3', SystemStatusFlags::MS3_REPORT_TROUBLE),
    ('X', SystemStatusFlags::BOX_TAMPER),
    ('J', SystemStatusFlags::JAMMING_TROUBLE),
    ('I', SystemStatusFlags::PROG_MODE),
    ('L', SystemStatusFlags::LEARN_MODE),
    ('M', SystemStatusFlags::THREE_MIN_BYPASS),
    ('W', SystemStatusFlags::WALK_TEST),
    ('U', SystemStatusFlags::AUX_TROUBLE),
    ('R', SystemStatusFlags::RS485_BUS_TROUBLE),
    ('S', SystemStatusFlags::LS_SWITCH),
    ('F', SystemStatusFlags::BELL_SWITCH),
    ('E', SystemStatusFlags::BELL_TROUBLE),
    ('Y', SystemStatusFlags::BELL_TAMPER),
    ('V', SystemStatusFlags::SERVICE_EXPIRED),
    ('T', SystemStatusFlags::PAYMENT_EXPIRED),
    ('Z', SystemStatusFlags::SERVICE_MODE),
    ('Q', SystemStatusFlags::DUAL_PATH),
];

impl SystemStatusFlags {
    /// Parse a system status string (21 chars) into flags.
    pub fn from_status_str(s: &str) -> Self {
        let mut flags = Self::empty();
        for (ch, flag) in &SYSTEM_FLAG_CHARS {
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

    /// Get human-readable event names for flags that became set.
    pub fn set_event_names(changed: Self, new: Self) -> Vec<&'static str> {
        let became_set = changed & new;
        let mut events = Vec::new();
        if became_set.contains(Self::LOW_BATTERY) { events.push("LowBattery"); }
        if became_set.contains(Self::AC_TROUBLE) { events.push("ACUnplugged"); }
        if became_set.contains(Self::PHONE_LINE_TROUBLE) { events.push("PhoneLineTrouble"); }
        if became_set.contains(Self::CLOCK_TROUBLE) { events.push("ClockTrouble"); }
        if became_set.contains(Self::DEFAULT_SWITCH) { events.push("DefaultSwitchOn"); }
        if became_set.contains(Self::MS1_REPORT_TROUBLE) { events.push("MS1ReportTrouble"); }
        if became_set.contains(Self::MS2_REPORT_TROUBLE) { events.push("MS2ReportTrouble"); }
        if became_set.contains(Self::MS3_REPORT_TROUBLE) { events.push("MS3ReportTrouble"); }
        if became_set.contains(Self::BOX_TAMPER) { events.push("BoxTamperOpen"); }
        if became_set.contains(Self::JAMMING_TROUBLE) { events.push("JammingTrouble"); }
        if became_set.contains(Self::PROG_MODE) { events.push("ProgModeOn"); }
        if became_set.contains(Self::LEARN_MODE) { events.push("LearnModeOn"); }
        if became_set.contains(Self::THREE_MIN_BYPASS) { events.push("ThreeMinBypassOn"); }
        if became_set.contains(Self::WALK_TEST) { events.push("WalkTestOn"); }
        if became_set.contains(Self::AUX_TROUBLE) { events.push("AuxTrouble"); }
        if became_set.contains(Self::RS485_BUS_TROUBLE) { events.push("Rs485BusTrouble"); }
        if became_set.contains(Self::LS_SWITCH) { events.push("LsSwitchOn"); }
        if became_set.contains(Self::BELL_SWITCH) { events.push("BellSwitchOn"); }
        if became_set.contains(Self::BELL_TROUBLE) { events.push("BellTrouble"); }
        if became_set.contains(Self::BELL_TAMPER) { events.push("BellTamper"); }
        if became_set.contains(Self::SERVICE_EXPIRED) { events.push("ServiceExpired"); }
        if became_set.contains(Self::PAYMENT_EXPIRED) { events.push("PaymentExpired"); }
        if became_set.contains(Self::SERVICE_MODE) { events.push("ServiceModeOn"); }
        if became_set.contains(Self::DUAL_PATH) { events.push("DualPathOn"); }
        events
    }

    /// Get human-readable event names for flags that became unset.
    pub fn unset_event_names(changed: Self, new: Self) -> Vec<&'static str> {
        let became_unset = changed & !new;
        let mut events = Vec::new();
        if became_unset.contains(Self::LOW_BATTERY) { events.push("BatteryOk"); }
        if became_unset.contains(Self::AC_TROUBLE) { events.push("ACPlugged"); }
        if became_unset.contains(Self::PHONE_LINE_TROUBLE) { events.push("PhoneLineOk"); }
        if became_unset.contains(Self::CLOCK_TROUBLE) { events.push("ClockOk"); }
        if became_unset.contains(Self::DEFAULT_SWITCH) { events.push("DefaultSwitchOff"); }
        if became_unset.contains(Self::MS1_REPORT_TROUBLE) { events.push("MS1ReportOk"); }
        if became_unset.contains(Self::MS2_REPORT_TROUBLE) { events.push("MS2ReportOk"); }
        if became_unset.contains(Self::MS3_REPORT_TROUBLE) { events.push("MS3ReportOk"); }
        if became_unset.contains(Self::BOX_TAMPER) { events.push("BoxTamperClosed"); }
        if became_unset.contains(Self::JAMMING_TROUBLE) { events.push("JammingOk"); }
        if became_unset.contains(Self::PROG_MODE) { events.push("ProgModeOff"); }
        if became_unset.contains(Self::LEARN_MODE) { events.push("LearnModeOff"); }
        if became_unset.contains(Self::THREE_MIN_BYPASS) { events.push("ThreeMinBypassOff"); }
        if became_unset.contains(Self::WALK_TEST) { events.push("WalkTestOff"); }
        if became_unset.contains(Self::AUX_TROUBLE) { events.push("AuxOk"); }
        if became_unset.contains(Self::RS485_BUS_TROUBLE) { events.push("Rs485BusOk"); }
        if became_unset.contains(Self::LS_SWITCH) { events.push("LsSwitchOff"); }
        if became_unset.contains(Self::BELL_SWITCH) { events.push("BellSwitchOff"); }
        if became_unset.contains(Self::BELL_TROUBLE) { events.push("BellOk"); }
        if became_unset.contains(Self::BELL_TAMPER) { events.push("BellTamperOk"); }
        if became_unset.contains(Self::SERVICE_EXPIRED) { events.push("ServiceOk"); }
        if became_unset.contains(Self::PAYMENT_EXPIRED) { events.push("PaymentOk"); }
        if became_unset.contains(Self::SERVICE_MODE) { events.push("ServiceModeOff"); }
        if became_unset.contains(Self::DUAL_PATH) { events.push("DualPathOff"); }
        events
    }
}

/// System-level information and status.
#[derive(Debug, Clone)]
pub struct MBSystem {
    pub label: String,
    pub status: SystemStatusFlags,
    pub first_status: bool,
    pub need_update_config: bool,
}

impl MBSystem {
    pub fn new(label: String, status_str: &str) -> Self {
        let status = SystemStatusFlags::from_status_str(status_str);
        Self {
            label,
            status,
            first_status: true,
            need_update_config: false,
        }
    }

    /// Update status from a status string. Returns the changed flags.
    pub fn update_status(&mut self, status_str: &str) -> SystemStatusFlags {
        let new_status = SystemStatusFlags::from_status_str(status_str);
        let changed = SystemStatusFlags::changed(self.status, new_status);
        self.status = new_status;
        if self.first_status {
            self.first_status = false;
            return SystemStatusFlags::empty();
        }
        changed
    }

    // Convenience accessors
    pub fn is_prog_mode(&self) -> bool { self.status.contains(SystemStatusFlags::PROG_MODE) }
    pub fn is_low_battery(&self) -> bool { self.status.contains(SystemStatusFlags::LOW_BATTERY) }
    pub fn is_ac_trouble(&self) -> bool { self.status.contains(SystemStatusFlags::AC_TROUBLE) }
    pub fn is_box_tamper(&self) -> bool { self.status.contains(SystemStatusFlags::BOX_TAMPER) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_status_from_str() {
        let flags = SystemStatusFlags::from_status_str("----------I----------");
        assert!(flags.contains(SystemStatusFlags::PROG_MODE));
        assert!(!flags.contains(SystemStatusFlags::LOW_BATTERY));
    }

    #[test]
    fn test_system_status_empty() {
        let flags = SystemStatusFlags::from_status_str("---------------------");
        assert_eq!(flags, SystemStatusFlags::empty());
    }

    #[test]
    fn test_system_update_status() {
        let mut sys = MBSystem::new("Panel".to_string(), "---------------------");
        // First status: no events
        let changed = sys.update_status("----------I----------");
        assert!(changed.is_empty());
        assert!(sys.is_prog_mode());

        // Exit prog mode
        let changed = sys.update_status("---------------------");
        assert!(changed.contains(SystemStatusFlags::PROG_MODE));
        assert!(!sys.is_prog_mode());
    }
}
