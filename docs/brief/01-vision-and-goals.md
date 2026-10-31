# Vision, goals, non-goals, definition of success

> Source: ORIGINAL_BRIEF.md §1, §2, §21

## 1. Vision

Build a lightweight, open-source, local-first NIBE heat-pump gateway in Rust. Replace the legacy NibePi runtime (Node-RED/Node.js and associated services) with a purpose-built appliance exposing a secure HTTP API and browser administration UI. It must work without Home Assistant, MQTT, cloud services, or an external server. A custom dashboard on an Android tablet can consume its API separately.

**User experience:** Flash an SD card once → power on → join the gateway's secure temporary Wi-Fi → select home Wi-Fi and configure credentials in a browser → discover gateway on LAN → monitor/control supported heat-pump functions → update and debug over the network without reflashing.

**Developer experience:** Develop and verify locally with simulated hardware and replayed F730 protocol captures. Cross-compile and deploy over SSH when physical testing is needed.

## 2. Goals and non-goals

### Goals
- Minimal, reliable Rust daemon suitable for ARMv6 single-core / 512 MB RAM.
- Correct F-series MODBUS 40 accessory protocol, starting with F730.
- Versioned HTTP API, OpenAPI schema, live telemetry, safe validated controls.
- Embedded responsive web administration UI and authenticated setup.
- Secure first-boot Wi-Fi access-point provisioning, mDNS, and recovery.
- Versioned register catalog, passive discovery, safe read-only probing where supported, and register explorer.
- Deterministic local simulation, real-frame replay, and observable verification surfaces.
- Signed application OTA, rollback, SSH dev deployment, protected diagnostics.
- Reproducible 32-bit Raspberry Pi OS Lite image with read-only root and persistent config.
- Extensible model adapters and optional future Matter/Apple Home support.

### Non-goals for initial release
- General-purpose home automation, automation rules, cloud backend, mandatory MQTT, Node-RED, Docker, or heavyweight databases.
- Native phone apps or rich historical analytics hosted on the Pi.
- Matter certification or full OS OTA before a tested boot-level recovery strategy exists.

## 21. Definition of success

A user can flash a prepared SD image, configure the Pi through a secure temporary Wi-Fi portal, connect to a supported NIBE heat pump, inspect verified live readings and safely operate approved controls through a documented local API and admin UI. Developers can simulate, replay, test, debug, deploy, update, and roll back without reflashing. The appliance operates reliably on the Pi Zero WH without Home Assistant, Node-RED, MQTT, or cloud services.

**Product principle:** *Flash once. Configure in a browser. Develop locally. Deploy over the network. Operate independently.*
