# OS image & filesystem

> Source: ORIGINAL_BRIEF.md §15

## 15. Image and filesystem

Use **pi-gen** (https://github.com/RPi-Distro/pi-gen) for a reproducible minimal 32-bit Raspberry Pi OS Lite image containing gateway binary/static UI, systemd units, serial setup, NetworkManager, AP provisioning dependencies, mDNS, updater, and recovery tooling.

- Prefer read-only root filesystem.
- Use a small writable persistent data/config area for credentials, settings, update metadata and application slots.
- Keep ephemeral logs in memory where practical, with intentional diagnostic export.
- Verify behavior under unexpected power loss and corrupted/incomplete updates.
- Publish compressed flashable images from CI; support binary-only updates separately.
