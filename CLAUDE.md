# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

risco2mqtt is a Rust library for direct TCP/IP communication with Risco alarm control panels (Agility, WiComm, WiCommPro, LightSYS, ProsysPlus, GTPlus), bypassing the RiscoCloud service, bridging them to high-level operations via MQTT. The original TypeScript reference implementation/documentation is available as a git submodule in `risco-lan-bridge/`.

## Commands

```bash
cargo build                          # debug build
cargo build --release                # release build
cargo test                           # run all tests
cargo test <test_name>               # run a single test
cargo clippy                         # lint
cargo check                          # quick type check
cargo run -- --config config.toml    # run risco2mqtt bridge
```

## Architecture

The Rust library (`src/`) is a layered async system built on tokio:

```
risco2mqtt (src/main.rs) — MQTT bridge binary
  └── RiscoPanel (src/panel.rs) — public API
        ├── RiscoComm (src/comm.rs) — connection init, device fetch, panel verification
        │     ├── Transport (src/transport/)
        │     │     ├── DirectTcpTransport (direct.rs) — TCP to panel port 1000
        │     │     ├── ProxyTcpTransport (proxy.rs) — middleman to RiscoCloud
        │     │     ├── CommandEngine (command.rs) — send/retry, sequence IDs 1-49
        │     │     └── Discovery (discovery.rs) — panel ID brute-force 9999→0
        │     └── RiscoCrypt (src/crypto.rs) — LFSR XOR cipher, CRC16, DLE escaping
        ├── Device collections: Vec<Zone>, Vec<Partition>, Vec<Output>, Option<MBSystem>
        │     └── (src/devices/) — each with bitflags-based status tracking
        ├── EventChannel (src/event.rs) — tokio broadcast for PanelEvent
        ├── Watchdog task — sends CLOCK every 5s
        └── DataListener task — handles unsolicited status updates (seq ID 50+)
```

**Key Rust modules:**
- `config.rs` — `PanelConfig` builder, `PanelType` enum (6 variants), `ArmType`, `SocketMode`
- `protocol.rs` — message parsing, status update routing
- `error.rs` — `RiscoError` enum with `thiserror`
- `constants.rs` — CRC lookup table, protocol bytes (STX=0x02, ETX=0x03, DLE=0x10, CRC_MARKER=0x17)

## Protocol Notes

- Message format: `[STX][ENC?][CMDID][DATA][CRC_MARKER][CRC][ETX]`
- Commands: `RMT=<password>` (auth), `LCL` (encrypted session), `ARM=<id>`, `STAY=<id>`, `DISARM=<id>`, `ZBYPAS=<id>`, `ACTUO<id>` (output), `CLOCK` (keep-alive), `PNLCNF` (panel type query)
- Mandatory 10-second delay after TCP connect before sending RMT auth
- Panel ID discovery brute-forces 9999→0 when enabled
- Mono-socket panels (IPC/RW132IP) require disabling RiscoCloud for direct connection
- Command sequence IDs cycle 1-49; unsolicited panel messages use 50+
- Connection retry uses exponential backoff: `base_delay * 2^min(attempt-1, 4)`

## MQTT Bridge (risco2mqtt)

The binary in `src/main.rs` reads `config.toml` (see `config.toml.example`), connects to both the Risco panel and an MQTT broker, and bridges events bidirectionally. JSON schemas for MQTT messages are in `schemas/mqtt/`.

## Key Events (PanelEvent enum in Rust)

- `ZoneStatus` / `PartitionStatus` / `OutputStatus` / `SystemStatus` — device state changes
- `SystemInitComplete` — all devices discovered, panel ready
- `Connected` / `Disconnected` — socket lifecycle

## Panel Type Limits

Panel types define max zones/partitions/outputs. LightSYS and ProsysPlus limits are firmware-dependent (e.g., LightSYS FW >= 3.0 gets 50 zones/32 outputs vs 32/14; ProsysPlus FW >= 1.2.0.7 gets 128 zones vs 64).
