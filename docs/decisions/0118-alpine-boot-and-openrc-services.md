# ADR 0118: Alpine slot boot and services: every failure returns to a good slot and leaves evidence

Date: 2026-10-25 · Status: accepted, live on hardware (HP-38) · Research: [alpine-image-2026-10-25.md](../research/alpine-image-2026-10-25.md)

## Context
- Alpine's boot tooling cannot find the root the way the existing slot contract names it.
- OpenRC has no equivalent of systemd's watchdog integration and sandboxing.
- The first hardware trial with Alpine's kernel hung silently.
- The Pi is headless and remote.

## Decisions and why
1. **Our own minimal initramfs, not a changed slot contract.** The updater and the existing slot stayed
   compatible, so migration and rollback kept working.
2. **The Raspberry Pi OS kernel, not Alpine's.** That firmware and kernel pair was proven on this board. The other
   pair hung with no trace.
3. **Boot files that don't depend on the firmware applying overlays.** On one trial the firmware skipped an
   overlay, and the serial port, which is the whole product, moved.
4. **Explicit headless safety.** A hardware watchdog plus a boot guard reboot a hung trial or a boot that is
   missing its data partition. Every failure path ends at the committed slot without anyone on site.
5. **Boot traces on trial and smoke boots only.** A failed remote trial must leave evidence, but normal boots must
   not write the boot partition.
6. **A smoke boot before every trial.** It catches boot failures before the slot could ever be committed.

## Consequences
- systemd's per-service sandboxing is gone. Unprivileged users and the read-only root remain.
- The root helpers keep their capabilities, because services they restart inherit them.
- Logs live in RAM.
- Open proposal: a firmware-level early watchdog for trials (HP-38).
