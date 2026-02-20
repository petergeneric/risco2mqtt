// MIT License - Copyright (c) 2021 TJForc
// Rust translation of lib/RCrypt.js

use crate::constants::{CRC_TABLE, CRYPT, DLE, ETX, SEP, STX};
use tracing::debug;

/// The result of decoding a message from the panel.
#[derive(Debug, Clone)]
pub struct DecodedMessage {
    /// Command sequence ID (e.g., "01"-"49"), or empty string for error responses.
    pub cmd_id: String,
    /// The command/response string content.
    pub command: String,
    /// Whether the CRC was valid.
    pub crc_valid: bool,
}

/// Risco encryption/decryption engine.
///
/// Uses a Panel ID to generate an XOR cipher (LFSR-based pseudo-random buffer).
/// When Panel ID is 0, no encryption is applied.
pub struct RiscoCrypt {
    panel_id: u16,
    pseudo_buffer: [u8; 255],
    crypt_enabled: bool,
    crypt_pos: usize,
}

impl RiscoCrypt {
    /// Create a new crypto engine with the given panel ID.
    pub fn new(panel_id: u16) -> Self {
        let pseudo_buffer = Self::create_pseudo_buffer(panel_id);
        Self {
            panel_id,
            pseudo_buffer,
            crypt_enabled: false,
            crypt_pos: 0,
        }
    }

    /// Get the current panel ID.
    pub fn panel_id(&self) -> u16 {
        self.panel_id
    }

    /// Update the panel ID and regenerate the pseudo buffer.
    pub fn set_panel_id(&mut self, panel_id: u16) {
        self.panel_id = panel_id;
        self.pseudo_buffer = Self::create_pseudo_buffer(panel_id);
    }

    /// Whether encrypted mode is currently active.
    pub fn crypt_enabled(&self) -> bool {
        self.crypt_enabled
    }

    /// Set whether encrypted mode is active.
    pub fn set_crypt_enabled(&mut self, enabled: bool) {
        self.crypt_enabled = enabled;
    }

    /// Generate the LFSR-based pseudo-random buffer for a given panel ID.
    ///
    /// Uses taps at positions [2, 4, 16, 32768] (as Uint16Array values).
    /// When panel_id is 0, returns an all-zeros buffer (no encryption).
    pub fn create_pseudo_buffer(panel_id: u16) -> [u8; 255] {
        let mut buffer = [0u8; 255];
        if panel_id == 0 {
            return buffer;
        }

        let taps: [u16; 4] = [2, 4, 16, 32768];
        let mut pid = panel_id as u32; // Use u32 to handle overflow from shift

        for item in &mut buffer {
            let mut n2: u32 = 0;
            for &tap in &taps {
                if (pid & tap as u32) > 0 {
                    n2 ^= 1;
                }
            }
            pid = (pid << 1) | n2;
            *item = (pid & 255) as u8;
        }

        debug!("Pseudo buffer created for panel ID {}", panel_id);
        buffer
    }

    /// Compute CRC-16 for command bytes.
    ///
    /// Uses CRC-16/ARC algorithm: init 0xFFFF, table lookup.
    /// Returns the CRC as a 4-character uppercase hex string.
    pub fn compute_crc(data: &[u8]) -> String {
        let mut crc: u16 = 0xFFFF;
        for &byte in data {
            crc = (crc >> 8) ^ CRC_TABLE[(crc & 0xFF ^ byte as u16) as usize];
        }
        format!(
            "{:X}{:X}{:X}{:X}",
            (crc >> 12) & 0xF,
            (crc >> 8) & 0xF,
            (crc >> 4) & 0xF,
            crc & 0xF
        )
    }

    /// Compute CRC-16 as a raw u16 value.
    pub fn compute_crc_raw(data: &[u8]) -> u16 {
        let mut crc: u16 = 0xFFFF;
        for &byte in data {
            crc = (crc >> 8) ^ CRC_TABLE[(crc & 0xFF ^ byte as u16) as usize];
        }
        crc
    }

    /// Validate CRC of a decrypted message.
    ///
    /// The message should contain the separator byte (0x17).
    /// Everything up to and including the separator is CRC'd,
    /// and compared against the hex CRC string after the separator.
    pub fn is_valid_crc(decrypted_message: &str, received_crc: &str) -> bool {
        let sep_char = char::from(SEP);
        if let Some(sep_pos) = decrypted_message.find(sep_char) {
            if received_crc.len() != 4 || !received_crc.chars().all(|c| c.is_ascii_hexdigit()) {
                debug!("CRC Not Ok (received CRC is not 4 hex chars: {:?})", received_crc);
                return false;
            }
            let data_with_sep = &decrypted_message[..=sep_pos];
            let computed = Self::compute_crc(data_with_sep.as_bytes());
            let valid = received_crc == computed;
            if valid {
                debug!("CRC Ok");
            } else {
                debug!("CRC Not Ok (expected {}, got {})", computed, received_crc);
            }
            valid
        } else {
            debug!("No separator found in message for CRC validation");
            false
        }
    }

    /// Encode a command for transmission to the panel.
    ///
    /// Produces the full wire frame: `[STX][CRYPT?][encrypted_data][ETX]`
    ///
    /// The total frame size must not exceed [`MAX_PACKET_SIZE`](crate::constants::MAX_PACKET_SIZE)
    /// (230 bytes) — the panel's `DeviceMaxBufferSize` from the Risco
    /// Configuration Software device definitions. Frames exceeding this
    /// limit may be silently truncated or rejected.
    ///
    /// # Arguments
    /// * `command` - The command string (e.g., "RMT=5678")
    /// * `cmd_id` - The 2-digit command ID string (e.g., "01")
    /// * `force_crypt` - Override encryption: Some(true) forces on, Some(false) forces off, None uses current state
    pub fn encode_command(
        &mut self,
        command: &str,
        cmd_id: &str,
        force_crypt: Option<bool>,
    ) -> Vec<u8> {
        self.crypt_pos = 0;
        let mut encrypted = Vec::new();

        // Start of frame
        encrypted.push(STX);

        // Encryption indicator byte: controlled by force_crypt or crypt_enabled
        let show_crypt = force_crypt.unwrap_or(self.crypt_enabled);
        if show_crypt {
            encrypted.push(CRYPT);
        }

        // Actual XOR encryption: always governed by crypt_enabled (matching JS
        // behavior where EncryptChars checks this.CryptCommand regardless of
        // the ForceCrypt parameter).
        let use_crypt = self.crypt_enabled;

        // Build full command: cmd_id + command + separator
        let full_cmd = format!("{}{}{}", cmd_id, command, char::from(SEP));
        let cmd_bytes = full_cmd.as_bytes();

        // Encrypt command bytes
        let encrypted_cmd = self.encrypt_chars(cmd_bytes, use_crypt);
        encrypted.extend_from_slice(&encrypted_cmd);

        // Calculate CRC on unencrypted command bytes
        let crc_value = Self::compute_crc(cmd_bytes);

        // Encrypt CRC bytes
        let encrypted_crc = self.encrypt_chars(crc_value.as_bytes(), use_crypt);
        encrypted.extend_from_slice(&encrypted_crc);

        // End of frame
        encrypted.push(ETX);

        encrypted
    }

    /// Encrypt/encode characters with XOR and DLE escaping.
    ///
    /// Each byte is XORed with the pseudo buffer (if encryption is active),
    /// then DLE-escaped if the result is STX, ETX, or DLE.
    fn encrypt_chars(&mut self, data: &[u8], use_crypt: bool) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len() * 2);

        for &byte in data {
            let mut b = byte;
            if use_crypt {
                b ^= self.pseudo_buffer[self.crypt_pos];
            }

            // DLE escape: if the encrypted byte matches a framing byte,
            // prepend a DLE character
            match b {
                STX | ETX | DLE => {
                    result.push(DLE);
                }
                _ => {}
            }

            result.push(b);
            self.crypt_pos += 1;
        }

        result
    }

    /// Decrypt characters from received data.
    ///
    /// Handles DLE unescaping BEFORE XOR decryption, tracking offset for
    /// DLE bytes that shift the pseudo buffer alignment.
    fn decrypt_chars(&mut self, data: &[u8], use_crypt: bool) -> Vec<u8> {
        let mut result = Vec::new();
        let mut offset: usize = 0;

        // Skip framing: start after STX (and CRYPT if present)
        let start = if use_crypt { 2 } else { 1 };
        // Stop before ETX
        let end = data.len() - 1;

        let mut i = start;
        while i < end {
            if use_crypt {
                // Check for DLE escape sequence
                if data[i] == DLE
                    && i + 1 < end
                    && (data[i + 1] == STX || data[i + 1] == ETX || data[i + 1] == DLE)
                {
                    // DLE escape prefix: skip the DLE byte, account for offset shift
                    offset += 1;
                    self.crypt_pos += 1;
                    i += 1;
                    // Fall through to decrypt the escaped byte below
                }
                let decrypted = data[i] ^ self.pseudo_buffer[self.crypt_pos - offset];
                result.push(decrypted);
            } else {
                result.push(data[i]);
            }

            self.crypt_pos += 1;
            i += 1;
        }

        result
    }

    /// Decode a received message from the panel.
    ///
    /// Determines if encryption is active (byte[1] == 0x11),
    /// decrypts the payload, extracts command ID, command string, and validates CRC.
    pub fn decode_message(&mut self, data: &[u8]) -> DecodedMessage {
        self.crypt_pos = 0;

        // Check if message is encrypted
        let use_crypt = data.len() > 1 && data[1] == CRYPT;
        let decrypted_bytes = self.decrypt_chars(data, use_crypt);

        // Work with raw bytes to avoid panics on invalid UTF-8 (e.g. wrong decryption key).
        // SEP byte is the field separator.
        let (cmd_id, command, crc_value, decrypted_str) = {
            let sep = SEP;

            if decrypted_bytes.len() >= 2
                && decrypted_bytes[0].is_ascii_digit()
                && decrypted_bytes[1].is_ascii_digit()
            {
                // Normal response: first 2 bytes are command ID (ASCII digits 00-49)
                let id = String::from_utf8_lossy(&decrypted_bytes[..2]).to_string();
                let rest = &decrypted_bytes[2..];
                let sep_pos = rest.iter().position(|&b| b == sep).unwrap_or(rest.len());
                let cmd = String::from_utf8_lossy(&rest[..sep_pos]).to_string();
                let crc = if sep_pos < rest.len() {
                    String::from_utf8_lossy(&rest[sep_pos + 1..]).to_string()
                } else {
                    String::new()
                };
                let full = String::from_utf8_lossy(&decrypted_bytes).to_string();
                (id, cmd, crc, full)
            } else {
                // No numeric command ID prefix (error responses like N01/BCK2,
                // bare commands like ACK, LCL, RMT, BOOTRES, etc.)
                let sep_pos = decrypted_bytes
                    .iter()
                    .position(|&b| b == sep)
                    .unwrap_or(decrypted_bytes.len());
                let cmd = String::from_utf8_lossy(&decrypted_bytes[..sep_pos]).to_string();
                let crc = if sep_pos < decrypted_bytes.len() {
                    String::from_utf8_lossy(&decrypted_bytes[sep_pos + 1..]).to_string()
                } else {
                    String::new()
                };
                let full = String::from_utf8_lossy(&decrypted_bytes).to_string();
                (String::new(), cmd, crc, full)
            }
        };

        let crc_valid = Self::is_valid_crc(&decrypted_str, &crc_value);

        DecodedMessage {
            cmd_id,
            command,
            crc_valid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pseudo_buffer_zero() {
        let buf = RiscoCrypt::create_pseudo_buffer(0);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_pseudo_buffer_nonzero() {
        let buf = RiscoCrypt::create_pseudo_buffer(1);
        // Should not be all zeros
        assert!(buf.iter().any(|&b| b != 0));
        // Buffer should be deterministic
        let buf2 = RiscoCrypt::create_pseudo_buffer(1);
        assert_eq!(buf, buf2);
    }

    #[test]
    fn test_pseudo_buffer_different_ids() {
        let buf1 = RiscoCrypt::create_pseudo_buffer(1);
        let buf2 = RiscoCrypt::create_pseudo_buffer(9999);
        assert_ne!(buf1, buf2);
    }

    #[test]
    fn test_crc_computation() {
        // Known test: CRC of empty data with init 0xFFFF should be specific value
        let crc = RiscoCrypt::compute_crc(b"");
        // With init 0xFFFF and no data, CRC remains 0xFFFF
        assert_eq!(crc, "FFFF");
    }

    #[test]
    fn test_crc_known_value() {
        // Test CRC with known input
        let crc = RiscoCrypt::compute_crc(b"01RMT=5678\x17");
        // Just verify it produces a 4-char hex string
        assert_eq!(crc.len(), 4);
        assert!(crc.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_encode_decode_roundtrip_no_crypt() {
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        let encoded = crypt.encode_command("RMT=5678", "01", None);

        // Should start with STX and end with ETX
        assert_eq!(encoded[0], STX);
        assert_eq!(*encoded.last().unwrap(), ETX);

        // Decode it back
        let decoded = crypt.decode_message(&encoded);
        assert_eq!(decoded.cmd_id, "01");
        assert_eq!(decoded.command, "RMT=5678");
        assert!(decoded.crc_valid);
    }

    #[test]
    fn test_encode_decode_roundtrip_with_crypt() {
        let mut crypt = RiscoCrypt::new(1234);
        crypt.set_crypt_enabled(true);

        let encoded = crypt.encode_command("RMT=5678", "01", None);

        // Should start with STX, then CRYPT indicator
        assert_eq!(encoded[0], STX);
        assert_eq!(encoded[1], CRYPT);
        assert_eq!(*encoded.last().unwrap(), ETX);

        // Decode it back
        let decoded = crypt.decode_message(&encoded);
        assert_eq!(decoded.cmd_id, "01");
        assert_eq!(decoded.command, "RMT=5678");
        assert!(decoded.crc_valid);
    }

    #[test]
    fn test_encode_decode_roundtrip_various_panel_ids() {
        for panel_id in [1, 100, 5678, 9999] {
            let mut crypt = RiscoCrypt::new(panel_id);
            crypt.set_crypt_enabled(true);

            let encoded = crypt.encode_command("CUSTLST?", "05", None);
            let decoded = crypt.decode_message(&encoded);

            assert_eq!(decoded.cmd_id, "05");
            assert_eq!(decoded.command, "CUSTLST?");
            assert!(
                decoded.crc_valid,
                "CRC failed for panel_id {}",
                panel_id
            );
        }
    }

    #[test]
    fn test_wrong_panel_id_fails_crc() {
        let mut crypt_enc = RiscoCrypt::new(1234);
        crypt_enc.set_crypt_enabled(true);
        let encoded = crypt_enc.encode_command("CUSTLST?", "01", None);

        // Decode with wrong panel ID
        let mut crypt_dec = RiscoCrypt::new(5678);
        crypt_dec.set_crypt_enabled(true);
        let decoded = crypt_dec.decode_message(&encoded);

        // CRC should NOT be valid with wrong key
        assert!(!decoded.crc_valid);
    }

    #[test]
    fn test_dle_escape_in_encrypted_data() {
        // Test that DLE bytes in encrypted output are properly escaped and unescaped
        // Use a panel ID and command that produces DLE bytes in the XOR output
        for panel_id in 1..100 {
            let mut crypt = RiscoCrypt::new(panel_id);
            crypt.set_crypt_enabled(true);

            let encoded = crypt.encode_command("ACK", "50", None);
            let decoded = crypt.decode_message(&encoded);

            assert_eq!(decoded.command, "ACK", "Failed for panel_id {}", panel_id);
            assert!(decoded.crc_valid, "CRC failed for panel_id {}", panel_id);
        }
    }

    #[test]
    fn test_encode_decode_all_panel_ids() {
        // Exhaustive roundtrip test for all 10,000 panel IDs.
        // This catches DLE+DLE escape bugs: some panel IDs produce encrypted
        // bytes equal to DLE (0x10), which must be DLE-escaped on the wire
        // and correctly recovered during decryption.
        let mut dle_dle_count = 0;
        for panel_id in 0..=9999u16 {
            let mut crypt = RiscoCrypt::new(panel_id);
            crypt.set_crypt_enabled(true);

            let encoded = crypt.encode_command("CUSTLST?", "01", None);

            // Track how many panel IDs produce DLE+DLE escapes
            for w in encoded.windows(2) {
                if w[0] == DLE && w[1] == DLE {
                    dle_dle_count += 1;
                    break;
                }
            }

            let decoded = crypt.decode_message(&encoded);

            assert_eq!(decoded.cmd_id, "01", "cmd_id wrong for panel_id {}", panel_id);
            assert_eq!(
                decoded.command, "CUSTLST?",
                "command wrong for panel_id {}",
                panel_id
            );
            assert!(decoded.crc_valid, "CRC failed for panel_id {}", panel_id);
        }
        // Sanity: at least some panel IDs should produce DLE+DLE escapes
        assert!(
            dle_dle_count > 0,
            "Expected some panel IDs to produce DLE+DLE escapes"
        );
    }

    #[test]
    fn test_force_crypt_parameter() {
        let mut crypt = RiscoCrypt::new(1234);
        crypt.set_crypt_enabled(false);

        // Force encryption on even though crypt_enabled is false
        let encoded = crypt.encode_command("TEST", "01", Some(true));
        assert_eq!(encoded[1], CRYPT);

        // Force encryption off even if we later set crypt_enabled to true
        crypt.set_crypt_enabled(true);
        let encoded = crypt.encode_command("TEST", "01", Some(false));
        assert_ne!(encoded[1], CRYPT);
    }

    /// Build an unencrypted frame for a payload with no cmd_id prefix.
    /// Format: STX + payload + SEP + CRC + ETX
    fn build_bare_frame(payload: &str) -> Vec<u8> {
        let data_with_sep = format!("{}{}", payload, char::from(SEP));
        let crc = RiscoCrypt::compute_crc(data_with_sep.as_bytes());
        let mut frame = vec![STX];
        frame.extend_from_slice(data_with_sep.as_bytes());
        frame.extend_from_slice(crc.as_bytes());
        frame.push(ETX);
        frame
    }

    #[test]
    fn test_decode_bare_ack_no_cmd_id() {
        // A bare "ACK" from the panel (no numeric prefix) must not be
        // misparsed as cmd_id="AC" / command="K".
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        let frame = build_bare_frame("ACK");
        let decoded = crypt.decode_message(&frame);
        assert_eq!(decoded.cmd_id, "");
        assert_eq!(decoded.command, "ACK");
        assert!(decoded.crc_valid);
    }

    #[test]
    fn test_decode_bare_bootres() {
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        let frame = build_bare_frame("BOOTRES");
        let decoded = crypt.decode_message(&frame);
        assert_eq!(decoded.cmd_id, "");
        assert_eq!(decoded.command, "BOOTRES");
        assert!(decoded.crc_valid);
    }

    #[test]
    fn test_decode_bare_lcl() {
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        let frame = build_bare_frame("LCL");
        let decoded = crypt.decode_message(&frame);
        assert_eq!(decoded.cmd_id, "");
        assert_eq!(decoded.command, "LCL");
        assert!(decoded.crc_valid);
    }

    #[test]
    fn test_decode_error_responses_still_work() {
        // N01 and BCK2 must still parse correctly after the refactor.
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        for payload in ["N01", "N11", "N25", "BCK2"] {
            let frame = build_bare_frame(payload);
            let decoded = crypt.decode_message(&frame);
            assert_eq!(decoded.cmd_id, "", "cmd_id should be empty for {}", payload);
            assert_eq!(decoded.command, payload, "command mismatch for {}", payload);
            assert!(decoded.crc_valid, "CRC should be valid for {}", payload);
        }
    }

    #[test]
    fn test_is_valid_crc_rejects_non_hex_crc() {
        // A garbled CRC should be rejected without panicking
        assert!(!RiscoCrypt::is_valid_crc("01ACK\x17", "ZZZZ"));
        assert!(!RiscoCrypt::is_valid_crc("01ACK\x17", "12"));
        assert!(!RiscoCrypt::is_valid_crc("01ACK\x17", ""));
        assert!(!RiscoCrypt::is_valid_crc("01ACK\x17", "12345"));
    }

    #[test]
    fn test_unencrypted_dle_bytes_preserved() {
        // Verify that DLE bytes (0x10) in unencrypted messages are NOT stripped.
        // Build a raw unencrypted frame containing a DLE byte in the command payload
        // and verify decode_message preserves it.
        let mut crypt = RiscoCrypt::new(0);
        crypt.set_crypt_enabled(false);

        // Build a frame manually: STX + "01" + cmd_with_dle + SEP + CRC + ETX
        // The command contains a byte that happens to equal DLE (0x10).
        // In a real scenario this is unusual but must not be silently dropped.
        let cmd_bytes = b"X\x10Y"; // 'X', DLE, 'Y'
        let full_cmd = format!("01{}{}", String::from_utf8_lossy(cmd_bytes), char::from(0x17));
        let crc = RiscoCrypt::compute_crc(full_cmd.as_bytes());

        let mut frame = vec![0x02]; // STX
        frame.extend_from_slice(full_cmd.as_bytes());
        frame.extend_from_slice(crc.as_bytes());
        frame.push(0x03); // ETX

        let decoded = crypt.decode_message(&frame);
        // The DLE byte should be preserved in the command
        assert_eq!(decoded.cmd_id, "01");
        assert_eq!(decoded.command.as_bytes(), cmd_bytes);
        assert!(decoded.crc_valid);
    }
}
