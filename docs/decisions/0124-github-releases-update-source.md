# ADR 0124: GitHub Releases is an optional update transport; the signature stays the only trust

Date: 2026-10-31 · Status: accepted (owner decision)

## Context
The repository is becoming public, and the owner wants the admin UI to show and install new releases without
building and uploading a tar by hand. The appliance is local-first (brief §1): it must work offline and must not
depend on a cloud service. Signed releases already exist, and the updater already verifies them (ADR 0070).

## Decision
- Official application releases are published as GitHub Releases, each with the signed release tar attached.
- The Updates page can check the configured repository for the latest release and, as a second explicit step,
  download it and hand it to the updater exactly like a manual upload. Activation stays a third, separate step.
- Nothing happens in the background: no periodic polling, no automatic download or install.
- Only a newer version is offered. Downgrades remain a deliberate manual upload with confirmation.
- The source can be turned off per gateway. Manual upload keeps working with or without it.

## Why
- GitHub is only a transport. A compromised account, a fake release or an intercepted download still has to pass
  the Ed25519 signature check, so the trust model does not change.
- Admin-initiated checks reveal nothing to GitHub until someone asks, and an offline gateway just reports
  "unreachable".
- The appliance image already has a CA-verifying HTTPS client, so this adds no TLS stack to the gateway on a
  512 MB ARMv6 device.

## Rejected
- Background polling with an "update available" badge: steady outbound traffic and rate-limit use for little gain.
- Automatic installation: an unattended restart of the heat-pump gateway is the owner's decision, not a timer's.
- A TLS library in the gateway: larger binary and more dependencies than calling the image's client.
- Showing release notes in the UI: untrusted text. The UI links to the release page instead.

## Consequences
Publishing a release now also needs the signing key on the publishing machine and a GitHub Release with the signed tar
attached. OS bundles are not covered and still install over SSH. Anyone with a fork can point their gateways at
their own repository, but those gateways still only accept releases signed with the key they trust.
