# OTA updates & recovery

> Source: ORIGINAL_BRIEF.md §13

## 13. OTA updates and recovery

**Initial scope: application-level OTA**, not full OS OTA.

- Publish signed ARMv6 release artifacts with checksums, version and compatibility metadata.
- Support admin-UI upload and optional periodic release checking; default to manual approval.
- Use two persistent application slots (A/B). A separate small updater verifies, stages, activates, restarts, checks readiness, and rolls back on failure.
- Keep persistent configuration/network credentials across updates and rollbacks; design schema migrations for backward compatibility.
- Check more than just an open HTTP port: startup health and appropriate service readiness.
- Treat OS updates separately; do not promise rollback of kernel/rootfs changes without boot-level recovery testing.

Illustrative persistent layout:

```text
/var/lib/nibe-gateway/
  releases/slot-a/
  releases/slot-b/
  config/
  update-state/
```
