# Rules for agents working in this repo

1. Read `PROGRESS.md`, your workstream file in `docs/progress/`, and the brief files it references in `docs/brief/`.
2. Stay inside the paths your workstream **owns**. If you must touch another workstream's files, keep it minimal and note it in your workstream log.
3. **Do not `git commit`** — the orchestrator commits between waves. Other agents may be working in the same checkout concurrently.
4. Keep local simulation working (`cargo run -p nibe-gateway -- --device simulated`). `cargo test --workspace` must pass when you finish.
5. Distinguish **verified facts** from **hypotheses**. Anything needing the physical Pi/F730 goes into `docs/progress/HARDWARE_PENDING.md` (append, don't rewrite) — the owner is remote, so build everything possible and leave precise hardware test steps.
6. Register writes by number (ADR 0125/0126, owner decision 2026-10-31) go through the one shared implementation used by HTTP and MQTT: one at a time, never retried, always read back. Do not add a second write path or remove those properties. Named controls follow ADR 0104 and stay config-gated until hardware-verified.
7. Record notable design decisions as ADRs in `docs/decisions/NNNN-title.md` using the template in `docs/decisions/README.md`. An ADR records **why** (context, decision, rejected alternatives, consequences), about 20 lines, with no implementation detail (no paths, parameters, commands or test results; those go in code, `docs/dev/` or progress files). Supersede or add a short dated amendment instead of rewriting, and add the ADR to the index.
8. Before finishing, update your workstream file: tick checklist items, set `Status:`, add a dated log entry with what was done, how to verify (exact commands), and open issues. Also update your row in `PROGRESS.md`.
9. Minimal dependencies. Target is ARMv6 / 512 MB RAM (Pi Zero W).
