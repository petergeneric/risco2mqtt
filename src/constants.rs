// MIT License - Copyright (c) 2021 TJForc
// Rust translation

/// Protocol framing bytes.
pub const STX: u8 = 0x02; // Start of frame
pub const ETX: u8 = 0x03; // End of frame
pub const DLE: u8 = 0x10; // Data Link Escape (byte stuffing)
pub const CRYPT: u8 = 0x11; // Encryption indicator
pub const SEP: u8 = 0x17; // Separator between command and CRC
pub const CLOUD: u8 = 0x13; // Cloud data indicator (byte 19)

/// CRC-16/ARC lookup table (256 entries).
/// Decoded from the base64-encoded JSON array in the original JS source.
pub const CRC_TABLE: [u16; 256] = [
    0, 49345, 49537, 320, 49921, 960, 640, 49729,
    50689, 1728, 1920, 51009, 1280, 50625, 50305, 1088,
    52225, 3264, 3456, 52545, 3840, 53185, 52865, 3648,
    2560, 51905, 52097, 2880, 51457, 2496, 2176, 51265,
    55297, 6336, 6528, 55617, 6912, 56257, 55937, 6720,
    7680, 57025, 57217, 8000, 56577, 7616, 7296, 56385,
    5120, 54465, 54657, 5440, 55041, 6080, 5760, 54849,
    53761, 4800, 4992, 54081, 4352, 53697, 53377, 4160,
    61441, 12480, 12672, 61761, 13056, 62401, 62081, 12864,
    13824, 63169, 63361, 14144, 62721, 13760, 13440, 62529,
    15360, 64705, 64897, 15680, 65281, 16320, 16000, 65089,
    64001, 15040, 15232, 64321, 14592, 63937, 63617, 14400,
    10240, 59585, 59777, 10560, 60161, 11200, 10880, 59969,
    60929, 11968, 12160, 61249, 11520, 60865, 60545, 11328,
    58369, 9408, 9600, 58689, 9984, 59329, 59009, 9792,
    8704, 58049, 58241, 9024, 57601, 8640, 8320, 57409,
    40961, 24768, 24960, 41281, 25344, 41921, 41601, 25152,
    26112, 42689, 42881, 26432, 42241, 26048, 25728, 42049,
    27648, 44225, 44417, 27968, 44801, 28608, 28288, 44609,
    43521, 27328, 27520, 43841, 26880, 43457, 43137, 26688,
    30720, 47297, 47489, 31040, 47873, 31680, 31360, 47681,
    48641, 32448, 32640, 48961, 32000, 48577, 48257, 31808,
    46081, 29888, 30080, 46401, 30464, 47041, 46721, 30272,
    29184, 45761, 45953, 29504, 45313, 29120, 28800, 45121,
    20480, 37057, 37249, 20800, 37633, 21440, 21120, 37441,
    38401, 22208, 22400, 38721, 21760, 38337, 38017, 21568,
    39937, 23744, 23936, 40257, 24320, 40897, 40577, 24128,
    23040, 39617, 39809, 23360, 39169, 22976, 22656, 38977,
    34817, 18624, 18816, 35137, 19200, 35777, 35457, 19008,
    19968, 36545, 36737, 20288, 36097, 19904, 19584, 35905,
    17408, 33985, 34177, 17728, 34561, 18368, 18048, 34369,
    33281, 17088, 17280, 33601, 16640, 33217, 32897, 16448,
];

/// Zone type numeric IDs and their display names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ZoneType {
    NotUsed = 0,
    ExitEntry1 = 1,
    ExitEntry2 = 2,
    ExitOpenEntry1 = 3,
    EntryFollower = 4,
    Instant = 5,
    InternalExitEntry1 = 6,
    InternalExitEntry2 = 7,
    InternalExitOpenEntry1 = 8,
    InternalEntryFollower = 9,
    InternalInstant = 10,
    UoTrigger = 11,
    Day = 12,
    Hour24 = 13,
    Fire = 14,
    Panic = 15,
    Special = 16,
    PulsedKeySwitch = 17,
    FinalExit = 18,
    LatchedKeySwitch = 19,
    EntryFollowerStay = 20,
    PulsedKeySwitchDelayed = 21,
    LatchedKeySwitchDelayed = 22,
    Tamper = 23,
    Technical = 24,
    ExitOpenEntry2 = 25,
    InternalExitOpenEntry2 = 26,
    Water = 27,
    Gas = 28,
    Co = 29,
    ExitTerminator = 30,
    HighTemperature = 31,
    LowTemperature = 32,
    KeyBox = 33,
    KeyswitchArm = 34,
    KeyswitchDelayedArm = 35,
}

impl ZoneType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::NotUsed),
            1 => Some(Self::ExitEntry1),
            2 => Some(Self::ExitEntry2),
            3 => Some(Self::ExitOpenEntry1),
            4 => Some(Self::EntryFollower),
            5 => Some(Self::Instant),
            6 => Some(Self::InternalExitEntry1),
            7 => Some(Self::InternalExitEntry2),
            8 => Some(Self::InternalExitOpenEntry1),
            9 => Some(Self::InternalEntryFollower),
            10 => Some(Self::InternalInstant),
            11 => Some(Self::UoTrigger),
            12 => Some(Self::Day),
            13 => Some(Self::Hour24),
            14 => Some(Self::Fire),
            15 => Some(Self::Panic),
            16 => Some(Self::Special),
            17 => Some(Self::PulsedKeySwitch),
            18 => Some(Self::FinalExit),
            19 => Some(Self::LatchedKeySwitch),
            20 => Some(Self::EntryFollowerStay),
            21 => Some(Self::PulsedKeySwitchDelayed),
            22 => Some(Self::LatchedKeySwitchDelayed),
            23 => Some(Self::Tamper),
            24 => Some(Self::Technical),
            25 => Some(Self::ExitOpenEntry2),
            26 => Some(Self::InternalExitOpenEntry2),
            27 => Some(Self::Water),
            28 => Some(Self::Gas),
            29 => Some(Self::Co),
            30 => Some(Self::ExitTerminator),
            31 => Some(Self::HighTemperature),
            32 => Some(Self::LowTemperature),
            33 => Some(Self::KeyBox),
            34 => Some(Self::KeyswitchArm),
            35 => Some(Self::KeyswitchDelayedArm),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::NotUsed => "Not Used",
            Self::ExitEntry1 => "Exit/Entry 1",
            Self::ExitEntry2 => "Exit/Entry 2",
            Self::ExitOpenEntry1 => "Exit Open/Entry 1",
            Self::EntryFollower => "Entry Follower",
            Self::Instant => "Instant",
            Self::InternalExitEntry1 => "Internal + Exit/Entry 1",
            Self::InternalExitEntry2 => "Internal + Exit/Entry 2",
            Self::InternalExitOpenEntry1 => "Internal + Exit Open/Entry 1",
            Self::InternalEntryFollower => "Internal + Entry Follower",
            Self::InternalInstant => "Internal + Instant",
            Self::UoTrigger => "UO Trigger",
            Self::Day => "Day",
            Self::Hour24 => "24 Hour",
            Self::Fire => "Fire",
            Self::Panic => "Panic",
            Self::Special => "Special",
            Self::PulsedKeySwitch => "Pulsed Key-Switch",
            Self::FinalExit => "Final Exit",
            Self::LatchedKeySwitch => "Latched Key-Switch",
            Self::EntryFollowerStay => "Entry Follower + Stay",
            Self::PulsedKeySwitchDelayed => "Pulsed Key-Switch Delayed",
            Self::LatchedKeySwitchDelayed => "Latched Key-Switch Delayed",
            Self::Tamper => "Tamper",
            Self::Technical => "Technical",
            Self::ExitOpenEntry2 => "Exit Open/Entry 2",
            Self::InternalExitOpenEntry2 => "Internal + Exit Open/Entry 2",
            Self::Water => "Water",
            Self::Gas => "Gas",
            Self::Co => "CO",
            Self::ExitTerminator => "Exit Terminator",
            Self::HighTemperature => "High Temperature",
            Self::LowTemperature => "Low Temperature",
            Self::KeyBox => "Key Box",
            Self::KeyswitchArm => "Keyswitch Arm",
            Self::KeyswitchDelayedArm => "Keyswitch Delayed Arm",
        }
    }
}

/// Timezone offset strings indexed by panel timezone ID (0-33).
pub const TIMEZONE_OFFSETS: [&str; 34] = [
    "-12:00", "-11:00", "-10:00", "-09:00", "-08:00", "-07:00", "-06:00",
    "-05:00", "-04:30", "-04:00", "-03:30", "-03:00", "-02:00", "-01:00",
    "+00:00", "+01:00", "+02:00", "+03:00", "+03:30", "+04:00", "+04:30",
    "+05:00", "+05:30", "+05:45", "+06:00", "+06:30", "+07:00", "+08:00",
    "+09:00", "+09:30", "+10:00", "+11:00", "+12:00", "+13:00",
];

/// Find the timezone index for a given offset string (e.g., "+02:00" → 16).
pub fn timezone_index_for_offset(offset: &str) -> Option<u8> {
    TIMEZONE_OFFSETS
        .iter()
        .position(|&tz| tz == offset)
        .map(|i| i as u8)
}

/// Panel hardware type identifiers returned by PNLCNF command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelHwType {
    RW132,
    RW232,
    RW332,
    RP432,
    RP512,
}

impl PanelHwType {
    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "RW132" => Some(Self::RW132),
            "RW232" => Some(Self::RW232),
            "RW332" => Some(Self::RW332),
            "RP432" => Some(Self::RP432),
            "RP512" => Some(Self::RP512),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RW132 => "RW132",
            Self::RW232 => "RW232",
            Self::RW332 => "RW332",
            Self::RP432 => "RP432",
            Self::RP512 => "RP512",
        }
    }
}
