//! Example: Connect to a Risco panel and print device status.

use risco_lan_bridge::{PanelConfig, PanelType, RiscoPanel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = PanelConfig::builder()
        .panel_type(PanelType::Agility)
        .panel_ip("192.168.0.100")
        .panel_port(1000)
        .panel_password("5678")
        .panel_id(1)
        .discover_code(true)
        .build();

    println!("Connecting to panel...");
    let mut panel = RiscoPanel::connect(config).await?;

    // Print zones
    let zones = panel.zones().await;
    println!("\n--- Zones ({}) ---", zones.len());
    for zone in &zones {
        if !zone.is_not_used() {
            println!(
                "  Zone {:3}: {:20} type={:?} tech={} open={} armed={} bypass={}",
                zone.id,
                zone.label,
                zone.zone_type,
                zone.technology.description(),
                zone.is_open(),
                zone.is_armed(),
                zone.is_bypassed(),
            );
        }
    }

    // Print partitions
    let partitions = panel.partitions().await;
    println!("\n--- Partitions ({}) ---", partitions.len());
    for part in &partitions {
        if part.exists() {
            println!(
                "  Partition {:2}: {:20} armed={} stay={} ready={} open={}",
                part.id,
                part.label,
                part.is_armed(),
                part.is_home_stay(),
                part.is_ready(),
                part.is_open(),
            );
        }
    }

    // Print outputs
    let outputs = panel.outputs().await;
    println!("\n--- Outputs ({}) ---", outputs.len());
    for output in &outputs {
        if output.user_usable {
            println!(
                "  Output {:3}: {:20} type={} active={}",
                output.id,
                output.label,
                output.output_type.description(),
                output.active,
            );
        }
    }

    // Print system
    if let Some(sys) = panel.system().await {
        println!("\n--- System ---");
        println!("  Label: {}", sys.label);
        println!("  Prog mode: {}", sys.is_prog_mode());
        println!("  Low battery: {}", sys.is_low_battery());
        println!("  AC trouble: {}", sys.is_ac_trouble());
    }

    println!("\nPress Ctrl+C to disconnect...");
    tokio::signal::ctrl_c().await?;
    panel.disconnect().await?;
    println!("Disconnected.");

    Ok(())
}
