# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

risco-lan-bridge is a Node.js library for direct TCP/IP communication with Risco alarm control panels (Agility, WiComm, WiCommPro, LightSYS, ProsysPlus, GTPlus). It acts as a bridge between applications and Risco panels, bypassing the RiscoCloud service. No external npm dependencies — uses only Node.js built-in modules (`net`, `events`).

## Commands

There is no build step (pure JavaScript) and no test suite configured. The entry point is `index.js` which re-exports `lib/RiscoPanel.js`.

```bash
npm install          # install (no deps, but sets up node_modules)
node examples/Full_Options.js  # run an example (requires a real panel)
```

## Architecture

The library follows a layered event-driven architecture:

```
RiscoPanel (lib/RiscoPanel.js)
  → RiscoComm (lib/RiscoComm.js)
    → Socket Layer (lib/RiscoChannels.js)
      → RCrypt (lib/RCrypt.js)
    → Device Models (lib/Devices/)
```

**RiscoPanel.js** — Top-level controller. Contains a base `RiscoPanel` class and 6 subclasses (one per panel type: `Agility`, `WiComm`, `WiCommPro`, `LightSys`, `ProsysPlus`, `GTPlus`). Each subclass defines panel-specific limits (max zones, partitions, outputs). Exposes `Connect()`, `Disconnect()`, `ArmPart()`, `DisarmPart()`, `ToggleBypassZone()`, `ToggleOutput()`. Runs a 5-second watchdog keep-alive.

**RiscoComm.js** — Communication handler. Manages connection initialization sequence, panel type verification, firmware version detection, and data fetching (zones, partitions, outputs, system). Emits events when device states change from panel updates.

**RiscoChannels.js** — TCP socket layer with two modes:
- `Risco_DirectTCP_Socket` — Direct TCP connection to the panel (default port 1000). Authenticates with RMT command after a mandatory 10-second delay post-connect.
- `Risco_ProxyTCP_Socket` — Acts as middleman between the panel and RiscoCloud (www.riscocloud.com:33000), allowing simultaneous local and cloud access.
- Both extend `Risco_Base_Socket` which handles command send/retry, CRC validation, and encryption key discovery.

**RCrypt.js** — Implements the proprietary Risco encryption protocol. Uses Panel ID (0001-9999) to generate an XOR cipher. Handles CRC16 checksums and escape character encoding (STX/ETX/DLE).

**lib/Devices/** — Device model classes (`Zones.js`, `Partitions.js`, `Outputs.js`, `System.js`). Each extends Array, tracks real-time state via property setters, and emits change events.

**constants.js** — Enums for zone types, CRC lookup table, and protocol constants.

## Protocol Notes

- Message format: `[STX][ENC?][CMDID][DATA][CRC_MARKER][CRC][ETX]`
- Commands: `RMT=<password>` (auth), `LCL` (local encrypted session), `ARM=<id>`, `STAY=<id>`, `DISARM=<id>`, `ZBYPAS=<id>`, `CLOCK` (keep-alive)
- Panel ID brute-force discovery goes from 9999 down to 0 when `DiscoverCode` is enabled
- Mono-socket panels (IPC/RW132IP) require disabling RiscoCloud for direct connection

## Key Events

All classes use Node.js EventEmitter. Main events to know:
- `SystemInitComplete` — All devices discovered, panel ready
- `PanelCommReady` — Communication layer established
- `NewZoneStatusFromPanel` / `NewOutputStatusFromPanel` / `NewPartitionStatusFromPanel` — Device state changes from panel
- `Disconnected`, `PanelConnected` — Socket lifecycle
- `BadCode`, `BadCryptKey` — Trigger auto-discovery when enabled
