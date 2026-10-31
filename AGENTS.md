# Rules for agents working in this repo

1. Read `PROGRESS.md`, your workstream file in `docs/progress/`, and the brief files it references in `docs/brief/`.
2. Stay inside the paths your workstream **owns**. If you must touch another workstream's files, keep it minimal and note it in your workstream log.
3. **Do not `git commit`** — the orchestrator commits between waves. Other agents may be working in the same checkout concurrently.
4. Keep local simulation working (`cargo run -p nibe-gateway -- --device simulated`). `cargo test --workspace` must pass when you finish.
5. Distinguish **verified facts** from **hypotheses**. Anything needing the physical Pi/F730 goes into `docs/progress/HARDWARE_PENDING.md` (append, don't rewrite) — the owner is remote, so build everything possible and leave precise hardware test steps.
6. Register writes by number (ADR 0125, owner decision 2026-10-31) go through the one shared implementation used by HTTP and MQTT: one at a time, never retried, always read back. Do not add a second write path or remove those properties. With open writes off, only the named controls write, and each stays config-gated until hardware-verified; with open writes on, the hardware-verified named controls are enabled too.
7. ADRs (`docs/decisions/`) are rare. Write or extend one only when a decision is hard to reverse, is a safety or security invariant, or rejects an obvious alternative that a future agent would plausibly propose again. A new feature does not need one by default. Everything else goes in a code comment next to the code, in `docs/dev/`, or in your workstream log. Prefer extending the ADR that already covers the subject; an ADR states the current decision, so rewrite it rather than append amendments. Follow `docs/decisions/README.md`. If you are unsure, ask the owner instead of writing one.
8. Before finishing, update your workstream file: tick checklist items, set `Status:`, add a dated log entry with what was done, how to verify (exact commands), and open issues. Also update your row in `PROGRESS.md`.
9. Minimal dependencies. Target is ARMv6 / 512 MB RAM (Pi Zero W).
