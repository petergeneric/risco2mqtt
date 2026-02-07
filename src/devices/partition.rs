// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/Devices/Partitions.js

use bitflags::bitflags;

bitflags! {
    /// Partition status flags parsed from the 17-character status string.
    ///
    /// Flag positions: `a D C F P M N A H R O E S 1 2 3 4 T`
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PartitionStatusFlags: u32 {
        /// a - Alarm
        const ALARM          = 0x0000_0001;
        /// D - Duress
        const DURESS         = 0x0000_0002;
        /// C - False code
        const FALSE_CODE     = 0x0000_0004;
        /// F - Fire
        const FIRE           = 0x0000_0008;
        /// P - Panic
        const PANIC          = 0x0000_0010;
        /// M - Medic
        const MEDIC          = 0x0000_0020;
        /// N - No activity
        const NO_ACTIVITY    = 0x0000_0040;
        /// A - Armed
        const ARMED          = 0x0000_0080;
        /// H - Home/Stay armed
        const HOME_STAY      = 0x0000_0100;
        /// R - Ready (can be armed)
        const READY          = 0x0000_0200;
        /// O - Open (at least 1 zone active)
        const OPEN           = 0x0000_0400;
        /// E - Exists
        const EXISTS         = 0x0000_0800;
        /// S - Reset required (memory event)
        const RESET_REQUIRED = 0x0000_1000;
        /// 1 - Group A armed
        const GRP_A_ARMED    = 0x0000_2000;
        /// 2 - Group B armed
        const GRP_B_ARMED    = 0x0000_4000;
        /// 3 - Group C armed
        const GRP_C_ARMED    = 0x0000_8000;
        /// 4 - Group D armed
        const GRP_D_ARMED    = 0x0001_0000;
        /// T - Trouble
        const TROUBLE        = 0x0002_0000;
    }
}

const PARTITION_FLAG_CHARS: [(char, PartitionStatusFlags); 18] = [
    ('a', PartitionStatusFlags::ALARM),
    ('D', PartitionStatusFlags::DURESS),
    ('C', PartitionStatusFlags::FALSE_CODE),
    ('F', PartitionStatusFlags::FIRE),
    ('P', PartitionStatusFlags::PANIC),
    ('M', PartitionStatusFlags::MEDIC),
    ('N', PartitionStatusFlags::NO_ACTIVITY),
    ('A', PartitionStatusFlags::ARMED),
    ('H', PartitionStatusFlags::HOME_STAY),
    ('R', PartitionStatusFlags::READY),
    ('O', PartitionStatusFlags::OPEN),
    ('E', PartitionStatusFlags::EXISTS),
    ('S', PartitionStatusFlags::RESET_REQUIRED),
    ('1', PartitionStatusFlags::GRP_A_ARMED),
    ('2', PartitionStatusFlags::GRP_B_ARMED),
    ('3', PartitionStatusFlags::GRP_C_ARMED),
    ('4', PartitionStatusFlags::GRP_D_ARMED),
    ('T', PartitionStatusFlags::TROUBLE),
];

impl PartitionStatusFlags {
    /// Parse a partition status string (17 chars) into flags.
    pub fn from_status_str(s: &str) -> Self {
        let mut flags = Self::empty();
        for (ch, flag) in &PARTITION_FLAG_CHARS {
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
        if became_set.contains(Self::ALARM) { events.push("Alarm"); }
        if became_set.contains(Self::DURESS) { events.push("Duress"); }
        if became_set.contains(Self::FALSE_CODE) { events.push("FalseCode"); }
        if became_set.contains(Self::FIRE) { events.push("Fire"); }
        if became_set.contains(Self::PANIC) { events.push("Panic"); }
        if became_set.contains(Self::MEDIC) { events.push("Medic"); }
        if became_set.contains(Self::NO_ACTIVITY) { events.push("ActivityAlert"); }
        if became_set.contains(Self::ARMED) { events.push("Armed"); }
        if became_set.contains(Self::HOME_STAY) { events.push("HomeStay"); }
        if became_set.contains(Self::READY) { events.push("Ready"); }
        if became_set.contains(Self::OPEN) { events.push("ZoneOpen"); }
        if became_set.contains(Self::EXISTS) { events.push("Exist"); }
        if became_set.contains(Self::RESET_REQUIRED) { events.push("MemoryEvent"); }
        if became_set.contains(Self::GRP_A_ARMED) { events.push("GrpAArmed"); }
        if became_set.contains(Self::GRP_B_ARMED) { events.push("GrpBArmed"); }
        if became_set.contains(Self::GRP_C_ARMED) { events.push("GrpCArmed"); }
        if became_set.contains(Self::GRP_D_ARMED) { events.push("GrpDArmed"); }
        if became_set.contains(Self::TROUBLE) { events.push("Trouble"); }
        events
    }

    /// Get human-readable event names for flags that became unset.
    pub fn unset_event_names(changed: Self, new: Self) -> Vec<&'static str> {
        let became_unset = changed & !new;
        let mut events = Vec::new();
        if became_unset.contains(Self::ALARM) { events.push("StandBy"); }
        if became_unset.contains(Self::DURESS) { events.push("Free"); }
        if became_unset.contains(Self::FALSE_CODE) { events.push("CodeOk"); }
        if became_unset.contains(Self::FIRE) { events.push("NoFire"); }
        if became_unset.contains(Self::PANIC) { events.push("NoPanic"); }
        if became_unset.contains(Self::MEDIC) { events.push("NoMedic"); }
        if became_unset.contains(Self::NO_ACTIVITY) { events.push("ActivityOk"); }
        if became_unset.contains(Self::ARMED) { events.push("Disarmed"); }
        if became_unset.contains(Self::HOME_STAY) { events.push("HomeDisarmed"); }
        if became_unset.contains(Self::READY) { events.push("NotReady"); }
        if became_unset.contains(Self::OPEN) { events.push("ZoneClosed"); }
        if became_unset.contains(Self::EXISTS) { events.push("NotExist"); }
        if became_unset.contains(Self::RESET_REQUIRED) { events.push("MemoryAck"); }
        if became_unset.contains(Self::GRP_A_ARMED) { events.push("GrpADisarmed"); }
        if became_unset.contains(Self::GRP_B_ARMED) { events.push("GrpBDisarmed"); }
        if became_unset.contains(Self::GRP_C_ARMED) { events.push("GrpCDisarmed"); }
        if became_unset.contains(Self::GRP_D_ARMED) { events.push("GrpDDisarmed"); }
        if became_unset.contains(Self::TROUBLE) { events.push("Ok"); }
        events
    }
}

/// A single alarm partition.
#[derive(Debug, Clone)]
pub struct Partition {
    pub id: u32,
    pub label: String,
    pub status: PartitionStatusFlags,
    pub first_status: bool,
    pub need_update_config: bool,
}

impl Partition {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            label: String::new(),
            status: PartitionStatusFlags::empty(),
            first_status: true,
            need_update_config: false,
        }
    }

    /// Update status from a status string. Returns the changed flags.
    pub fn update_status(&mut self, status_str: &str) -> PartitionStatusFlags {
        let new_status = PartitionStatusFlags::from_status_str(status_str);
        let changed = PartitionStatusFlags::changed(self.status, new_status);
        self.status = new_status;
        if self.first_status {
            self.first_status = false;
            return PartitionStatusFlags::empty();
        }
        changed
    }

    // Convenience accessors
    pub fn is_alarm(&self) -> bool { self.status.contains(PartitionStatusFlags::ALARM) }
    pub fn is_armed(&self) -> bool { self.status.contains(PartitionStatusFlags::ARMED) }
    pub fn is_home_stay(&self) -> bool { self.status.contains(PartitionStatusFlags::HOME_STAY) }
    pub fn is_ready(&self) -> bool { self.status.contains(PartitionStatusFlags::READY) }
    pub fn is_open(&self) -> bool { self.status.contains(PartitionStatusFlags::OPEN) }
    pub fn exists(&self) -> bool { self.status.contains(PartitionStatusFlags::EXISTS) }
    pub fn is_trouble(&self) -> bool { self.status.contains(PartitionStatusFlags::TROUBLE) }
    pub fn is_duress(&self) -> bool { self.status.contains(PartitionStatusFlags::DURESS) }
    pub fn is_false_code(&self) -> bool { self.status.contains(PartitionStatusFlags::FALSE_CODE) }
    pub fn is_panic(&self) -> bool { self.status.contains(PartitionStatusFlags::PANIC) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_status_from_str() {
        let flags = PartitionStatusFlags::from_status_str("-------AH--------");
        assert!(flags.contains(PartitionStatusFlags::ARMED));
        assert!(flags.contains(PartitionStatusFlags::HOME_STAY));
        assert!(!flags.contains(PartitionStatusFlags::ALARM));
    }

    #[test]
    fn test_partition_status_ready_open() {
        let flags = PartitionStatusFlags::from_status_str("---------RO------");
        assert!(flags.contains(PartitionStatusFlags::READY));
        assert!(flags.contains(PartitionStatusFlags::OPEN));
    }

    #[test]
    fn test_partition_status_empty() {
        let flags = PartitionStatusFlags::from_status_str("-----------------");
        assert_eq!(flags, PartitionStatusFlags::empty());
    }

    #[test]
    fn test_partition_update_status() {
        let mut part = Partition::new(1);
        // First status: no events
        let changed = part.update_status("-------A-R-E-----");
        assert!(changed.is_empty());
        assert!(part.is_armed());
        assert!(part.is_ready());

        // Second status: armed → disarmed
        let changed = part.update_status("---------R-E-----");
        assert!(changed.contains(PartitionStatusFlags::ARMED));
        assert!(!part.is_armed());
    }
}
