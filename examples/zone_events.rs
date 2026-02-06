//! Example: Subscribe to zone status events and print changes.

use risco_lan_bridge::{PanelConfig, PanelEvent, PanelType, RiscoPanel, ZoneStatusFlags};

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
    let mut events = panel.subscribe();

    println!("Listening for zone events (Ctrl+C to stop)...\n");

    loop {
        tokio::select! {
            event = events.recv() => {
                match event {
                    Ok(PanelEvent::ZoneStatusChanged { zone_id, old_status: _, new_status, changed }) => {
                        println!("Zone {} status changed:", zone_id);
                        let set_events = ZoneStatusFlags::set_event_names(changed, new_status);
                        let unset_events = ZoneStatusFlags::unset_event_names(changed, new_status);
                        for e in set_events {
                            println!("  + {}", e);
                        }
                        for e in unset_events {
                            println!("  - {}", e);
                        }
                    }
                    Ok(PanelEvent::Disconnected) => {
                        println!("Panel disconnected!");
                        break;
                    }
                    Ok(event) => {
                        println!("Event: {:?}", event);
                    }
                    Err(e) => {
                        println!("Event channel error: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nDisconnecting...");
                break;
            }
        }
    }

    panel.disconnect().await?;
    Ok(())
}
