# Remote Access

Drive your SuperHQ host from another device. Open a browser at
`https://remote.superhq.ai` from your phone, tablet, or another
computer and you get the same workspaces, agent tabs, and shells
you see on the desktop.

## How the transport works

Remote control rides on [iroh](https://iroh.computer), a peer-to-peer
QUIC transport. Your desktop host advertises a public key as its
`host id`. A paired device connects directly to the host by that id.

You do not need to:

- open a port on your router
- run a VPN or Tailscale
- know your IP address or use DDNS

Iroh handles NAT traversal and routing. Traffic is end-to-end
encrypted; our servers are not in the middle.

## Enabling remote control on the host

1. Open SuperHQ on your desktop.
2. Go to Settings > Remote control.
3. Toggle **Enable remote control** on.

The host now runs an iroh endpoint and shows a host id + QR code in
the network popover (the icon next to the sidebar toggle in the
title bar). The setting persists across restarts.

## Pairing a device

1. On the device, open `https://remote.superhq.ai`. You can install
   it as a PWA from the browser's "Add to Home Screen" menu for a
   native-feeling app icon.
2. Tap **Scan QR code** and scan the code in the SuperHQ popover on
   the desktop. If the device does not have a camera, paste the
   host id directly.
3. Your desktop shows an approval prompt. Click **Approve**.
4. The mobile client asks for a Touch ID, Face ID, or device
   password check to seal the credential on the device.

Pairing issues a device-specific key that lives in the browser's
secure storage (WebAuthn PRF wrapped in IndexedDB). Future sessions
reuse it without a fresh approval prompt.

## What you can do remotely

- Browse workspaces, including stopped ones. Tapping an inactive
  workspace starts its sandbox the same way clicking would on the
  desktop.
- Open tabs: agents, guest shells attached to a running agent's
  sandbox, and (opt in) the host shell.
- Type into any live terminal over a real PTY stream, with
  scrollback that matches the desktop.
- Watch setup progress in real time while an agent boots.
- Close tabs with the same Checkpoint / Close / Cancel choices the
  desktop offers.

The state you see on the phone is the same state the desktop sees.
Snapshots are pushed from the host over the control stream; changes
appear without polling.

## Host shell access is opt in

Agent sandboxes are isolated from your host OS. The host shell is
not: it runs commands on your actual machine.

By default, paired devices cannot open a new host shell or attach to
an existing one. Flip **Allow remote access to host shell** under
Settings > Remote control to opt in. Enable it only for trusted
devices on trusted networks.

## Managing paired devices

Settings > Remote control lists every paired device with its label
and pairing timestamp. Hit **Revoke** to invalidate that device's
credential; it will have to pair again.

### Rotating the host id

If a host id leaks or you want to wipe every pairing at once, tap
**Rotate host id**. A fresh iroh endpoint is generated, every paired
device is unpaired, and any in-flight remote sessions drop. You then
re-pair the devices you want to keep.

Rotation is irreversible. A confirmation dialog explains the
consequences before you commit.

### Audit log

Every remote RPC (`session.hello`, `tabs.create`, `pty.attach`, and
so on) is recorded to a local JSON log with the device id, method,
and timestamp. The file path is shown at the bottom of the Remote
control settings tab. Click it to open the file.

## Security model

- **Pairing is the trust boundary.** A paired device is treated like
  a second keyboard attached to your host. If a device is stolen and
  the OS-level authentication is broken, the attacker inherits
  remote access until you revoke the pairing.
- **Fresh auth on every session.** Each reconnect performs a
  nonce-based HMAC challenge against the device's pairing key. There
  is no session cookie to steal; the host issues a fresh nonce per
  connection and accepts it exactly once.
- **Encrypted end to end.** iroh provides QUIC-level TLS; the host
  and client exchange no plaintext.
- **Host shell is off by default** so a leaked pairing credential
  still cannot run commands on your machine unless you opted in.
- **The audit log is local.** Nothing about your sessions leaves
  your host.

## Troubleshooting

**"Couldn't reach the host"** - the host is offline, or remote
control is turned off. Check Settings > Remote control on the
desktop and make sure the toggle is on.

**Pairing times out** - the approval prompt appears on the host's
desktop. If the desktop is locked or the app is minimized, unlock
and click Approve.

**Terminal looks cramped** - the PTY size is the minimum across
every attached client (desktop + remote). Close the desktop
terminal panel (or narrow the window) if you want the mobile to
drive the dimensions.

**Nothing updates after reconnecting** - if the snapshot stops
refreshing, the control stream probably died. Pull down to refresh
the workspace list, or close and reopen the PWA. The client
reconnects automatically.
