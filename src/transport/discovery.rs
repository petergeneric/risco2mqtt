// MIT License - Copyright (c) 2021 TJForc
// Rust translation of panel ID discovery from RiscoChannels.js

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::crypto::RiscoCrypt;
use crate::error::{RiscoError, Result};
use crate::protocol::Command;
use crate::transport::command::CommandEngine;

/// Attempt to discover the correct panel ID by offline CRC testing.
///
/// Uses the captured raw encrypted response buffer from a CUSTLST? command
/// to test all 10,000 panel IDs (9999 down to 0) purely in memory. For each
/// candidate, clones the buffer, creates a fresh crypto engine, decodes the
/// message, and checks CRC validity. Once a candidate passes the offline
/// test, sends a single live CUSTLST command to confirm.
///
/// This mirrors the Node.js implementation in RiscoChannels.js and completes
/// in seconds rather than the ~83 minutes a live-command-per-ID approach takes.
pub async fn discover_panel_id(engine: &Arc<CommandEngine>) -> Result<u16> {
    let encrypted_buffer = {
        let arc = engine.last_received_buffer();
        let buf = arc.lock().await;
        buf.clone()
    };

    let encrypted_buffer = match encrypted_buffer {
        Some(buf) if !buf.is_empty() => buf,
        _ => {
            return Err(RiscoError::DiscoveryFailed {
                reason: "No encrypted response buffer available for offline discovery".to_string(),
            });
        }
    };

    info!("Starting offline panel ID discovery (9999 → 0)...");

    // Offline phase: test each panel ID against the stored encrypted buffer
    let mut candidate: Option<u16> = None;
    for possible_key in (0..=9999u16).rev() {
        // Create a fresh crypto engine with the candidate panel ID
        let mut crypt = RiscoCrypt::new(possible_key);
        crypt.set_crypt_enabled(true);

        // Decode a fresh copy of the encrypted buffer
        let decoded = crypt.decode_message(&encrypted_buffer);

        if decoded.crc_valid {
            debug!("Panel ID {} is a possible candidate (CRC valid)", possible_key);
            candidate = Some(possible_key);
            break;
        }
    }

    let possible_key = match candidate {
        Some(key) => key,
        None => {
            return Err(RiscoError::DiscoveryFailed {
                reason: "Exhausted all panel IDs (0-9999) — no CRC match".to_string(),
            });
        }
    };

    // Live verification: set the discovered key and send a real command to confirm
    info!("Verifying candidate panel ID {} with live command...", possible_key);
    {
        let crypt_arc = engine.crypt();
        let mut crypt = crypt_arc.lock().await;
        crypt.set_panel_id(possible_key);
    }

    engine.set_in_crypt_test(true).await;
    let result = engine.send_command(&Command::CustomerListVerify, false).await;
    let valid = {
        let v = engine.crypt_key_valid.read().await;
        v.unwrap_or(false)
    };
    engine.set_in_crypt_test(false).await;

    match result {
        Ok(ref response) if valid && !CommandEngine::is_error_code(response) => {
            warn!("Discovered panel ID: {}", possible_key);
            Ok(possible_key)
        }
        _ => {
            warn!("Offline candidate {} failed live verification", possible_key);
            Err(RiscoError::DiscoveryFailed {
                reason: format!(
                    "Candidate panel ID {} passed offline CRC but failed live verification",
                    possible_key
                ),
            })
        }
    }
}

/// Attempt to discover the correct access code by brute-force.
///
/// The panel treats leading-zero-padded codes as distinct from their unpadded
/// forms (e.g. "5678" != "05678" != "005678"), so we must test:
///   - Pass 1 (length=1): all 1,000,000 codes 0–999999 with natural representation
///   - Pass 2 (length=2): "00"–"09" (leading-zero variants not covered by pass 1)
///   - Pass 3 (length=3): "000"–"099"
///   - Pass 4–6: similarly, up to "000000"–"099999"
///
/// This mirrors the original Node.js discovery in RiscoChannels.js.
/// Total codes tested: 1,111,110. This is a slow process that can take many hours.
pub async fn discover_access_code(
    engine: &Arc<CommandEngine>,
    _panel_ip: &str,
    _panel_port: u16,
) -> Result<String> {
    info!("Starting access code discovery...");
    let max_password: u32 = 999999;

    for password_length in 1..=6u32 {
        for password in 0..=max_password {
            // Check connectivity before each attempt
            if !*engine.connected_flag().read().await {
                return Err(RiscoError::Disconnected);
            }

            // Skip passwords whose natural digit count already reaches this length —
            // they were already tested in pass 1. Only relevant for pass 2+.
            if password_length > 1 && password.to_string().len() >= password_length as usize {
                continue;
            }

            let padded = format!("{:0>width$}", password, width = password_length as usize);
            let rmt_cmd = Command::Rmt { password: padded.clone() };

            match engine.send_command(&rmt_cmd, false).await {
                Ok(response) if response.contains("ACK") => {
                    info!("Discovered access code: {}", padded);
                    return Ok(padded);
                }
                Err(RiscoError::Disconnected) => {
                    warn!("Socket disconnected during access code discovery");
                    return Err(RiscoError::Disconnected);
                }
                _ => {
                    debug!("Access code {} is not correct", padded);
                }
            }

            // Reset sequence ID for next attempt
            *engine.sequence_id().lock().await = 1;
        }
    }

    Err(RiscoError::DiscoveryFailed {
        reason: "Exhausted all access codes".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use crate::crypto::RiscoCrypt;

    /// Simulate the offline discovery: encode a message with a known panel ID,
    /// then iterate candidates until one produces a valid CRC.
    #[test]
    fn test_offline_panel_id_discovery() {
        let real_panel_id: u16 = 4321;

        // Simulate the panel sending an encrypted error response to CUSTLST?
        let encrypted_buffer = {
            let mut crypt = RiscoCrypt::new(real_panel_id);
            crypt.set_crypt_enabled(true);
            // Panel responds with an error code (no cmd_id prefix) — use encode
            // with a normal command ID to simulate a realistic encrypted frame.
            crypt.encode_command("N01", "01", None)
        };

        // Offline brute-force: try each panel ID
        let mut discovered: Option<u16> = None;
        for candidate in (0..=9999u16).rev() {
            let mut crypt = RiscoCrypt::new(candidate);
            crypt.set_crypt_enabled(true);
            let decoded = crypt.decode_message(&encrypted_buffer);
            if decoded.crc_valid {
                discovered = Some(candidate);
                break;
            }
        }

        assert_eq!(discovered, Some(real_panel_id));
    }

    /// Verify that the access code discovery iteration covers the expected
    /// password set: all 1M natural codes plus leading-zero variants.
    /// This mirrors the Node.js discovery in RiscoChannels.js.
    #[test]
    fn test_access_code_iteration_coverage() {
        use std::collections::HashSet;

        let max_password: u32 = 999999;
        let mut passwords = Vec::new();

        // Mirror the exact iteration logic from discover_access_code
        for password_length in 1..=6u32 {
            for password in 0..=max_password {
                if password_length > 1 && password.to_string().len() >= password_length as usize {
                    continue;
                }
                let padded = format!("{:0>width$}", password, width = password_length as usize);
                passwords.push(padded);
            }
        }

        // Total: 1,000,000 + 10 + 100 + 1,000 + 10,000 + 100,000 = 1,111,110
        assert_eq!(passwords.len(), 1_111_110);

        // No duplicates
        let unique: HashSet<&String> = passwords.iter().collect();
        assert_eq!(unique.len(), passwords.len());

        // Verify specific leading-zero codes are present
        assert!(unique.contains(&"0".to_string()));
        assert!(unique.contains(&"00".to_string()));
        assert!(unique.contains(&"000".to_string()));
        assert!(unique.contains(&"0000".to_string()));
        assert!(unique.contains(&"00000".to_string()));
        assert!(unique.contains(&"000000".to_string()));
        assert!(unique.contains(&"007".to_string()));
        assert!(unique.contains(&"05678".to_string()));

        // Verify all natural-length codes are present
        assert!(unique.contains(&"1".to_string()));
        assert!(unique.contains(&"42".to_string()));
        assert!(unique.contains(&"999999".to_string()));

        // Verify ordering: pass 1 starts with "0", pass 2 starts with "00"
        assert_eq!(passwords[0], "0");
        assert_eq!(passwords[1_000_000], "00");
        assert_eq!(passwords[1_000_010], "000");
    }

    /// Verify that only the correct panel ID produces a valid CRC on the
    /// encrypted buffer — no false positives among nearby IDs.
    #[test]
    fn test_offline_discovery_no_false_positives_nearby() {
        let real_panel_id: u16 = 7777;

        let encrypted_buffer = {
            let mut crypt = RiscoCrypt::new(real_panel_id);
            crypt.set_crypt_enabled(true);
            crypt.encode_command("CUSTLST?", "05", None)
        };

        // Check a range around the real ID — only the real one should match
        let start = real_panel_id.saturating_sub(50);
        let end = (real_panel_id + 50).min(9999);
        for candidate in start..=end {
            let mut crypt = RiscoCrypt::new(candidate);
            crypt.set_crypt_enabled(true);
            let decoded = crypt.decode_message(&encrypted_buffer);
            if candidate == real_panel_id {
                assert!(decoded.crc_valid, "Real panel ID should pass CRC");
            } else {
                assert!(!decoded.crc_valid, "Panel ID {} should not pass CRC", candidate);
            }
        }
    }
}
