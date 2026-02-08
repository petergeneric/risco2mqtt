// Schema validation tests for MQTT wire format
//
// These tests construct JSON values directly (independent of Rust structs)
// and validate them against the JSON Schema files in schemas/mqtt/.

use serde_json::json;

fn load_schema(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/schemas/mqtt/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read schema {path}: {e}"));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("Failed to parse schema {path}: {e}"))
}

fn build_validator(schema_name: &str) -> jsonschema::Validator {
    let schema = load_schema(schema_name);
    jsonschema::options()
        .with_retriever(LocalRetriever)
        .build(&schema)
        .unwrap_or_else(|e| panic!("Failed to compile schema {schema_name}: {e}"))
}

fn validate(schema_name: &str, instance: &serde_json::Value) {
    let validator = build_validator(schema_name);
    let errors: Vec<_> = validator.iter_errors(instance).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors.iter().map(|e| format!("  - {e}")).collect();
        panic!(
            "Schema validation failed for {schema_name}:\n{}\nInstance: {}",
            msgs.join("\n"),
            serde_json::to_string_pretty(instance).unwrap()
        );
    }
}

fn validate_fails(schema_name: &str, instance: &serde_json::Value) {
    let validator = build_validator(schema_name);
    assert!(
        !validator.is_valid(instance),
        "Expected schema validation to fail for {schema_name}, but it passed.\nInstance: {}",
        serde_json::to_string_pretty(instance).unwrap()
    );
}

// Retriever that loads $ref schemas from the local filesystem
struct LocalRetriever;

impl jsonschema::Retrieve for LocalRetriever {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        let uri_str = uri.as_str();
        let schema_dir = format!("{}/schemas/mqtt/", env!("CARGO_MANIFEST_DIR"));

        // Extract the schema filename from various URI forms:
        // - "json-schema:///zone_state.schema.json"
        // - "file:///path/to/zone_state.schema.json"
        // - "zone_state.schema.json"
        let filename = if let Some(rest) = uri_str.strip_prefix("json-schema:///") {
            rest
        } else if let Some(path) = uri_str.strip_prefix("file://") {
            // For file:// URIs, use the path directly
            let text = std::fs::read_to_string(path)?;
            return Ok(serde_json::from_str(&text)?);
        } else {
            uri_str
        };

        let path = format!("{schema_dir}{filename}");
        if std::path::Path::new(&path).exists() {
            let text = std::fs::read_to_string(&path)?;
            return Ok(serde_json::from_str(&text)?);
        }
        Err(format!("Cannot retrieve schema: {uri_str}").into())
    }
}

// =========================================================================
// Snapshot
// =========================================================================

#[test]
fn snapshot_valid() {
    validate(
        "snapshot.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "SNAPSHOT",
            "state": {
                "parts": [{
                    "id": 1,
                    "name": "House",
                    "armAway": false,
                    "homeStay": false,
                    "open": true,
                    "ready": false,
                    "alarm": false,
                    "duress": false,
                    "falseCode": false,
                    "panic": false,
                    "trouble": false
                }],
                "zones": [{
                    "id": 1,
                    "name": "Front Door",
                    "arm": true,
                    "open": false,
                    "bypass": false,
                    "alarm": false,
                    "tamper": false,
                    "trouble": false
                }]
            }
        }),
    );
}

#[test]
fn snapshot_empty_arrays() {
    validate(
        "snapshot.schema.json",
        &json!({
            "now": 0,
            "op": "SNAPSHOT",
            "state": {
                "parts": [],
                "zones": []
            }
        }),
    );
}

#[test]
fn snapshot_multiple_zones_and_partitions() {
    validate(
        "snapshot.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "SNAPSHOT",
            "state": {
                "parts": [
                    {
                        "id": 1, "name": "House",
                        "armAway": true, "homeStay": false, "open": false,
                        "ready": true, "alarm": false, "duress": false,
                        "falseCode": false, "panic": false, "trouble": false
                    },
                    {
                        "id": 2, "name": "Garage",
                        "armAway": false, "homeStay": true, "open": false,
                        "ready": true, "alarm": false, "duress": false,
                        "falseCode": false, "panic": false, "trouble": false
                    }
                ],
                "zones": [
                    {
                        "id": 1, "name": "Front Door",
                        "arm": true, "open": false, "bypass": false,
                        "alarm": false, "tamper": false, "trouble": false
                    },
                    {
                        "id": 5, "name": "Kitchen Window",
                        "arm": false, "open": true, "bypass": true,
                        "alarm": false, "tamper": false, "trouble": false
                    }
                ]
            }
        }),
    );
}

#[test]
fn snapshot_wrong_op() {
    validate_fails(
        "snapshot.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "WRONG",
            "state": { "parts": [], "zones": [] }
        }),
    );
}

#[test]
fn snapshot_missing_state() {
    validate_fails(
        "snapshot.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "SNAPSHOT"
        }),
    );
}

#[test]
fn snapshot_timestamp_string_rejected() {
    validate_fails(
        "snapshot.schema.json",
        &json!({
            "now": "2026-01-01T00:00:00Z",
            "op": "SNAPSHOT",
            "state": { "parts": [], "zones": [] }
        }),
    );
}

// =========================================================================
// Zone state
// =========================================================================

#[test]
fn zone_state_valid() {
    validate(
        "zone_state.schema.json",
        &json!({
            "id": 1,
            "name": "Front Door",
            "arm": true,
            "open": false,
            "bypass": false,
            "alarm": false,
            "tamper": false,
            "trouble": false
        }),
    );
}

#[test]
fn zone_state_missing_field() {
    validate_fails(
        "zone_state.schema.json",
        &json!({
            "id": 1,
            "name": "Front Door",
            "arm": true,
            "open": false
            // missing bypass, alarm, tamper, trouble
        }),
    );
}

#[test]
fn zone_state_extra_field_rejected() {
    validate_fails(
        "zone_state.schema.json",
        &json!({
            "id": 1,
            "name": "Front Door",
            "arm": true,
            "open": false,
            "bypass": false,
            "alarm": false,
            "tamper": false,
            "trouble": false,
            "extra": true
        }),
    );
}

#[test]
fn zone_state_old_field_names_rejected() {
    // Using old Rust field names (label, armed, bypassed) should fail
    validate_fails(
        "zone_state.schema.json",
        &json!({
            "id": 1,
            "label": "Front Door",
            "armed": true,
            "open": false,
            "bypassed": false,
            "alarm": false,
            "tamper": false,
            "trouble": false
        }),
    );
}

// =========================================================================
// Partition state
// =========================================================================

#[test]
fn partition_state_valid() {
    validate(
        "partition_state.schema.json",
        &json!({
            "id": 1,
            "name": "House",
            "armAway": true,
            "homeStay": false,
            "open": false,
            "ready": true,
            "alarm": false,
            "duress": false,
            "falseCode": false,
            "panic": false,
            "trouble": false
        }),
    );
}

#[test]
fn partition_state_missing_duress() {
    validate_fails(
        "partition_state.schema.json",
        &json!({
            "id": 1,
            "name": "House",
            "armAway": true,
            "homeStay": false,
            "open": false,
            "ready": true,
            "alarm": false,
            // missing duress, falseCode, panic
            "trouble": false
        }),
    );
}

#[test]
fn partition_state_old_field_names_rejected() {
    validate_fails(
        "partition_state.schema.json",
        &json!({
            "id": 1,
            "label": "House",
            "armed": true,
            "home_stay": false,
            "open": false,
            "ready": true,
            "alarm": false,
            "trouble": false
        }),
    );
}

// =========================================================================
// Zone events
// =========================================================================

#[test]
fn zone_event_open() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_OPEN", "zone": 5 }),
    );
}

#[test]
fn zone_event_close() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_CLOSE", "zone": 1 }),
    );
}

#[test]
fn zone_event_armed() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_ARMED", "zone": 3 }),
    );
}

#[test]
fn zone_event_disarmed() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_DISARMED", "zone": 3 }),
    );
}

#[test]
fn zone_event_alarm() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_ALARM", "zone": 2 }),
    );
}

#[test]
fn zone_event_standby() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_STANDBY", "zone": 2 }),
    );
}

#[test]
fn zone_event_tamper() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_TAMPER", "zone": 7 }),
    );
}

#[test]
fn zone_event_hold() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_HOLD", "zone": 7 }),
    );
}

#[test]
fn zone_event_trouble() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_TROUBLE", "zone": 4 }),
    );
}

#[test]
fn zone_event_battery_low() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_BATTERY_LOW", "zone": 6 }),
    );
}

#[test]
fn zone_event_battery_ok() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_BATTERY_OK", "zone": 6 }),
    );
}

#[test]
fn zone_event_bypassed() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_BYPASSED", "zone": 8 }),
    );
}

#[test]
fn zone_event_unbypassed() {
    validate(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_UNBYPASSED", "zone": 8 }),
    );
}

#[test]
fn zone_event_unknown_op_rejected() {
    validate_fails(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_LOST", "zone": 1 }),
    );
}

#[test]
fn zone_event_missing_zone_rejected() {
    validate_fails(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_OPEN" }),
    );
}

#[test]
fn zone_event_extra_field_rejected() {
    validate_fails(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_OPEN", "zone": 1, "label": "Front Door" }),
    );
}

// =========================================================================
// Partition events
// =========================================================================

#[test]
fn partition_event_alarm() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ALARM", "partition": 1 }),
    );
}

#[test]
fn partition_event_alarm_standby() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ALARM_STANDBY", "partition": 1 }),
    );
}

#[test]
fn partition_event_duress_alarm() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_DURESS_ALARM", "partition": 1 }),
    );
}

#[test]
fn partition_event_duress_free() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_DURESS_FREE", "partition": 1 }),
    );
}

#[test]
fn partition_event_code_false() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_CODE_FALSE", "partition": 1 }),
    );
}

#[test]
fn partition_event_code_ok() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_CODE_OK", "partition": 1 }),
    );
}

#[test]
fn partition_event_panic() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_PANIC", "partition": 2 }),
    );
}

#[test]
fn partition_event_no_panic() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_NO_PANIC", "partition": 2 }),
    );
}

#[test]
fn partition_event_armed() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ARMSTATE_ARMED", "partition": 1 }),
    );
}

#[test]
fn partition_event_disarmed() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ARMSTATE_DISARMED", "partition": 1 }),
    );
}

#[test]
fn partition_event_armed_home_stay() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ARMSTATE_ARMED_HOME_STAY", "partition": 1 }),
    );
}

#[test]
fn partition_event_disarmed_home_stay() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ARMSTATE_DISARMED_HOME_STAY", "partition": 1 }),
    );
}

#[test]
fn partition_event_ready() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_READY", "partition": 1 }),
    );
}

#[test]
fn partition_event_not_ready() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_NOT_READY", "partition": 1 }),
    );
}

#[test]
fn partition_event_trouble() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_TROUBLE", "partition": 1 }),
    );
}

#[test]
fn partition_event_trouble_ok() {
    validate(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_TROUBLE_OK", "partition": 1 }),
    );
}

#[test]
fn partition_event_status_change_with_event_str() {
    validate(
        "partition_event.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "PART_STATUS_CHANGE",
            "partition": 1,
            "eventStr": "set=[Armed] unset=[]"
        }),
    );
}

#[test]
fn partition_event_unknown_op_rejected() {
    validate_fails(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_FIRE", "partition": 1 }),
    );
}

#[test]
fn partition_event_missing_partition_rejected() {
    validate_fails(
        "partition_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "PART_ALARM" }),
    );
}

// =========================================================================
// CMD_ACK
// =========================================================================

#[test]
fn cmd_ack_success() {
    validate(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "CMD_ACK",
            "success": true
        }),
    );
}

#[test]
fn cmd_ack_failure() {
    validate(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "CMD_ACK",
            "success": false
        }),
    );
}

#[test]
fn cmd_ack_with_src() {
    validate(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "CMD_ACK",
            "success": true,
            "src": { "op": "PING" }
        }),
    );
}

#[test]
fn cmd_ack_with_data() {
    validate(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "CMD_ACK",
            "success": true,
            "data": {
                "now": 1738900000000_u64,
                "op": "SNAPSHOT",
                "state": { "parts": [], "zones": [] }
            }
        }),
    );
}

#[test]
fn cmd_ack_wrong_op_rejected() {
    validate_fails(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "PONG",
            "success": true
        }),
    );
}

#[test]
fn cmd_ack_missing_success_rejected() {
    validate_fails(
        "command_ack.schema.json",
        &json!({
            "now": 1738900000000_u64,
            "op": "CMD_ACK"
        }),
    );
}

// =========================================================================
// Inbound commands
// =========================================================================

#[test]
fn command_snapshot() {
    validate(
        "command.schema.json",
        &json!({ "op": "SNAPSHOT" }),
    );
}

#[test]
fn command_ping() {
    validate(
        "command.schema.json",
        &json!({ "op": "PING" }),
    );
}

#[test]
fn command_arm_away_with_partition() {
    validate(
        "command.schema.json",
        &json!({ "op": "ARM_AWAY", "partition": 1 }),
    );
}

#[test]
fn command_arm_home_stay() {
    validate(
        "command.schema.json",
        &json!({ "op": "ARM_HOME_STAY", "partition": 2 }),
    );
}

#[test]
fn command_disarm() {
    validate(
        "command.schema.json",
        &json!({ "op": "DISARM", "partition": 1 }),
    );
}

#[test]
fn command_zone_bypass_enable() {
    validate(
        "command.schema.json",
        &json!({ "op": "ZONE_BYPASS_ENABLE", "zone": 5 }),
    );
}

#[test]
fn command_zone_bypass_disable() {
    validate(
        "command.schema.json",
        &json!({ "op": "ZONE_BYPASS_DISABLE", "zone": 3 }),
    );
}

#[test]
fn command_with_op_id() {
    validate(
        "command.schema.json",
        &json!({ "op": "PING", "op_id": "abc-123" }),
    );
}

#[test]
fn command_unknown_op_rejected() {
    validate_fails(
        "command.schema.json",
        &json!({ "op": "EXPLODE" }),
    );
}

#[test]
fn command_missing_op_rejected() {
    validate_fails(
        "command.schema.json",
        &json!({ "zone": 1 }),
    );
}

#[test]
fn command_old_field_names_rejected() {
    // Old Rust field names (zone_id, partition_id) should fail
    validate_fails(
        "command.schema.json",
        &json!({ "op": "ARM_AWAY", "partition_id": 1 }),
    );
}

#[test]
fn command_extra_field_rejected() {
    validate_fails(
        "command.schema.json",
        &json!({ "op": "PING", "extra": true }),
    );
}

// =========================================================================
// Negative tests — wrong types
// =========================================================================

#[test]
fn zone_event_zone_as_string_rejected() {
    validate_fails(
        "zone_event.schema.json",
        &json!({ "now": 1738900000000_u64, "op": "ZONE_OPEN", "zone": "five" }),
    );
}

#[test]
fn snapshot_now_as_float_rejected() {
    // JSON Schema "integer" — some validators allow floats; our schemas should reject
    validate_fails(
        "snapshot.schema.json",
        &json!({
            "now": 1738900000000.5,
            "op": "SNAPSHOT",
            "state": { "parts": [], "zones": [] }
        }),
    );
}

#[test]
fn partition_state_arm_away_as_string_rejected() {
    validate_fails(
        "partition_state.schema.json",
        &json!({
            "id": 1, "name": "House",
            "armAway": "yes", "homeStay": false,
            "open": false, "ready": true, "alarm": false,
            "duress": false, "falseCode": false, "panic": false,
            "trouble": false
        }),
    );
}
