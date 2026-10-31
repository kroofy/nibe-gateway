# ADR 0111: Separate root helper for system settings

Date: 2026-10-25 · Status: accepted, live on hardware

## Context
SSH keys and Tailscale are root-owned. The UI should manage them, while the gateway stays unprivileged.

## Decision
- A small root helper accepts a fixed, validated set of operations over a local socket.
- It never runs free-form commands.
- It refuses to disconnect Tailscale when the request itself arrives over Tailscale.

## Why
- This is the updater's pattern: privilege lives in a small process with a fixed set of operations that is easy
  to audit.
- The owner is remote, so the UI must not be able to cut the path it is being used through.
