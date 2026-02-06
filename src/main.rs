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
    #[serde(default)]
    zone_names: HashMap<u32, String>,
}

#[derive(Debug, Deserialize)]
struct PanelToml {
    panel_type: String,
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
    let panel_type = parse_panel_type(&toml.panel_type)?;
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
        .build())
}

// ---------------------------------------------------------------------------
// MQTT JSON types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MqttMessage {
    #[serde(rename = "type")]
    msg_type: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<MqttEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<MqttSnapshot>,
}

#[derive(Serialize)]
struct MqttEvent {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    zone_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zone_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partition_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partition_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events_set: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events_unset: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct MqttSnapshot {
    zones: Vec<MqttZone>,
    partitions: Vec<MqttPartition>,
}

#[derive(Serialize)]
struct MqttZone {
    id: u32,
    label: String,
    open: bool,
    armed: bool,
    alarm: bool,
    tamper: bool,
    trouble: bool,
    bypassed: bool,
}

#[derive(Serialize)]
struct MqttPartition {
    id: u32,
    label: String,
    armed: bool,
    home_stay: bool,
    ready: bool,
    open: bool,
    alarm: bool,
    trouble: bool,
}

#[derive(Deserialize)]
struct MqttCommand {
    command: String,
    #[serde(default)]
    partition_id: Option<u32>,
    #[serde(default)]
    zone_id: Option<u32>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn zone_label(zone_id: u32, panel_label: &str, overrides: &HashMap<u32, String>) -> String {
    if let Some(name) = overrides.get(&zone_id) {
        return name.clone();
    }
    panel_label.to_string()
}

async fn publish_event(client: &AsyncClient, topic: &str, event: MqttEvent) {
    let msg = MqttMessage {
        msg_type: "event".to_string(),
        timestamp: now_iso(),
        event: Some(event),
        snapshot: None,
    };
    if let Ok(payload) = serde_json::to_string(&msg) {
        if let Err(e) = client.publish(topic, QoS::AtLeastOnce, false, payload).await {
            error!("Failed to publish event: {e}");
        }
    }
}

async fn build_snapshot(
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) -> MqttSnapshot {
    let zones_data = panel.zones().await;
    let parts_data = panel.partitions().await;

    let zones: Vec<MqttZone> = zones_data
        .iter()
        .filter(|z| !z.is_not_used())
        .map(|z| MqttZone {
            id: z.id,
            label: zone_label(z.id, &z.label, zone_names),
            open: z.is_open(),
            armed: z.is_armed(),
            alarm: z.is_alarm(),
            tamper: z.is_tamper(),
            trouble: z.is_trouble(),
            bypassed: z.is_bypassed(),
        })
        .collect();

    let partitions: Vec<MqttPartition> = parts_data
        .iter()
        .filter(|p| p.exists())
        .map(|p| MqttPartition {
            id: p.id,
            label: p.label.clone(),
            armed: p.is_armed(),
            home_stay: p.is_home_stay(),
            ready: p.is_ready(),
            open: p.is_open(),
            alarm: p.is_alarm(),
            trouble: p.is_trouble(),
        })
        .collect();

    MqttSnapshot { zones, partitions }
}

async fn publish_snapshot(
    client: &AsyncClient,
    topic: &str,
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) {
    let snapshot = build_snapshot(panel, zone_names).await;
    let msg = MqttMessage {
        msg_type: "snapshot".to_string(),
        timestamp: now_iso(),
        event: None,
        snapshot: Some(snapshot),
    };
    match serde_json::to_string(&msg) {
        Ok(payload) => {
            if let Err(e) = client.publish(topic, QoS::AtLeastOnce, true, payload).await {
                error!("Failed to publish snapshot: {e}");
            }
        }
        Err(e) => error!("Failed to serialize snapshot: {e}"),
    }
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

            let events_set: Vec<String> = ZoneStatusFlags::set_event_names(changed, new_status)
                .into_iter()
                .map(String::from)
                .collect();
            let events_unset: Vec<String> =
                ZoneStatusFlags::unset_event_names(changed, new_status)
                    .into_iter()
                    .map(String::from)
                    .collect();

            let status = serde_json::json!({
                "open": new_status.contains(ZoneStatusFlags::OPEN),
                "armed": new_status.contains(ZoneStatusFlags::ARMED),
                "alarm": new_status.contains(ZoneStatusFlags::ALARM),
                "tamper": new_status.contains(ZoneStatusFlags::TAMPER),
                "trouble": new_status.contains(ZoneStatusFlags::TROUBLE),
                "bypassed": new_status.contains(ZoneStatusFlags::BYPASS),
                "low_battery": new_status.contains(ZoneStatusFlags::LOW_BATTERY),
                "lost": new_status.contains(ZoneStatusFlags::LOST),
            });

            info!(
                "Zone {} ({}) changed: set={:?} unset={:?}",
                zone_id, label, events_set, events_unset
            );

            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "zone_status".to_string(),
                    zone_id: Some(zone_id),
                    zone_label: Some(label),
                    partition_id: None,
                    partition_label: None,
                    events_set: Some(events_set),
                    events_unset: Some(events_unset),
                    status: Some(status),
                },
            )
            .await;
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

            let events_set: Vec<String> =
                PartitionStatusFlags::set_event_names(changed, new_status)
                    .into_iter()
                    .map(String::from)
                    .collect();
            let events_unset: Vec<String> =
                PartitionStatusFlags::unset_event_names(changed, new_status)
                    .into_iter()
                    .map(String::from)
                    .collect();

            let status = serde_json::json!({
                "armed": new_status.contains(PartitionStatusFlags::ARMED),
                "home_stay": new_status.contains(PartitionStatusFlags::HOME_STAY),
                "ready": new_status.contains(PartitionStatusFlags::READY),
                "open": new_status.contains(PartitionStatusFlags::OPEN),
                "alarm": new_status.contains(PartitionStatusFlags::ALARM),
                "trouble": new_status.contains(PartitionStatusFlags::TROUBLE),
            });

            info!(
                "Partition {} ({}) changed: set={:?} unset={:?}",
                partition_id, label, events_set, events_unset
            );

            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "partition_status".to_string(),
                    zone_id: None,
                    zone_label: None,
                    partition_id: Some(partition_id),
                    partition_label: Some(label),
                    events_set: Some(events_set),
                    events_unset: Some(events_unset),
                    status: Some(status),
                },
            )
            .await;
        }

        PanelEvent::SystemStatusChanged { .. } => {
            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "system_status".to_string(),
                    zone_id: None,
                    zone_label: None,
                    partition_id: None,
                    partition_label: None,
                    events_set: None,
                    events_unset: None,
                    status: None,
                },
            )
            .await;
        }

        PanelEvent::Connected => {
            info!("Panel connected");
            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "connected".to_string(),
                    zone_id: None,
                    zone_label: None,
                    partition_id: None,
                    partition_label: None,
                    events_set: None,
                    events_unset: None,
                    status: None,
                },
            )
            .await;
        }

        PanelEvent::Disconnected => {
            warn!("Panel disconnected");
            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "disconnected".to_string(),
                    zone_id: None,
                    zone_label: None,
                    partition_id: None,
                    partition_label: None,
                    events_set: None,
                    events_unset: None,
                    status: None,
                },
            )
            .await;
        }

        PanelEvent::SystemInitComplete => {
            info!("System init complete — publishing snapshot");
            publish_snapshot(client, topic, panel, zone_names).await;
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// MQTT command handler
// ---------------------------------------------------------------------------

async fn handle_command(
    cmd: MqttCommand,
    client: &AsyncClient,
    topic: &str,
    panel: &RiscoPanel,
    zone_names: &HashMap<u32, String>,
) {
    match cmd.command.as_str() {
        "SNAPSHOT" => {
            info!("Command: SNAPSHOT");
            publish_snapshot(client, topic, panel, zone_names).await;
        }

        "PING" => {
            info!("Command: PING");
            publish_event(
                client,
                topic,
                MqttEvent {
                    kind: "pong".to_string(),
                    zone_id: None,
                    zone_label: None,
                    partition_id: None,
                    partition_label: None,
                    events_set: None,
                    events_unset: None,
                    status: None,
                },
            )
            .await;
        }

        "ARM_AWAY" => {
            let id = cmd.partition_id.unwrap_or(1);
            info!("Command: ARM_AWAY partition {id}");
            match panel.arm_partition(id, ArmType::Away).await {
                Ok(true) => info!("ARM_AWAY partition {id}: success"),
                Ok(false) => warn!("ARM_AWAY partition {id}: panel returned NACK"),
                Err(e) => error!("ARM_AWAY partition {id} failed: {e}"),
            }
        }

        "ARM_HOME_STAY" => {
            let id = cmd.partition_id.unwrap_or(1);
            info!("Command: ARM_HOME_STAY partition {id}");
            match panel.arm_partition(id, ArmType::Stay).await {
                Ok(true) => info!("ARM_HOME_STAY partition {id}: success"),
                Ok(false) => warn!("ARM_HOME_STAY partition {id}: panel returned NACK"),
                Err(e) => error!("ARM_HOME_STAY partition {id} failed: {e}"),
            }
        }

        "DISARM" => {
            let id = cmd.partition_id.unwrap_or(1);
            info!("Command: DISARM partition {id}");
            match panel.disarm_partition(id).await {
                Ok(true) => info!("DISARM partition {id}: success"),
                Ok(false) => warn!("DISARM partition {id}: panel returned NACK"),
                Err(e) => error!("DISARM partition {id} failed: {e}"),
            }
        }

        "ZONE_BYPASS_ENABLE" => {
            let id = match cmd.zone_id {
                Some(id) => id,
                None => {
                    warn!("ZONE_BYPASS_ENABLE: missing zone_id");
                    return;
                }
            };
            info!("Command: ZONE_BYPASS_ENABLE zone {id}");
            if let Some(z) = panel.zone(id).await {
                if z.is_bypassed() {
                    info!("Zone {id} already bypassed");
                    return;
                }
            }
            match panel.toggle_bypass_zone(id).await {
                Ok(true) => info!("ZONE_BYPASS_ENABLE zone {id}: success"),
                Ok(false) => warn!("ZONE_BYPASS_ENABLE zone {id}: panel returned NACK"),
                Err(e) => error!("ZONE_BYPASS_ENABLE zone {id} failed: {e}"),
            }
        }

        "ZONE_BYPASS_DISABLE" => {
            let id = match cmd.zone_id {
                Some(id) => id,
                None => {
                    warn!("ZONE_BYPASS_DISABLE: missing zone_id");
                    return;
                }
            };
            info!("Command: ZONE_BYPASS_DISABLE zone {id}");
            if let Some(z) = panel.zone(id).await {
                if !z.is_bypassed() {
                    info!("Zone {id} already not bypassed");
                    return;
                }
            }
            match panel.toggle_bypass_zone(id).await {
                Ok(true) => info!("ZONE_BYPASS_DISABLE zone {id}: success"),
                Ok(false) => warn!("ZONE_BYPASS_DISABLE zone {id}: panel returned NACK"),
                Err(e) => error!("ZONE_BYPASS_DISABLE zone {id} failed: {e}"),
            }
        }

        other => {
            warn!("Unknown command: {other}");
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
