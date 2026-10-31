# ADR 0129: The first boot of a fresh card completes its partition layout

Date: 2026-10-31 · Status: accepted (owner decision 2026-10-31); amends ADR 0106 (how a fresh card gets its layout)
and relates to ADR 0117

## Context
The fresh-card image carried the full A/B layout. Almost all of it is empty, but a raw writer still writes every
byte, so flashing the owner's card took about 25 minutes. A fresh card holds no data that a failed first boot
could lose.

## Decision
- The image holds only the boot partition and slot A cut to its root. The first boot completes the ADR 0106 layout
  (slot A at full size, the data partition, slot B) and creates and seeds the data partition the way the image
  build used to.
- It does so only when a marker written by the image build is present and the partition table is exactly the
  image's short one. The marker is removed last, so an interrupted first boot starts over on the next boot.
- Without the marker a missing data partition stops the boot with a clear message. Nothing is guessed, and an
  installed gateway or an OS update never takes this path.
- If the first boot fails, the remedy is to write the card again.

## Rejected alternatives
- Keep the full image: correct but slow to flash, and the time grows with the slot sizes.
- Resize in the running system (OpenRC service): the data partition is needed before services start, and the
  root is read-only.
- Rely on Raspberry Pi Imager's sparse writing: not every writer skips zeros, and the owner flashes with dd.

## Consequences
- Flashing writes about a tenth of the bytes. The first boot takes a little longer and leaves a trace on the boot
  partition.
- The early boot now writes a partition table, a destructive step. It is guarded, and the first real card is a
  hardware check.
- The layout numbers live in two places, the image build and the initramfs, and tests keep them equal.
