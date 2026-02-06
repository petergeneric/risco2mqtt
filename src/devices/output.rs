// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/Devices/Outputs.js

/// Output type as reported by the panel.
///
/// Value 0 = Pulse NC, 1 = Latch NC, 2 = Pulse NO, 3 = Latch NO.
/// Even values are pulsed, odd values are latched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    PulseNc,
    LatchNc,
    PulseNo,
    LatchNo,
}

impl OutputType {
    /// Parse from the numeric value returned by OTYPE.
    pub fn from_value(v: u8) -> Self {
        match v {
            0 => Self::PulseNc,
            1 => Self::LatchNc,
            2 => Self::PulseNo,
            3 => Self::LatchNo,
            _ => Self::LatchNc, // default
        }
    }

    /// Whether this output type is pulsed (even values).
    pub fn is_pulsed(&self) -> bool {
        matches!(self, Self::PulseNc | Self::PulseNo)
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::PulseNc => "Pulse NC",
            Self::LatchNc => "Latch NC",
            Self::PulseNo => "Pulse NO",
            Self::LatchNo => "Latch NO",
        }
    }
}

/// A single output device.
#[derive(Debug, Clone)]
pub struct Output {
    pub id: u32,
    pub label: String,
    pub output_type: OutputType,
    pub active: bool,
    pub pulse_delay_ms: u64,
    pub user_usable: bool,
    pub first_status: bool,
    pub need_update_config: bool,
}

impl Output {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            label: String::new(),
            output_type: OutputType::LatchNc,
            active: false,
            pulse_delay_ms: 0,
            user_usable: false,
            first_status: true,
            need_update_config: false,
        }
    }

    /// Update status from a status string (e.g., "a-" or "--").
    /// Returns true if the active state changed (and it's not the first status).
    pub fn update_status(&mut self, status_str: &str) -> Option<OutputEvent> {
        let new_active = status_str.contains('a');
        let prev_active = self.active;
        self.active = new_active;

        if self.first_status {
            self.first_status = false;
            return None;
        }

        if new_active != prev_active {
            if new_active {
                if self.output_type.is_pulsed() {
                    Some(OutputEvent::Pulsed)
                } else {
                    Some(OutputEvent::Activated)
                }
            } else if !self.output_type.is_pulsed() {
                Some(OutputEvent::Deactivated)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Events emitted when output status changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputEvent {
    Activated,
    Deactivated,
    Pulsed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_type_pulsed() {
        assert!(OutputType::PulseNc.is_pulsed());
        assert!(!OutputType::LatchNc.is_pulsed());
        assert!(OutputType::PulseNo.is_pulsed());
        assert!(!OutputType::LatchNo.is_pulsed());
    }

    #[test]
    fn test_output_type_from_value() {
        assert_eq!(OutputType::from_value(0), OutputType::PulseNc);
        assert_eq!(OutputType::from_value(1), OutputType::LatchNc);
        assert_eq!(OutputType::from_value(2), OutputType::PulseNo);
        assert_eq!(OutputType::from_value(3), OutputType::LatchNo);
    }

    #[test]
    fn test_output_update_status() {
        let mut output = Output::new(1);
        output.output_type = OutputType::LatchNc;

        // First status: no event
        let event = output.update_status("a-");
        assert!(event.is_none());
        assert!(output.active);

        // Deactivate
        let event = output.update_status("--");
        assert_eq!(event, Some(OutputEvent::Deactivated));
        assert!(!output.active);

        // Activate again
        let event = output.update_status("a-");
        assert_eq!(event, Some(OutputEvent::Activated));
    }

    #[test]
    fn test_pulsed_output_events() {
        let mut output = Output::new(1);
        output.output_type = OutputType::PulseNc;

        // First status
        output.update_status("--");

        // Activate pulsed output
        let event = output.update_status("a-");
        assert_eq!(event, Some(OutputEvent::Pulsed));

        // Deactivate pulsed output: no event emitted
        let event = output.update_status("--");
        assert!(event.is_none());
    }
}
