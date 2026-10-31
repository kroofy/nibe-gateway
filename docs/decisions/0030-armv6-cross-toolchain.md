# ADR 0030: Pinned user-level ARMv6 cross toolchain with a binary gate

Date: 2026-10-22 · Status: accepted; libc target superseded by ADR 0117 (static musl)

## Context
The Pi Zero W is ARMv6 hard-float. Most "armhf" toolchains and userspaces silently target ARMv7. The build host has
system Rust without rustup.

## Decision
- Use a pinned, user-level cross toolchain.
- Check every built binary's architecture, float ABI and libc requirements before it is packaged.

## Why
- An ARMv7 instruction or the wrong ABI only fails on the device, possibly in the middle of a remote update.
  Catching it at build time is cheap.
- Pinning keeps builds reproducible.
- Leaving system Rust alone keeps the host clean.

## Consequences
- Emulation checks complement hardware tests but don't replace them.
- How-to: `docs/dev/cross-compile.md`.
