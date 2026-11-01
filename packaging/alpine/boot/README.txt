NIBE Gateway - fresh card, image @VERSION@ (Alpine Linux, A/B slots), disk id @DISKID@

This partition (bootfs) holds the Raspberry Pi firmware, config.txt and the two OS slots (slot-a/, slot-b/).
Do not edit config.txt or the slot directories: the updater manages them.

Optional first-boot provisioning
-------------------------------------------
Without a provisioning file the gateway starts its setup access point: an open Wi-Fi network (no password)
named NIBE-Gateway-XXXX. Join it, open http://192.168.4.1:8080/ and create the administrator straight away:
until then anyone in Wi-Fi range can claim the gateway.

To provision instead, copy nibe-provision.json.template to nibe-provision.json on this partition and edit it.
Every key is optional - delete the lines you do not need (keep the file valid JSON):
  wifi                 Wi-Fi network to join (ssid 1-32 bytes, password up to 64 characters)
  admin_password       web admin password (12-256 characters)
  ssh_authorized_keys  public keys for key-only SSH as nibe-admin (sudo allowed)
  tailscale_auth_key   a one-time Tailscale auth key (tskey-...)
Placeholder values are rejected: an invalid file is left untouched and nothing is applied.
On first boot the file is validated, applied and then deleted from this partition; secrets end up only in
root-only storage on the data partition. A wrong Wi-Fi password falls back to the setup access point.

The Pi is reachable afterwards as http://nibe-gateway.local:8080/ (mDNS) or by its address on your network.
