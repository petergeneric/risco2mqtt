// MIT License - Copyright (c) 2026 Peter Wright
// MQTT bridge

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use risco_lan_bridge::{
    ArmType, PanelConfig, PanelEvent, PanelType, PartitionStatusFlags, RiscoPanel, ZoneStatusFlags,
};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "risco2mqtt")]
#[command(about = "Bridge between a Risco alarm panel and MQTT")]
struct Cli {
    /// Path to the TOML configuration file
    #[arg(long, default_value = "config.toml")]
    config: String,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Config {
    panel: PanelToml,
    mqtt: MqttToml,
    #[serde(default, deserialize_with = "deserialize_zone_names")]
    zone_names: HashMap<u32, String>,
}

fn deserialize_zone_names<'de, D>(deserializer: D) -> Result<HashMap<u32, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let string_map: HashMap<String, String> = HashMap::deserialize(deserializer)?;
    string_map
        .into_iter()
        .map(|(k, v)| {
            k.parse::<u32>()
                .map(|id| (id, v))
                .map_err(|_| serde::de::Error::custom(format!("invalid zone ID: {k}")))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct PanelToml {
    /// Panel type name (e.g., "LightSys", "Agility"). Optional: when omitted,
    /// the panel type is auto-discovered from the PNLCNF response at connect time.
    #[serde(default)]
    panel_type: Option<String>,
    panel_ip: String,
    #[serde(default = "default_panel_port")]
    panel_port: u16,
    #[serde(default = "default_panel_id")]
    panel_id: u16,
    #[serde(default = "default_panel_password")]
    panel_password: String,
    #[serde(default)]
    discover_code: bool,
    #[serde(default = "default_reconnect_delay")]
    reconnect_delay_ms: u64,
    #[serde(default = "default_ntp_server")]
    ntp_server: String,
    #[serde(default = "default_ntp_port")]
    ntp_port: String,
    #[serde(default)]
    disable_risco_cloud: bool,
    #[serde(default = "default_watchdog_interval")]
    watchdog_interval_ms: u64,
}

fn default_panel_port() -> u16 {
    1000
}
fn default_panel_id() -> u16 {
    1
}
fn default_panel_password() -> String {
    "5678".to_string()
}
fn default_reconnect_delay() -> u64 {
    10000
}
fn default_ntp_server() -> String {
    "pool.ntp.org".to_string()
}
fn default_ntp_port() -> String {
    "123".to_string()
}
fn default_watchdog_interval() -> u64 {
    5000
}

#[derive(Debug, Deserialize)]
struct MqttToml {
    url: String,
    #[serde(default = "default_client_id")]
    client_id: String,
    #[serde(default = "default_subscribe_topic")]
    subscribe_topic: String,
    #[serde(default = "default_publish_topic")]
    publish_topic: String,
    #[serde(default = "default_snapshot_interval")]
    snapshot_interval_secs: u64,
}

fn default_client_id() -> String {
    "risco-bridge".to_string()
}
fn default_subscribe_topic() -> String {
    "risco/cmd".to_string()
}
fn default_publish_topic() -> String {
    "risco".to_string()
}
fn default_snapshot_interval() -> u64 {
    60
}

fn parse_panel_type(s: &str) -> Result<PanelType> {
    match s.to_lowercase().as_str() {
        "agility4" => Ok(PanelType::Agility4),
        "agility" => Ok(PanelType::Agility),
        "wicomm" => Ok(PanelType::WiComm),
        "wicommpro" => Ok(PanelType::WiCommPro),
        "lightsys" => Ok(PanelType::LightSys),
        "prosysplus" => Ok(PanelType::ProsysPlus),
        "gtplus" => Ok(PanelType::GTPlus),
        other => anyhow::bail!("Unknown panel type: {other}"),
    }
}

fn build_panel_config(toml: &PanelToml) -> Result<PanelConfig> {
    let panel_type = match &toml.panel_type {
        Some(pt) => parse_panel_type(pt)?,
        None => {
            info!("No panel_type configured; will auto-discover from panel");
            PanelType::Agility // Default; overridden by verify_panel_type auto-discovery
        }
    };
    Ok(PanelConfig::builder()
        .panel_type(panel_type)
        .panel_ip(&toml.panel_ip)
        .panel_port(toml.panel_port)
        .panel_password(&toml.panel_password)
        .panel_id(toml.panel_id)
        .discover_code(toml.discover_code)
        .reconnect_delay_ms(toml.reconnect_delay_ms)
        .ntp_server(&toml.ntp_server)
        .ntp_port(&toml.ntp_port)
        .disable_risco_cloud(toml.disable_risco_cloud)
        .watchdog_interval_ms(toml.watchdog_interval_ms)
        .build())
}

// ---------------------------------------------------------------------------
// MQTT JSON types
// ---------------------------------------------------------------------------

// Published messages — all share {now, op, ...} flat structure

#[derive(Serialize)]
struct MqttSnapshot {
    now: u64,
    op: String,
    state: MqttSnapshotState,
}

#[derive(Serialize)]
struct MqttSnapshotState {
    parts: Vec<MqttPartitionState>,
    zones: Vec<MqttZoneState>,
}

#[derive(Serialize)]
struct MqttZoneState {
    id: u32,
    name: String,
    arm: bool,
    open: bool,
    bypass: bool,
    alarm: bool,
    tamper: bool,
    trouble: bool,
}

#[derive(Serialize)]
struct MqttPartitionState {
    id: u32,
    name: String,
    #[serde(rename = "armAway")]
    arm_away: bool,
    #[serde(rename = "homeStay")]
    home_stay: bool,
    open: bool,
    ready: bool,
    alarm: bool,
    duress: bool,
    #[serde(rename = "falseCode")]
    false_code: bool,
    panic: bool,
    trouble: bool,
}

// Zone events: {now, op, zone}
#[derive(Serialize)]
struct MqttZoneEvent {
    now: u64,
    op: String,
    zone: u32,
}

// Partition events: {now, op, partition} or {now, op, partition, eventStr}
#[derive(Serialize)]
struct MqttPartitionEvent {
    now: u64,
    op: String,
    partition: u32,
    #[serde(skip_serializing_if = "Option::is_none", rename = "eventStr")]
    event_str: Option<String>,
}

// CMD_ACK response
#[derive(Serialize)]
struct MqttCmdAck {
    now: u64,
    op: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    src: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

// Simple event with just {now, op}
#[derive(Serialize)]
struct MqttSimpleEvent {
    now: u64,
    op: String,
}

// Inbound command (subscribed)
#[derive(Deserialize)]
struct MqttCommand {
    op: String,
    #[serde(default)]
    op_id: Option<String>,
    #[serde(default)]
    zone: Option<u32>,
    #[serde(default)]
    partition: Option<u32>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_epoch_ms() -> u64 {
    Utc::now().timestamp_millis() as u64
}

fn zone_label(zone_id: u32, panel_label: &str, overrides: &HashMap<u32, String>) -> String {
    if let Some(name) = overrides.get(&zone_id) {
        return name.clone();
    }
    panel_label.to_string()
}

async fn publish_json(client: &AsyncClient, topic: &str, payload: &impl Serialize, retain: bool) {
    match serde_json::to_string(payload) {
        Ok(json) => {
            if let Err(e) = client.publish(topic, QoS::AtLeastOnce, retain, json).await {
                error!("Failed to publish to {topic}: {e}");
            }
        }
        Err(e) => error!("Failed to serialize MQTT payload: {e}"),
    }
}

async fn publish_zone_event(client: &AsyncClient, topic: &str, op: &str, zone_id: u32) {
    let msg = MqttZoneEvent {
        now: now_epoch_ms(),
        op: op.to_string(),
        zone: zone_id,
    };
    publish_json(client, topic, &msg, false).await;
}

async fn publish_partition_event(
    client: &AsyncClient,
    topic: &str,
    op: &str,
    partition_id: u32,
    event_str: Option<String>,
) {
    let msg = MqttPartitionEvent {
        now: now_epoch_ms(),
        op: op.to_string(),
        partition: partition_id,
        event_str,
    };
    publish_json(client, topic, &msg, false).await;
}

async fn publish_cmd_ack(
    client: &AsyncClient,
    topic: &str,
    success: bool,
    src: Option<serde_json::Value>,
    data: Option<serde_json::Value>,
) {
    let msg = MqttCmdAck {
        now: now_epoch_ms(),
        op: "CMD_ACK".to_string(),
        success,
        src,
        data,
    };
    publish_json(client, topic, &msg, false).await;
}

async fn publish_simple_event(client: &AsyncClient, topic: &str, op: &str) {
    let msg = MqttSimpleEvent {
        now: now_epoch_ms(),
        op: op.to_string(),
    };
    publish_json(client, topic, &msg, false).await;
}

async fn build_snapshot(
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) -> MqttSnapshot {
    let zones_data = panel.zones().await;
    let parts_data = panel.partitions().await;

    let zones: Vec<MqttZoneState> = zones_data
        .iter()
        .filter(|z| !z.is_not_used())
        .map(|z| MqttZoneState {
            id: z.id,
            name: zone_label(z.id, &z.label, zone_names),
            arm: z.is_armed(),
            open: z.is_open(),
            bypass: z.is_bypassed(),
            alarm: z.is_alarm(),
            tamper: z.is_tamper(),
            trouble: z.is_trouble(),
        })
        .collect();

    let parts: Vec<MqttPartitionState> = parts_data
        .iter()
        .filter(|p| p.exists())
        .map(|p| MqttPartitionState {
            id: p.id,
            name: p.label.clone(),
            arm_away: p.is_armed(),
            home_stay: p.is_home_stay(),
            open: p.is_open(),
            ready: p.is_ready(),
            alarm: p.is_alarm(),
            duress: p.is_duress(),
            false_code: p.is_false_code(),
            panic: p.is_panic(),
            trouble: p.is_trouble(),
        })
        .collect();

    MqttSnapshot {
        now: now_epoch_ms(),
        op: "SNAPSHOT".to_string(),
        state: MqttSnapshotState { parts, zones },
    }
}

async fn publish_snapshot(
    client: &AsyncClient,
    topic: &str,
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) {
    let snapshot = build_snapshot(panel, zone_names).await;
    publish_json(client, topic, &snapshot, true).await;
}

// ---------------------------------------------------------------------------
// Panel event → MQTT
// ---------------------------------------------------------------------------

async fn handle_panel_event(
    event: PanelEvent,
    client: &AsyncClient,
    topic: &str,
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) {
    match event {
        PanelEvent::ZoneStatusChanged {
            zone_id,
            old_status: _,
            new_status,
            changed,
        } => {
            let label = if let Some(z) = panel.zone(zone_id).await {
                zone_label(zone_id, &z.label, zone_names)
            } else {
                format!("Zone {zone_id}")
            };

            let became_set = changed & new_status;
            let became_unset = changed & !new_status;

            info!("Zone {zone_id} ({label}) status changed");

            // Publish individual zone events per changed flag
            // Flags that became SET
            if became_set.contains(ZoneStatusFlags::OPEN) {
                publish_zone_event(client, topic, "ZONE_OPEN", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::ARMED) {
                publish_zone_event(client, topic, "ZONE_ARMED", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::ALARM) {
                publish_zone_event(client, topic, "ZONE_ALARM", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::TAMPER) {
                publish_zone_event(client, topic, "ZONE_TAMPER", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::TROUBLE) {
                publish_zone_event(client, topic, "ZONE_TROUBLE", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::LOW_BATTERY) {
                publish_zone_event(client, topic, "ZONE_BATTERY_LOW", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::BYPASS) {
                publish_zone_event(client, topic, "ZONE_BYPASSED", zone_id).await;
            }
            if became_set.contains(ZoneStatusFlags::LOST) {
                info!("Zone {zone_id} ({label}): Lost (no JS equivalent)");
            }
            if became_set.contains(ZoneStatusFlags::COMM_TROUBLE) {
                info!("Zone {zone_id} ({label}): CommTrouble (no JS equivalent)");
            }
            if became_set.contains(ZoneStatusFlags::SOAK_TEST) {
                info!("Zone {zone_id} ({label}): SoakTest (no JS equivalent)");
            }
            if became_set.contains(ZoneStatusFlags::HOURS_24) {
                info!("Zone {zone_id} ({label}): 24Hours (no JS equivalent)");
            }
            if became_set.contains(ZoneStatusFlags::NOT_USED) {
                info!("Zone {zone_id} ({label}): NotUsed (no JS equivalent)");
            }

            // Flags that became UNSET
            if became_unset.contains(ZoneStatusFlags::OPEN) {
                publish_zone_event(client, topic, "ZONE_CLOSE", zone_id).await;
            }
            if became_unset.contains(ZoneStatusFlags::ARMED) {
                publish_zone_event(client, topic, "ZONE_DISARMED", zone_id).await;
            }
            if became_unset.contains(ZoneStatusFlags::ALARM) {
                publish_zone_event(client, topic, "ZONE_STANDBY", zone_id).await;
            }
            if became_unset.contains(ZoneStatusFlags::TAMPER) {
                publish_zone_event(client, topic, "ZONE_HOLD", zone_id).await;
            }
            if became_unset.contains(ZoneStatusFlags::LOW_BATTERY) {
                publish_zone_event(client, topic, "ZONE_BATTERY_OK", zone_id).await;
            }
            if became_unset.contains(ZoneStatusFlags::BYPASS) {
                publish_zone_event(client, topic, "ZONE_UNBYPASSED", zone_id).await;
            }
        }

        PanelEvent::PartitionStatusChanged {
            partition_id,
            old_status: _,
            new_status,
            changed,
        } => {
            let label = if let Some(p) = panel.partition(partition_id).await {
                p.label.clone()
            } else {
                format!("Partition {partition_id}")
            };

            let became_set = changed & new_status;
            let became_unset = changed & !new_status;

            // Build eventStr summary
            let set_names: Vec<&str> =
                PartitionStatusFlags::set_event_names(changed, new_status);
            let unset_names: Vec<&str> =
                PartitionStatusFlags::unset_event_names(changed, new_status);
            let event_str = format!("set=[{}] unset=[{}]", set_names.join(","), unset_names.join(","));

            info!("Partition {partition_id} ({label}) changed: {event_str}");

            // Publish PART_STATUS_CHANGE with eventStr
            publish_partition_event(
                client,
                topic,
                "PART_STATUS_CHANGE",
                partition_id,
                Some(event_str),
            )
            .await;

            // Publish individual partition events per changed flag
            // Flags that became SET
            if became_set.contains(PartitionStatusFlags::ALARM) {
                publish_partition_event(client, topic, "PART_ALARM", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::DURESS) {
                publish_partition_event(client, topic, "PART_DURESS_ALARM", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::FALSE_CODE) {
                publish_partition_event(client, topic, "PART_CODE_FALSE", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::PANIC) {
                publish_partition_event(client, topic, "PART_PANIC", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::ARMED) {
                publish_partition_event(client, topic, "PART_ARMSTATE_ARMED", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::HOME_STAY) {
                publish_partition_event(client, topic, "PART_ARMSTATE_ARMED_HOME_STAY", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::READY) {
                publish_partition_event(client, topic, "PART_READY", partition_id, None).await;
            }
            if became_set.contains(PartitionStatusFlags::TROUBLE) {
                publish_partition_event(client, topic, "PART_TROUBLE", partition_id, None).await;
            }

            // Flags that became UNSET
            if became_unset.contains(PartitionStatusFlags::ALARM) {
                publish_partition_event(client, topic, "PART_ALARM_STANDBY", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::DURESS) {
                publish_partition_event(client, topic, "PART_DURESS_FREE", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::FALSE_CODE) {
                publish_partition_event(client, topic, "PART_CODE_OK", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::PANIC) {
                publish_partition_event(client, topic, "PART_NO_PANIC", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::ARMED) {
                publish_partition_event(client, topic, "PART_ARMSTATE_DISARMED", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::HOME_STAY) {
                publish_partition_event(client, topic, "PART_ARMSTATE_DISARMED_HOME_STAY", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::READY) {
                publish_partition_event(client, topic, "PART_NOT_READY", partition_id, None).await;
            }
            if became_unset.contains(PartitionStatusFlags::TROUBLE) {
                publish_partition_event(client, topic, "PART_TROUBLE_OK", partition_id, None).await;
            }
        }

        PanelEvent::SystemStatusChanged { .. } => {
            publish_simple_event(client, topic, "SYSTEM_STATUS").await;
        }

        PanelEvent::Connected => {
            info!("Panel connected");
            publish_simple_event(client, topic, "CONNECTED").await;
        }

        PanelEvent::Disconnected => {
            warn!("Panel disconnected");
            publish_simple_event(client, topic, "DISCONNECTED").await;
        }

        PanelEvent::SystemInitComplete => {
            info!("System init complete — publishing snapshot");
            publish_snapshot(client, topic, panel, zone_names).await;
        }

        PanelEvent::PanelData(data) => {
            panel.route_panel_data(&data).await;
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// MQTT command handler
// ---------------------------------------------------------------------------

async fn handle_command(
    payload_str: &str,
    cmd: MqttCommand,
    client: &AsyncClient,
    topic: &str,
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) {
    // Parse the raw payload as a JSON value for the CMD_ACK src field
    let src_json = serde_json::from_str::<serde_json::Value>(payload_str).ok();

    match cmd.op.as_str() {
        "SNAPSHOT" => {
            info!("Command: SNAPSHOT");
            let snapshot = build_snapshot(panel, zone_names).await;
            let snapshot_value = serde_json::to_value(&snapshot).ok();
            publish_snapshot(client, topic, panel, zone_names).await;
            publish_cmd_ack(client, topic, true, src_json, snapshot_value).await;
        }

        "PING" => {
            info!("Command: PING");
            publish_cmd_ack(client, topic, true, src_json, None).await;
        }

        "ARM_AWAY" => {
            let id = cmd.partition.unwrap_or(1);
            info!("Command: ARM_AWAY partition {id}");
            let success = match panel.arm_partition(id, ArmType::Away).await {
                Ok(true) => {
                    info!("ARM_AWAY partition {id}: success");
                    true
                }
                Ok(false) => {
                    warn!("ARM_AWAY partition {id}: panel returned NACK");
                    false
                }
                Err(e) => {
                    error!("ARM_AWAY partition {id} failed: {e}");
                    false
                }
            };
            publish_cmd_ack(client, topic, success, src_json, None).await;
        }

        "ARM_HOME_STAY" => {
            let id = cmd.partition.unwrap_or(1);
            info!("Command: ARM_HOME_STAY partition {id}");
            let success = match panel.arm_partition(id, ArmType::Stay).await {
                Ok(true) => {
                    info!("ARM_HOME_STAY partition {id}: success");
                    true
                }
                Ok(false) => {
                    warn!("ARM_HOME_STAY partition {id}: panel returned NACK");
                    false
                }
                Err(e) => {
                    error!("ARM_HOME_STAY partition {id} failed: {e}");
                    false
                }
            };
            publish_cmd_ack(client, topic, success, src_json, None).await;
        }

        "DISARM" => {
            let id = cmd.partition.unwrap_or(1);
            info!("Command: DISARM partition {id}");
            let success = match panel.disarm_partition(id).await {
                Ok(true) => {
                    info!("DISARM partition {id}: success");
                    true
                }
                Ok(false) => {
                    warn!("DISARM partition {id}: panel returned NACK");
                    false
                }
                Err(e) => {
                    error!("DISARM partition {id} failed: {e}");
                    false
                }
            };
            publish_cmd_ack(client, topic, success, src_json, None).await;
        }

        "ZONE_BYPASS_ENABLE" => {
            let id = match cmd.zone {
                Some(id) => id,
                None => {
                    warn!("ZONE_BYPASS_ENABLE: missing zone");
                    publish_cmd_ack(client, topic, false, src_json, None).await;
                    return;
                }
            };
            info!("Command: ZONE_BYPASS_ENABLE zone {id}");
            if let Some(z) = panel.zone(id).await {
                if z.is_bypassed() {
                    info!("Zone {id} already bypassed");
                    publish_cmd_ack(client, topic, true, src_json, None).await;
                    return;
                }
            }
            let success = match panel.toggle_bypass_zone(id).await {
                Ok(true) => {
                    info!("ZONE_BYPASS_ENABLE zone {id}: success");
                    true
                }
                Ok(false) => {
                    warn!("ZONE_BYPASS_ENABLE zone {id}: panel returned NACK");
                    false
                }
                Err(e) => {
                    error!("ZONE_BYPASS_ENABLE zone {id} failed: {e}");
                    false
                }
            };
            publish_cmd_ack(client, topic, success, src_json, None).await;
        }

        "ZONE_BYPASS_DISABLE" => {
            let id = match cmd.zone {
                Some(id) => id,
                None => {
                    warn!("ZONE_BYPASS_DISABLE: missing zone");
                    publish_cmd_ack(client, topic, false, src_json, None).await;
                    return;
                }
            };
            info!("Command: ZONE_BYPASS_DISABLE zone {id}");
            if let Some(z) = panel.zone(id).await {
                if !z.is_bypassed() {
                    info!("Zone {id} already not bypassed");
                    publish_cmd_ack(client, topic, true, src_json, None).await;
                    return;
                }
            }
            let success = match panel.toggle_bypass_zone(id).await {
                Ok(true) => {
                    info!("ZONE_BYPASS_DISABLE zone {id}: success");
                    true
                }
                Ok(false) => {
                    warn!("ZONE_BYPASS_DISABLE zone {id}: panel returned NACK");
                    false
                }
                Err(e) => {
                    error!("ZONE_BYPASS_DISABLE zone {id} failed: {e}");
                    false
                }
            };
            publish_cmd_ack(client, topic, success, src_json, None).await;
        }

        other => {
            warn!("Unknown command: {other}");
            publish_cmd_ack(client, topic, false, src_json, None).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load config
    let config_text =
        std::fs::read_to_string(&cli.config).context("Failed to read config file")?;
    let config: Config = toml::from_str(&config_text).context("Failed to parse config file")?;

    let panel_config = build_panel_config(&config.panel)?;
    let publish_topic = config.mqtt.publish_topic.clone();
    let subscribe_topic = config.mqtt.subscribe_topic.clone();
    let snapshot_interval_secs = config.mqtt.snapshot_interval_secs;
    let zone_names = Arc::new(config.zone_names);

    // Connect to panel
    info!("Connecting to Risco panel at {}:{}", config.panel.panel_ip, config.panel.panel_port);
    let panel = Arc::new(Mutex::new(RiscoPanel::connect(panel_config).await?));
    info!("Panel connected and initialized");

    // Set up MQTT
    let (host, port) = parse_mqtt_url(&config.mqtt.url)?;
    let mut mqtt_opts = MqttOptions::new(&config.mqtt.client_id, &host, port);
    mqtt_opts.set_keep_alive(Duration::from_secs(30));
    let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 256);

    // Subscribe to command topic
    client
        .subscribe(&subscribe_topic, QoS::AtLeastOnce)
        .await
        .context("Failed to subscribe to MQTT topic")?;
    info!("MQTT: subscribed to {subscribe_topic}");

    // Publish initial snapshot
    {
        let panel_lock = panel.lock().await;
        publish_snapshot(&client, &publish_topic, &*panel_lock, &zone_names).await;
    }

    // Task 1: Panel event listener
    let panel_events = Arc::clone(&panel);
    let client_events = client.clone();
    let topic_events = publish_topic.clone();
    let zn_events = Arc::clone(&zone_names);
    let event_rx = {
        let panel_lock = panel.lock().await;
        panel_lock.subscribe()
    };
    let event_handle = tokio::spawn(async move {
        let mut rx = event_rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let panel_lock = panel_events.lock().await;
                    handle_panel_event(
                        event,
                        &client_events,
                        &topic_events,
                        &*panel_lock,
                        &zn_events,
                    )
                    .await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event receiver lagged, missed {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("Event channel closed");
                    break;
                }
            }
        }
    });

    // Task 2: MQTT event loop (receives messages, handles commands)
    let panel_cmds = Arc::clone(&panel);
    let client_cmds = client.clone();
    let topic_cmds = publish_topic.clone();
    let zn_cmds = Arc::clone(&zone_names);
    let sub_topic = subscribe_topic.clone();
    let mqtt_handle = tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    if msg.topic == sub_topic {
                        let payload = String::from_utf8_lossy(&msg.payload);
                        info!("MQTT command received: {payload}");
                        match serde_json::from_str::<MqttCommand>(&payload) {
                            Ok(cmd) => {
                                let panel_lock = panel_cmds.lock().await;
                                handle_command(
                                    &payload,
                                    cmd,
                                    &client_cmds,
                                    &topic_cmds,
                                    &*panel_lock,
                                    &zn_cmds,
                                )
                                .await;
                            }
                            Err(e) => {
                                warn!("Failed to parse MQTT command: {e}");
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    error!("MQTT event loop error: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Task 3: Snapshot timer
    let panel_snap = Arc::clone(&panel);
    let client_snap = client.clone();
    let topic_snap = publish_topic.clone();
    let zn_snap = Arc::clone(&zone_names);
    let snap_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(snapshot_interval_secs));
        // Skip the first immediate tick (we already published an initial snapshot)
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let panel_lock = panel_snap.lock().await;
            publish_snapshot(&client_snap, &topic_snap, &*panel_lock, &zn_snap).await;
        }
    });

    // Wait for Ctrl+C
    info!("MQTT bridge running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    // Abort tasks
    event_handle.abort();
    mqtt_handle.abort();
    snap_handle.abort();

    // Disconnect panel
    match Arc::try_unwrap(panel) {
        Ok(mutex) => {
            let mut p = mutex.into_inner();
            if let Err(e) = p.disconnect().await {
                warn!("Error disconnecting panel: {e}");
            }
        }
        Err(_arc) => {
            warn!("Could not unwrap panel Arc for clean disconnect (tasks still hold references)");
        }
    }

    info!("Shutdown complete");
    Ok(())
}

/// Parse an MQTT URL like "mqtt://host:port" into (host, port).
fn parse_mqtt_url(url: &str) -> Result<(String, u16)> {
    let stripped = url
        .strip_prefix("mqtt://")
        .or_else(|| url.strip_prefix("tcp://"))
        .unwrap_or(url);

    let (host, port_str) = stripped
        .rsplit_once(':')
        .context("MQTT URL must be in format mqtt://host:port")?;

    let port: u16 = port_str
        .parse()
        .context("Invalid MQTT port number")?;

    Ok((host.to_string(), port))
}
