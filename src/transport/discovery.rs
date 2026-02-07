// MIT License - Copyright (c) 2021 TJForc
// Rust translation of panel ID discovery from RiscoChannels.js

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::crypto::RiscoCrypt;
use crate::error::{RiscoError, Result};
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
    let result = engine.send_command("CUSTLST", false).await;
    let valid = {
        let v = engine.crypt_key_valid.read().await;
        v.unwrap_or(false)
    };
    engine.set_in_crypt_test(false).await;

    match result {
        Ok(ref response) if valid && !CommandEngine::is_error_code(response) => {
            info!("Discovered panel ID: {}", possible_key);
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
/// Tries passwords from 0 to 999999, with variable-length padding
/// (1 to 6 digits). This is a slow process that can take many hours.
pub async fn discover_access_code(
    engine: &Arc<CommandEngine>,
    _panel_ip: &str,
    _panel_port: u16,
) -> Result<String> {
    info!("Starting access code discovery...");
    let max_password: u32 = 999999;

    for password_length in 1..=6u32 {
        let start = 0;
        for password in start..=max_password {
            // Only test passwords that fit within the current length window
            if password_length > 1 && password.to_string().len() >= password_length as usize {
                continue;
            }

            let padded = format!("{:0>width$}", password, width = password_length as usize);
            let rmt_cmd = format!("RMT={}", padded);

            match engine.send_command(&rmt_cmd, false).await {
                Ok(response) if response.contains("ACK") => {
                    info!("Discovered access code: {}", padded);
                    return Ok(padded);
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
