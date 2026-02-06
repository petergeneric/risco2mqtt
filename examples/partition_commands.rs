//! Example: Arm and disarm partitions.

use risco_lan_bridge::{ArmType, PanelConfig, PanelType, RiscoPanel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = PanelConfig::builder()
        .panel_type(PanelType::Agility)
        .panel_ip("192.168.0.100")
        .panel_password("5678")
        .panel_id(1)
        .build();

    let mut panel = RiscoPanel::connect(config).await?;

    // Show current partition states
    let partitions = panel.partitions().await;
    for part in &partitions {
        if part.exists() {
            println!(
                "Partition {}: {} (armed={}, stay={})",
                part.id,
                part.label,
                part.is_armed(),
                part.is_home_stay()
            );
        }
    }

    // Arm partition 1 in stay mode
    println!("\nArming partition 1 in stay mode...");
    match panel.arm_partition(1, ArmType::Stay).await {
        Ok(true) => println!("Partition 1 armed (stay)"),
        Ok(false) => println!("Partition 1 arm command not acknowledged"),
        Err(e) => println!("Error arming partition 1: {}", e),
    }

    // Wait a bit then disarm
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    println!("\nDisarming partition 1...");
    match panel.disarm_partition(1).await {
        Ok(true) => println!("Partition 1 disarmed"),
        Ok(false) => println!("Partition 1 disarm command not acknowledged"),
        Err(e) => println!("Error disarming partition 1: {}", e),
    }

    panel.disconnect().await?;
    Ok(())
}
