# ADR 0124: GitHub Releases is an optional update transport; the signature stays the only trust

Date: 2026-10-31 · Status: accepted (owner decisions)

## Context
The repository is public, and the owner wants the admin UI to show and install new releases without building and
uploading a file by hand. New users need a way to start. The appliance is local-first: it must work offline and
must not depend on a cloud service. Signed releases already exist (ADR 0070).

## Decision
- Each release is a GitHub Release carrying the signed update, a whole fresh-card image built from the same
  binaries, and its changelog section as the release notes. A release without a changelog section is not
  published.
- The Updates page checks the configured repository only when an admin asks, and offers only a newer version.
  After one confirmation it downloads, verifies and activates it.
- Nothing happens in the background. Downgrades remain a manual upload with confirmation. The source can be
  switched off, and manual upload always works.

## Why
- GitHub is only a transport. A compromised account, a fake release or an intercepted download still has to pass
  the signature check, so the trust model does not change.
- Checks that only run when asked reveal nothing to GitHub until then, and an offline gateway just reports
  "unreachable".
- The image already has a CA-verifying HTTPS client, so the gateway gains no TLS stack.
- A new installation and an update run the same code.

## Rejected
- Background polling with an "update available" badge: steady outbound traffic for little gain.
- Automatic installation: an unattended restart of the heat-pump gateway is the owner's decision.
- A TLS library in the gateway: a larger binary and more dependencies.
- Showing release notes in the UI: untrusted text. The UI links to the release page instead.
- Separate download, stage and activate steps for the user: confusing. The updater still keeps them apart.

## Consequences
Publishing needs the signing key on the publishing machine. OS bundles are not covered and still install over
SSH. A fork can point its gateways at its own repository, but they still only accept releases signed with the key
they trust.
