# ADR 0128: The gateway keeps an audit log of every change

Date: 2026-10-31 · Status: accepted (owner request)

## Context
Any register can be written from the admin UI, API keys and MQTT (ADR 0125), and leases (ADR 0123)
make the gateway itself write when a writer goes silent. When the pump behaves unexpectedly the owner, who is
often remote, needs to know who changed what, when, through which surface, and what the pump answered. Settings
changes (Wi-Fi, MQTT, keys, updates) raise the same question.

## Decision
- The gateway records every change request that reached it: who asked (admin, API key name, MQTT, the gateway
  itself), what was asked, and the outcome, including refused and failed attempts. It also records the pump's
  answer to every register write and its read-back, whoever caused the write.
- Reads, sign-ins and requests refused for lack of authentication are not changes and are not recorded.
- Secrets are removed before anything is stored. The log is admin-only and shown in the admin UI next to the
  leases currently in force.
- The log lives on the data partition, bounded in size and kept across restarts and updates.

## Why
- One recording point over all HTTP routes, plus the MQTT bridge, the lease coordinator and the write outcomes,
  covers every way a change enters without each feature remembering to log.
- Recording the pump's answer separately from the request shows what actually reached the pump.

## Rejected
- Logging only register writes: settings and access changes matter as much when diagnosing.
- Recording unauthenticated requests: anyone on the LAN could push real entries out of a bounded log.
- Durable writes (fsync) per entry: SD wear and latency on the Pi Zero; the last entries before a power cut may be
  lost, which is acceptable for a diagnostic record.

## Consequences
New change routes are recorded automatically, under a generic label until named. A factory reset may erase the
log with the rest of the data. Entries are not tamper-proof: the admin can delete the file over SSH.
