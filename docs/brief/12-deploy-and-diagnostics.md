# Developer deployment & diagnostics

> Source: ORIGINAL_BRIEF.md §14

## 14. Fast developer deployment and diagnostics

Ordinary development must never require rebuilding the image or reflashing SD:

```bash
just dev
just test
just build-pi
just deploy
just logs
just debug
just rollback
just image
```

`deploy` cross-compiles, transfers over authenticated SSH, activates a build, restarts the service, and checks health. Provide structured tracing, temporary verbose logs, serial-frame capture, communication counters, register inspection, and sanitized support bundles. Captures should be exportable for local replay. Developer mode requires explicit authorization, expires automatically, and **must not** bypass command safety or authentication.
