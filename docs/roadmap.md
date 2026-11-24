# Roadmap and open questions

What comes next and what is still open. What works today and every test: [status](status.md); what
each version changed: [CHANGELOG](../CHANGELOG.md).

## 1. Roadmap

| Step | What | When |
| --- | --- | --- |
| **Done** | The test builds before the first public release (internal) | 2026-11-17 to 2026-11-24 |
| **The next build** | The first Asima OS build, with new keys: the VM suites first, then the owner's PCs; every ⏳ in [status](status.md) gets a result | Now |
| **Asima Hub everywhere (D39)** | In steps, each tested before the next. Done: the app ID `io.github.asimaos.Hub`; the signed feed with a new feed key and the news; the Get Asima OS page (the ISO download, writing the USB stick, "What you'll get on this PC"); the Linux Flatpak as a trial bundle on GitHub Releases, offered by Asima Manager. Then the installer's offer for after the first sign-in, the app groups (with the next OS update), Flathub, the AppImage, the Windows `.exe` and the Snap. Needs a SourceForge project for the big files and a project email address ([Asima Hub](https://github.com/asimaos/asimaos-hub)) | Now |
| **Release candidate** | Once the owner confirms; the ISO and update file on SourceForge, the feed on the website (D50) | After that |
| **Later** | The release tools and the rest of section 3 | Later |

## 2. Open questions

**Decisions needed from the owner:**

- **Asima Maintenance's "Get Asima Hub" button:** it opens the Hub's page in Discover
  (`appstream://io.github.asimaos.Hub`), which finds it only once the Hub is on Flathub;
  until then Asima Manager's download is the way (D39 keeps Maintenance as it is): keep the button
  for Flathub, or point it at the same download now?
- **Where VM definitions live:** in `/etc/libvirt` (reset with the system's settings) or in `@vm`
  next to the disks; to decide when VM support is finished.

**Gaps to close before a release:**

- **PCs that skip versions:** every update file carries all one-time migration steps since its
  minimum version (`MIN_FROM`).
- **A security contact:** `SECURITY.md` and GitHub's private vulnerability reporting.

## 3. Planned parts

### Security and start-up

- Asima Rescue smaller still: ICU's data (32 MB of the 415 MB in memory) filtered to what Rescue
  needs, which takes an ICU build of its own.

### Apps

- A repair app on the USB stick: "Repair this computer" with screens, driving `asimaos-engine`; it
  installs the stick's version, newer or older, keeping files (going back is safe one version at a
  time, as migrations keep only the previous version working).
- Asima Gaming on handhelds: the quick-access and back buttons and gyro aiming, through InputPlumber
  once it is in the image (it wasn't reachable when the handheld controls were built).
- Asima Maintenance: `flatpak repair`; Flatpak app caches in RAM (inside the sandbox only
  `~/.var/app/<app ID>/cache` is visible; test with real apps first).
- Asima Manager → Shield: a zone per network, remembered when it is joined; the VPN in use; a count
  of the connections Shield stopped.
- Asima Manager → Sign-in: security keys (FIDO2, USB or NFC) for signing in.
- Asima Manager → Display: brightness of external monitors (`ddcutil` is installed; Plasma needs
  the `i2c-dev` module and seat access to `/dev/i2c-*`).
- Asima Dev: disk use and limits for containers, VMs and Nix (Btrfs simple quotas, also on `@home`
  and `@var`); no copy-on-write (`chattr +C`) for VM disk folders.
- Signed system extensions (`systemd-sysext`, signed images only).

### Release tools

- A release pipeline: a snapshot bump every two weeks to a testing channel, promoted to stable after
  the QEMU and hardware gates; kernel and security rebuilds within 48 hours; a check that two builds
  from the same snapshot are identical.
- Extra kernel modules (vhba, v4l2loopback, xone, xpadneo) built in clean containers against the
  exact kernel headers and signed; beesd deduplication between the two versions.
