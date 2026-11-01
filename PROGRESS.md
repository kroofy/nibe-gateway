# NIBE Gateway — current status

Brief: [docs/brief/00-index.md](docs/brief/00-index.md) · Agent rules: [AGENTS.md](AGENTS.md) · **Open hardware items: [docs/progress/HARDWARE_PENDING.md](docs/progress/HARDWARE_PENDING.md) (summary at the top)** · Original hardware test plan (mostly historical): [docs/hardware-test-plan.md](docs/hardware-test-plan.md) · Overview: [README.md](README.md)

Status legend: `todo` · `in-progress` · `done` · `done-sim` (done locally, needs hardware verification) · `done-hw` (verified on the owner's Pi/F730; any leftovers are listed in HARDWARE_PENDING) · `blocked-hw` (cannot proceed without physical hardware) · `retired` (artifact removed from the repo or no longer applicable)

## Milestones

| Milestone | Status | Notes |
|---|---|---|
| M0 Research & feasibility | done | Feasible; source facts since confirmed on the F730 (HP-03/05/06 via HP-26). Protocol spec: docs/research/protocol-summary-for-implementers.md |
| M1 Local foundation | done | Clone, `cargo run -- --device simulated`, UI + API + OpenAPI, deterministic scenarios, contract/integration/Playwright tests, all without hardware |
| M2 Protocol & register system | done | Accepted 2026-10-24 with real F730 captures; 2026-10-25 register survey (576 readable registers answered) and display-photo verification of units/scales (HP-34) |
| M3 Provisioning & administration | done-hw | On the owner's Pi: boot-partition provisioning (HP-30), station Wi-Fi, mDNS, LAN auth, API keys and the Access page. Setup-AP onboarding and factory reset only in simulation (HP-13/14, HP-40) |
| M4 Pi appliance | done-hw | Alpine Linux in both A/B slots on the owner's Pi (HP-38), static musl gateway, 35 MiB RAM in use. The fresh-card image is flashed on a spare card; layout passed, setup-AP rerun with `firstboot.3` pending (HP-47) |
| M5 OTA & dev workflow | done-hw | App OTA with probation (0.1.1 → 0.1.23) and OS A/B updates with tryboot (HP-29, HP-35…HP-38) on hardware. Failure-path and power-cut tests deferred by the owner (HP-16/17/22) |
| M6 Real F730 integration | done-hw | Two-speed polling, WebSocket stream, history, writes 47011 / 45171 (0 → 1 edge) / 48132 verified (HP-27/28/31/32). MQTT tested against a Docker broker only. Disconnect-recovery and logic-analyser timing are nice to have (HP-07/21) |
| M7 Stabilization & release | in-progress | Live since 2026-10-24. Open: power-cut matrix, update failure tests, 24 h memory record |

## What's left (2026-10-26)

The complete hardware list is the summary at the top of
[docs/progress/HARDWARE_PENDING.md](docs/progress/HARDWARE_PENDING.md). In short:
1. **Owner at the pump:** HP-34 leftovers: 43147 with a clamp meter, energy counters against myUplink (fan modes done 2026-10-26).
   HP-45 and HP-48: lease restores on the setpoints and holiday mode (48043), not yet seen on the pump.
2. **Bench Pi / spare card:** HP-47 rerun with `firstboot.3` from step 2, HP-40 (flash the fresh Alpine card, setup AP, first OS update, live Wi-Fi fallback
   trigger), then HP-13/14 (AP onboarding, recovery, factory reset) and the Alpine supervisor watchdog (HP-11 / HP-38a step 2).
3. **Deferred by the owner:** OS- and app-update failure paths (HP-16, HP-22, HP-29/38 unhealthy slot and network
   gate) and the SD power-cut matrix (HP-17, HP-18, HP-33 step 5).
4. **Nice to have:** HP-07/21 physical timing and disconnect recovery, HP-08, HP-19/20/23 leftovers, HP-25 photos,
   history numbers (HP-33), Access-page toggles under OpenRC.

**Firmware watchdog for OS trials** (needs a small `os.rs` change and one supervised
test): closes the one trial failure mode that needed a power cycle.

**Open owner decisions:**
- Which control, if any, to verify next (one register at a time).
- Public redistribution of the catalog text: decided 2026-10-31, published with attribution and a NIBE disclaimer.
- Wi-Fi passphrase storage: decided to keep the passphrase as is; revisit if the Pi joins a network
  whose passphrase is reused elsewhere.

## History

Git history. The day-by-day log and workstream files are at commit `218bae5` (`PROGRESS.md`, `docs/progress/`).
