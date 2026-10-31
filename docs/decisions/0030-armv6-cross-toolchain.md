# ADR 0030: Pinned user-level ARMv6 cross toolchain with a binary gate

Date: 2026-10-22 · Updated: 2026-10-31 · Status: accepted

## Context
The Pi Zero W is ARMv6 hard-float. Most "armhf" toolchains and userspaces silently target ARMv7. The build host has
system Rust without rustup.

## Decision
- Use a pinned, user-level cross toolchain that builds statically linked ARMv6 binaries.
- Check every built binary's architecture, float ABI and linking before it is packaged.

## Why
- An ARMv7 instruction or the wrong ABI only fails on the device, possibly in the middle of a remote update.
  Catching it at build time is cheap.
- Static binaries don't depend on the base OS's libc (ADR 0117).
- Pinning keeps builds reproducible, and leaving system Rust alone keeps the host clean.

## Rejected
- Dynamic linking against the OS's glibc: the binaries had to run on both bases during the move to Alpine.

## Consequences
Emulation checks complement hardware tests but don't replace them.
