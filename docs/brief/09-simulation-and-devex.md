# Local simulation & developer experience

> Source: ORIGINAL_BRIEF.md §11

## 11. Local simulation and developer experience

**First-class requirement:** Run and verify the entire service without a Pi or F730.

```bash
cargo run -- --device simulated
cargo run -- --device replay
cargo run -- --device hardware
pnpm dev    # Vite HMR with /api proxy to local Rust service
```

Modes:
- **Simulation:** deterministic synthetic measurements, commands, and simulated Wi-Fi state.
- **Replay:** recorded F730 serial frames through the real parser.
- **Hardware:** real RS485 and networking adapters.

Scenario fixtures should include normal heating, cold weather, hot-water recovery, compressor idle/running, disconnects, stale data, malformed frames, timeouts, rejected writes, first-boot setup, invalid Wi-Fi password, and network recovery. Support simulated time advance/freeze, fault injection, reset, and command history.

Development-only endpoints (e.g. `/__dev/scenario`, `/__dev/time/advance`, `/__dev/fault`, `/__dev/reset`) must be compiled out or inaccessible in production. Prefer protocol-level replay as the authoritative integration surface, with domain-level simulation for UI and workflow development.
