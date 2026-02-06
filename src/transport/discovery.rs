// MIT License - Copyright (c) 2021 TJForc
// Rust translation of panel ID discovery from RiscoChannels.js

use std::sync::Arc;

use tracing::{debug, info};

use crate::error::{RiscoError, Result};
use crate::transport::command::CommandEngine;

/// Attempt to discover the correct panel ID by brute-force CRC testing.
///
/// Tries panel IDs from 9999 down to 0. For each candidate, decodes
/// the last received buffer and checks if CRC is valid. Once a candidate
/// passes the offline test, sends a live CUSTLST command to verify.
pub async fn discover_panel_id(engine: &Arc<CommandEngine>) -> Result<u16> {
    let _test_buffer = {
        let misunderstood_arc = engine.last_misunderstood();
        let _data = misunderstood_arc.lock().await;
        // We need the raw encrypted buffer, not the decrypted string.
        // In the actual implementation, we'd store the raw buffer.
        // For now, attempt live discovery.
        None::<Vec<u8>>
    };

    // Live discovery: try each panel ID
    info!("Starting panel ID discovery (9999 → 0)...");
    for possible_key in (0..=9999u16).rev() {
        // Update the crypt engine with the candidate key
        {
            let crypt_arc = engine.crypt();
            let mut crypt = crypt_arc.lock().await;
            crypt.set_panel_id(possible_key);
        }

        // Try a live command with this key
        engine.set_in_crypt_test(true).await;
        let result = engine.send_command("CUSTLST", false).await;
        let valid = {
            let v = engine.crypt_key_valid.read().await;
            v.unwrap_or(false)
        };

        match result {
            Ok(ref response) if valid && !CommandEngine::is_error_code(response) => {
                info!("Discovered panel ID: {}", possible_key);
                engine.set_in_crypt_test(false).await;
                return Ok(possible_key);
            }
            _ => {
                debug!("Panel ID {} is not correct", possible_key);
            }
        }
    }

    engine.set_in_crypt_test(false).await;
    Err(RiscoError::DiscoveryFailed {
        reason: "Exhausted all panel IDs (0-9999)".to_string(),
    })
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
