# Roadmap and open questions

What comes next and what is still open. What works today and every test: [status](status.md); what
each version changed: [CHANGELOG](../CHANGELOG.md).

## 1. Roadmap

| Step | What | When |
| --- | --- | --- |
| **Done** | The test builds before the first public release (internal) | 2026-11-17 to 2026-11-24 |
| **Now** | The planned OS features (D55) and Asima App Center (D56), listed below, are in the source; next the first Asima OS build with new keys: the VM suites first, then the owner's PCs; every ⏳ in [status](status.md) gets a result | Now |
| **Asima Hub everywhere (D39)** | In steps, each tested before the next. Done: the app ID `io.github.asimaos.Hub`; the signed feed with a new feed key and the news; the Get Asima OS page (the ISO download, writing the USB stick, "What you'll get on this PC"); the Linux Flatpak as a trial bundle on GitHub Releases, offered by Asima Manager and Asima Maintenance (D54). Then the installer's offer for after the first sign-in, Flathub, the AppImage, the Windows `.exe` and the Snap. Needs a SourceForge project for the big files and a project email address ([Asima Hub](https://github.com/asimaos/asimaos-hub)) | After the first build |
| **Release candidate** | Once the owner confirms; the ISO and update file on SourceForge, the feed on the website (D50) | After that |
| **Later** | The release tools (section 4) | Later |

## 2. Open questions

**Gaps to close before a release:**

- **A security contact:** `SECURITY.md` and GitHub's private vulnerability reporting.

## 3. Built, to be tested (D55, D56)

### The system

- Updates that skip versions (D52): every update file carries the migration steps of the past year.
- Virtual machines' settings in `@vm` (D53).
- Game mode with gamescope; the handhelds' quick-access and back buttons and gyro aiming through
  InputPlumber.
- Asima Rescue smaller still: ICU's data (32 MB of the 415 MB in memory) filtered to what Rescue
  needs.
- The firmware's list of revoked boot programs (dbx) updated from Asima Maintenance, with TPM
  unlock kept working across it.
- Signed system extensions (`systemd-sysext`, signed images only).
- Extra kernel modules (vhba, v4l2loopback, xpadneo, xone with xpad-noone), built against the
  exact kernel and signed with the OS; game controllers of every kind (XInput and DirectInput, wired,
  Bluetooth and wireless receivers, the Xbox Wireless Adapter with its firmware from Microsoft on
  request).
- The two versions on the disk share their identical blocks (deduplication after each update).

### Apps

- A repair app on the USB stick: "Repair this computer" with screens, driving `asimaos-engine`; it
  installs the stick's version, newer or older, keeping files.
- Asima Manager → Shield: a zone per network, remembered when it is joined, and a zone that stops
  nothing ("Not secure"); the VPN in use; a count of the connections Shield stopped.
- Asima Manager → Display: brightness of external monitors (DDC/CI through `ddcutil`).
- Asima Dev: disk use and limits for containers, VMs and Nix (Btrfs quotas); no copy-on-write for VM
  disk folders.
- Flatpak repair; Flatpak apps' caches in memory, for those who switch it on.
- **Asima App Center (D56):** every kind of Linux software in one place, with purposes as tabs
  and one row per kind; Android apps, Nix programs and container apps move in from Asima Dev, the
  Flatpak clean-up from Asima Maintenance and the app groups from Asima Hub.

## 4. Later

- Security keys (FIDO2, USB or NFC) for signing in, once there is a key to test with.

### Release tools

- A release pipeline: a snapshot bump every two weeks to a testing channel, promoted to stable after
  the QEMU and hardware gates; kernel and security rebuilds within 48 hours; a check that two builds
  from the same snapshot are identical.
