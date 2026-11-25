# Architecture

How Asima OS works. The owner's decisions are in [decisions](decisions.md), the apps and their
services in [`apps/DESIGN.md`](../apps/DESIGN.md), building and testing in [building](building.md),
what works today in [status](status.md), and what comes next in the [roadmap](roadmap.md).

## 1. Layers

```text
Layer 6  Apps          KDE apps (in base) · everything else via Flatpak, AppImage, Snap, Nix, Android apps (Waydroid)
Layer 5  Desktop       KDE Plasma 6 (Wayland), KDE's standard look · the Asima OS apps
Layer 4  Plumbing      drivers, firmware, dev toolchains, containers, KVM, gaming plumbing
Layer 3  Runtime FS    /  = RAM (tmpfs) · /usr = EROFS + dm-verity · /etc = overlay · /var = disk
Layer 2  Storage       LUKS2 (Argon2id) → Btrfs pool with 11 subvolumes
Layer 1  Boot          7 GB ESP: (shim) → systemd-boot → signed UKIs · Asima Rescue
```

## 2. Base system and kernel

- **Pinned base:** packages come from a dated Arch Linux Archive snapshot (`builder/asimaos.conf`),
  so builds are repeatable. Version format `YYYY.MM.DD.N`. The snapshot moves forward on purpose,
  with a release.
- **systemd 262** (systemd-pcrlock for TPM unlock, `ukify` and `systemd-cryptenroll`).
- **No package manager at runtime:** pacman is removed from the image.
- **Factory configuration:** `/etc` is copied to `/usr/share/factory/etc` at build time, with a
  checksum list (`/usr/share/factory/etc.sha256`); `/usr/lib/os-release` carries
  `IMAGE_ID=asimaos` and `IMAGE_VERSION`.
- **A standard kernel, never one built by Asima OS (D23):** Arch's default `linux`. Asima OS needs
  NTSYNC and uinput (modules), Android binder (built in: the Rust binder, so Waydroid works),
  sched-ext, the EROFS, dm-verity, Btrfs and overlay drivers, and the virtual machine drivers.
- **Lockdown and signed drivers** are not kernel options: on the command line they would stop
  NVIDIA's driver on every PC where the build's key isn't approved. `asimaos-early-mount` writes
  `integrity` to `/sys/kernel/security/lockdown` at start-up when Secure Boot is on and the build's
  key is in the approved MOK list the kernel trusts for drivers (`MokListTrustedRT`); then only
  signed drivers load. Elsewhere lockdown stays off and the journal says why.
- **NVIDIA:** Arch ships `nvidia-open` prebuilt for its `linux`. A distribution kernel only trusts
  its own throwaway key, so stage 15 signs NVIDIA's modules with the build's Secure Boot key after
  Arch's strip (decompress, append a PKCS#7 signature in `sign-file`'s format, `zstd -19`; signing
  before stripping breaks the signature). Turing (GTX 16xx / RTX 20xx) and newer; older cards use
  nouveau and NVK. Stage 15 checks that Flathub has the matching
  `org.freedesktop.Platform.GL.nvidia-<version with dashes>` extension (a beta only warns, a release
  stops), so Flatpak apps keep GPU acceleration. After an update brings a new driver,
  `asimaos-nvidia-flatpak.service` runs `flatpak update` at the start (once per driver version, again
  at the next start when offline or before Flathub has it), which installs the matching extension.
- **Security modules:** `lsm=landlock,lockdown,yama,integrity,apparmor,bpf`. Swap is zram only (no
  hibernation).

## 3. Disk layout (write-once)

| Partition | GPT label | Size | Content |
| --- | --- | --- | --- |
| 1 | `ESP` | 7 GB, FAT32 | systemd-boot (plus shim for the MOK path), UKIs, Asima Rescue. **Never mounted for the desktop:** only the services that need it (updates, "mark as good", Asima Maintenance) mount it at `/boot` in their own private mount namespace, and they end after five quiet minutes, so it is closed again |
| 2 | `crypt_pool` | rest of the disk | LUKS2 (Argon2id, TRIM allowed) → Btrfs (zstd:3, metadata DUP, noatime). Without encryption (D9): Btrfs directly, label `asimaos_pool` |

Asima OS claims the **whole disk** and never resizes or moves these partitions; other operating
systems stay on other disks (D31). The definitions are `builder/repart.d/`, shared by the image
build and the installer.

## 4. Btrfs subvolumes

| Subvolume | Mounted at | Purpose |
| --- | --- | --- |
| `@slots` | `/run/initramfs/slots` (read-only mount) | OS images: `usr_<ver>.erofs` + `verity_<ver>.img` (current + previous version), a copy of each version's boot image in `boot/` (for "Repair this computer"), staged parts of an update in `staged/` |
| `@data` | `/run/initramfs/data` | `/etc` overlay (upper/work) and flags read by the initramfs |
| `@reserve` | `/run/initramfs/reserve` | 2 GB emergency ballast file, never snapshotted |
| `@var` | `/var` | Persistent system state; `/var/tmp` stays on disk |
| `@containers` | `/var/lib/containers` | Podman / Distrobox |
| `@vm` | `/var/lib/libvirt` | The system's VM disks and their definitions (D53): `definitions/{qemu,storage,secrets}`, bound over the empty folders in `/etc/libvirt` at every start, so Reset settings leaves them alone |
| `@flatpak` | `/var/lib/flatpak` | System-wide Flatpaks |
| `@home` | `/home` | User files |
| `@root` | `/root` | Admin home |
| `@nix` | `/nix` | Each person's Nix programs (D28); made at the first start of a version that needs it |
| `@snapshots` | `/run/asimaos-snapshots` (root only) and read-only at `/home/.snapshots` | Snapshots: `home/` and `data/` on the schedule set in Asima Maintenance, `update/<version>/` before every update |

- **Disk use per area (D55):** the installer switches Btrfs simple quotas on, so what each subvolume
  writes is counted from the start, and Asima Dev can limit containers, virtual machines and Nix
  (`btrfs qgroup limit`, through `asimaos-maintenanced`). Each person's containers and virtual
  machine disks are subvolumes of their own inside `@home` (`asimaos-dev-storage` at the first
  sign-in), so they can be limited and stay out of the snapshots of `/home`; virtual machine disk
  folders are written without copy-on-write (`chattr +C`).
- **`@slots` is a writable subvolume mounted read-only:** deduplication tools skip read-only
  subvolumes, and `FIDEDUPERANGE` fails on immutable files. Slots are written only inside
  `asimaos-updaterd`'s private writable mount namespace. There, right after an update, the new
  version's system image shares every run of blocks (64 KiB or more) it has in common with the
  running version's (D55): blocks are matched by a hash of their 4 KiB content, and the kernel
  compares the bytes itself before it shares anything, so two versions take little more room than
  one.
- **No compression where it hurts:** the early mount marks `@slots` "no compression" (`chattr +m`)
  at every start, so the already compressed images updates write are stored as they are; the
  ballast is allocated with `fallocate`, uncompressed. Defragmentation is off for good (it would
  undo shared extents).
- **An installed `/var` starts empty:** the folders packages put under `/var` are made at every start
  from `/usr/lib/tmpfiles.d/asimaos-var.conf`, which the build writes. What an interrupted job left in
  `/var/tmp` (`asimaos-*`) is removed at every start (`asimaos-tmp.conf`).

## 5. Runtime filesystem and early boot

| Path | Comes from |
| --- | --- |
| `/` | tmpfs: fresh on every boot |
| `/usr` | EROFS image from `@slots`, verified by dm-verity; the root hash is baked into the UKI |
| `/etc` | OverlayFS: lower = `/usr/share/factory/etc`, upper = `@data` |
| `/var` | `@var` |
| `/tmp`, `/var/cache` | RAM (`/var/cache` at most 10 % of the memory); AppArmor's compiled policy is kept on disk (`/var/lib/asimaos/apparmor-cache`) |
| `/var/tmp` | Disk (large container pulls and update staging) |
| `/boot` | Empty for the desktop; an administrator can open it by hand with `mount /boot` |

All of this is set up inside the initramfs by `asimaos-early-mount` (`builder/rootfs/usr/lib/asimaos/`),
which also serves the live ISO (with `/etc` changes and `/var` in RAM). In order:

1. **Find and unlock the pool** on the disk the computer started from (`LoaderDevicePartUUID`), so a
   second Asima OS disk is never picked by mistake. `crypt_pool` is unlocked with `systemd-cryptsetup`
   (TPM, then passphrase, then recovery key, section 6); `asimaos_pool` is mounted directly.
   `/etc/crypttab` stays empty.
2. **Clean up:** half-written `.partial` files of an interrupted update are removed (a resumed update
   writes them again), through a short read-write mount before `@slots` is mounted read-only.
3. **Scheduled work:** a factory reset (section 9), factory versions of single `/etc` files or of
   every setting at once (Asima Maintenance → System changes → Reset settings), and putting settings
   back after a failed update (section 8). After "Reset settings", `asimaos-reset-settings.service`
   resets the person's own desktop and app settings before the login screen, as that person.
4. **`/usr`** with dm-verity, then **`/etc`** as an overlay with
   `index=off,metacopy=off,redirect_dir=off,xino=off`: the lower layer is a different file system
   after every update, and with overlayfs's `index` the mount would fail with `ESTALE`. The upper
   directory's mode becomes `/etc`'s mode, so it is always `root:root 0755`.
5. **The subvolumes**, including new ones a version needs (`@nix`).
6. **Emergency reserve:** the 2 GB `@reserve/.emergency_ballast` is made at the first start with more
   than 6 GiB free, and released when less than 1 GiB of data space or 256 MiB of metadata space is
   left (`.released` tells Asima Maintenance).
7. **Snapshots** are shown read-only at `/home/.snapshots`; root keeps the writable mount at
   `/run/asimaos-snapshots`.

A trial start that fails in early boot restarts after 5 s instead of stopping at an emergency
shell nobody can use (root is locked): the start menu counts the failed try, and after three the
backup starts. A TPM whose driver comes up late is waited for (`tpm2.target`, section 6).

## 6. Boot chain and trust

```text
UEFI firmware (Secure Boot)
 ├─ Default (D16): Microsoft-signed shim + the Asima OS key approved once on the blue MokManager screen
 │                 (the certificate is kept on the ESP: EFI/BOOT/asimaos.der)
 └─ Own keys in the firmware (Asima Maintenance → Security): this computer's Platform Key and KEK,
    db = the firmware's own certificates (Microsoft's, the maker's) + the Asima OS key
        ▼
systemd-boot (signed) → start menu: Asima OS (current) · Asima OS (backup) · Asima Rescue
        (the first entry with tries left starts by itself)
        ▼
Signed UKI = kernel + initramfs + CPU microcode + command line (with the /usr verity hash)
        ▼
Plymouth: the Asima OS start-up screen; the disk passphrase asked on it (D51)
        ▼
Unlock the pool: TPM2 (systemd-pcrlock policy: PCR 4 + 7; optionally + a PIN, D49) → passphrase → 64-character recovery key
        ▼
asimaos-early-mount → /usr (verified) · /etc · /var → switch root → KDE Plasma
```

- **Shim:** Fedora's Microsoft-signed shim, taken on the build host by `builder/fetch-shim.sh`
  (`dnf download shim-x64`, RPM signature checked). Shim 16.1 is signed with both Microsoft
  certificates, 2011 and 2023. ESP layout with shim: `EFI/BOOT/BOOTX64.EFI` = shim, `grubx64.efi` =
  systemd-boot, `mmx64.efi`, `asimaos.der`. Without dnf the build still works and systemd-boot is
  `BOOTX64.EFI`. **SBAT:** shim starts only programs whose SBAT generations are at or above its
  revocation level; `builder/check-sbat` reads the level from the shim's `.sbatlevel` section and
  stops the build when systemd-boot, the boot images or Rescue's are below its "previous" level (a
  warning below "latest"). Shim's revocations come only with shim; Asima OS can't add its own.
- **Own keys** (`asimaos-maintenanced`, `ownkeys.rs`): the firmware's db, KEK and dbx are saved and
  this computer's Platform Key and KEK made (RSA-2048, `/var/lib/asimaos/secureboot`, root-only, never
  leave it); the person puts the firmware into Setup Mode; then dbx (when the firmware lost it), db
  (the kept certificates plus the Asima OS key), KEK (plus this computer's) and the Platform
  Key last are written as authenticated variables (EFI_VARIABLE_AUTHENTICATION_2, PKCS#7 by
  `openssl cms`, db and dbx signed by the KEK, KEK and PK by the Platform Key). Unticked
  certificates are left out: that clears old keys out of the firmware. Shim and MOK keep working
  (Microsoft's certificates stay).
- **Old keys:** Asima Maintenance → Security lists every approved MOK besides this version's key
  (earlier builds', or another system's) and retires the chosen ones: `mokutil --delete` and
  `mokutil --mokx --import`, both with one one-time code, so nothing signed with them starts again.
  The firmware's own revocation list (`dbx`) is updated by fwupd, as a firmware update in Discover;
  Asima Updater checks a new shim against it (above). TPM unlock follows it (D55):
  `asimaos-secureboot-db.path` sees fwupd write `dbx`, `db` or `KEK` and brings the pcrlock policy
  up to date at once (`asimaos-pcrlock refresh`), so the next start, which measures the new
  databases into PCR 7, still unlocks.
- **Signing:** `ukify build --signtool=systemd-sbsign --sign-kernel` signs every UKI (also the live
  and Rescue ones); `systemd-sbsign` signs systemd-boot. The UKIs carry no signed PCR policy: TPM
  unlock uses systemd-pcrlock (D43, below).
- **TPM unlock** (D43) is set up on the installed system in Asima Maintenance → Security (D10),
  never by the installer, and only with Secure Boot on and a TPM that `systemd-pcrlock is-supported`
  accepts (TPM 2.0 revision 1.38 or newer, SHA-256). The disk key is sealed to a **systemd-pcrlock**
  policy kept in the TPM (an NV index): PCR 4 (shim, systemd-boot and the boot image, as the firmware
  measured them) and PCR 7 (the Secure Boot state and the certificates that verified the start).
  `/usr/lib/asimaos/asimaos-pcrlock refresh` writes the allowed variants: the shim and boot loader on
  the ESP and a new one waiting in the slots or on trial, and the boot image of each version in the
  start menu, never Asima Rescue's, so Rescue can't unseal the disk. It then runs `systemd-pcrlock
  make-policy --pcr=4 --pcr=7`; enrolment is `systemd-cryptenroll --tpm2-device=auto
  --tpm2-pcrlock=/var/lib/systemd/pcrlock.json`, **in two steps** when TPM unlock is set up again:
  the new slot is added next to the old one and tried at once (`cryptsetup open --test-passphrase
  --token-only --token-id=N`; when it fails it is removed and nothing changes), and the old one is
  wiped only after the next start, when the new one still opens the disk
  (`asimaos-tpm-rotation.service`, `/var/lib/asimaos/tpm2-rotation.json`). Asima Updater runs the refresh
  after writing an update, after a good start and after "Make it the current version", so updates
  keep TPM unlock working. PCR 0 and 2 (the firmware) are left out, so firmware updates don't break
  it; a Secure Boot database update can ask for the passphrase once. While the system runs, root can
  unseal the key too (PCR 4 and 7 stay the same), accepted with D43. The recovery key is always
  enrolled first (the installer shows it once).
- **With a PIN** (D49, optional; automatic is the default): the same pcrlock policy plus a PIN typed
  at every start (`--tpm2-with-pin=yes`, the PIN in `NEWPIN`, never on a command line). systemd
  hardens it with Argon2id before it reaches the TPM; Asima OS fixes its memory at 512 MiB
  (`--tpm2-argon2id-memory=512M`), because systemd's default sizes it from the computer's RAM and
  every start must find that much free. The new slot is tried at once with the PIN on stdin, and
  since a PIN can't be tried again after the restart without asking for it, the old TPM slots are
  wiped then and there (automatic unlock keeps the two steps). At the start `systemd-cryptsetup`
  asks for it on the start-up screen by itself ("LUKS2 token PIN", as the slot's token says);
  wrong PINs count towards the TPM's own dictionary-attack lockout, after which it falls back to
  the passphrase, and the passphrase and the recovery key always work. While wrong PINs have the
  TPM locked, Asima Maintenance says how long it waits instead of trying a new PIN. systemd-cryptenroll adds no slot when one with the very same policy exists (a PIN's value
  isn't part of the policy), so setting up a PIN again replaces the old slot outright. Computers
  without a usable TPM keep the passphrase.
- **The policy at start-up:** systemd-pcrlock also writes a copy of the policy to the ESP
  (`/loader/credentials/pcrlock.*.cred`); systemd-stub hands it to the initramfs, and
  `asimaos-early-mount` decodes it to `/run/systemd/pcrlock.json` (it isn't secret: the TPM enforces
  the policy itself) before `systemd-cryptsetup` unlocks the pool.
- **Waiting for the TPM:** `asimaos-early-mount` starts after `tpm2.target` and
  `systemd-pcrphase-initrd`; the initramfs carries `systemd-tpm2-generator` and every TPM driver, so
  a firmware TPM that comes up a moment late is waited for before the unlock. When a TPM key slot exists, the unlock waits up to 10 s for `/dev/tpmrm0` and
  asks the TPM explicitly (`tpm2-device=auto`).
- **Out-of-the-box Secure Boot** without the approval screen would need Asima OS's own Microsoft-signed
  shim; skipped (D31).

## 7. Signing keys

Keys belong to each build (D25): the build machine makes them in `build/keys` on first use. They
never go into a repository; they are kept in an encrypted archive (D50).

| Key | Type | Signs |
| --- | --- | --- |
| Secure Boot key (MOK or db) | X.509 RSA-2048 | systemd-boot, every UKI, Asima Rescue, NVIDIA's modules, the IOMMU boot add-on, system extensions (their dm-verity root hash) |
| Release key | Ed25519 (`aosupd keygen`) | Update files (header and manifest) and each build's `SHA256SUMS` |
| Next release key (optional) | Ed25519 | Shipped ahead of a planned change: systems from a build that carries `release-next.pub` accept both |

A lost key never strands a PC: Asima Rescue and "Repair this computer" work without the signing
keys, a new Secure Boot key needs one more approval on the blue screen. A PC whose versions no longer take updates (a new release key) gets the USB stick's
version from "Repair this computer" with `--install-this-version`: written into `@slots` (the
stick's version current, the newest one on the disk its backup, older ones removed for room),
files and settings kept, updates from then on with the stick's key.

## 8. Updates, health checks and automatic rollback

1. **Update file** (`.aosupd`, D18; tool and library: `services/asimaos-update`, command `aosupd`): a
   24-byte header, a text manifest (`VERSION`, `ARCH`, `CHANNEL`, `EXPIRES`, optional `MIN_FROM`,
   `UNPACKED_SIZE`, one `FILE` line per part with its size and two SHA-256 checksums), an Ed25519
   signature over header and manifest, then the parts at 4 KiB boundaries: `usr_<ver>.erofs`,
   `verity_<ver>.img`, `asimaos_<ver>.efi`, and optionally systemd-boot, shim with MokManager,
   Asima Rescue, migration steps, release notes and parts of new kinds. Parts are zstd-compressed
   with a 128 MiB window when that saves space (the `/usr` image shrinks to about 36 %). The file's
   length must match the manifest exactly and the padding must be zero, so every byte is signed,
   checksummed or known. **Format 1 is fixed (D31):** a part of a kind the installed updater
   doesn't know is checked like every other part and kept in `@slots/staged/<ver>/parts/<kind>/`
   for the new version's own code, so a new kind of part never needs a stepping-stone update.
2. **Asima Updater checks it:** the signature (by the current or the next release key), the
   architecture, that the version is newer (no downgrades) and not older than `MIN_FROM` needs, that
   the release hasn't expired, and free space ≥ max(10 GiB, 1.5 × `UNPACKED_SIZE`); then it hashes
   every part before writing anything. **`asimaos-updaterd` writes it:** a snapshot of the system
   first, then each part decompressed to a `.partial` file, hashed again while writing and renamed
   only when it matches; the start menu entry comes last, and it keeps two versions.
3. **Resume and reinstall (D21):** applying the same file again keeps every part already in place
   whose size and signed checksum match and writes only what is missing. "Reinstall this version"
   (the running version's own file) writes a fresh copy of every part with half the space of an
   update. Asima Rescue installs with `aosupd install`, the same code; it may also install an older
   version, after a warning (D35).
4. **Boot loader and Rescue wait for a good start:** an update's boot loader, shim and Asima Rescue
   are staged in `@slots/staged/<ver>` and installed by `asimaos-finish-update.service` only after
   `systemd-bless-boot` has marked the new version good. **A changed boot loader or shim starts once
   on trial:** it is copied to `EFI/asimaos/trial/` with a firmware boot entry of its own, and
   `BootNext` starts it at the next start only. If that start reaches Asima OS (systemd-boot's
   `LoaderImageIdentifier` names the trial folder), the trial copy replaces the old one; if not, the
   firmware starts the old boot loader again by itself, Asima OS tries once more, then keeps the
   old one. A boot loader the same as the installed one (most updates) is simply dropped. **A new
   shim is checked first** (`asimaos_svc::authenticode`): with Secure Boot on, it goes on trial only
   when one of its Authenticode signatures chains to a certificate in the firmware's `db` and neither
   its hash nor a signer is in `dbx` (`openssl smime -verify`, expiry dates ignored as firmware does);
   otherwise the installed shim and MokManager stay, and a new boot loader is still tried behind them.
5. **The start menu and boot counting:** systemd-boot shows **"Asima OS 2026.11.18.1 (current)"**,
   the backup below it and **"Asima Rescue"** last. Each is a Boot Loader Specification entry in
   `loader/entries/` (`asimaos-current.conf`, `asimaos-backup.conf`, `asimaos-rescue.conf`, and
   `asimaos-failed.conf` for an update that didn't start) pointing at its UKI in `EFI/Asima/`. An
   update becomes current with three tries (`asimaos-current+3-0.conf`) and the running version
   becomes the backup; `default asimaos-*` starts the first entry that still has tries, so a version
   that fails three times falls back to the backup by itself (and to Rescue without a backup). After
   a good start the working version is current again; a failed update stays "failed" until the next
   one. Code: `services/asimaos-update/src/boot.rs`, used by Asima Updater, the installer, the build
   and Rescue.
6. **Health checks before a start counts as good** (`/usr/lib/asimaos/asimaos-health`, results in
   `/run/asimaos/health/`):

   | Piece | Job |
   | --- | --- |
   | `asimaos-core-health.service` | `/usr` verified by dm-verity and the EROFS image, the `/etc` overlay, `/var` and `/home` on Btrfs, the security modules active |
   | `asimaos-desktop-health.service` | The login screen or an automatic sign-in is active on seat0 and stays up for 15 s without the login manager restarting; D-Bus, logind, polkit and journald running. A monitor that is off or unplugged never counts as a failure |
   | `asimaos-boot-timeout.timer` | Restarts a trial start that hasn't reached `boot-complete.target` after 15 minutes |
   | `10-asimaos-watchdog.conf` | `RuntimeWatchdogSec=30s`, `RebootWatchdogSec=2min` |

   Then `boot-complete.target` → `systemd-bless-boot` removes the counter. During a trial start a
   failed check restarts at once, so the three tries pass in minutes; a kernel panic restarts after
   10 s (`panic=10`), a hard freeze through the watchdog.
7. **After the login screen:** the sign-in safety net (`asimaos-desktop-check`, a user service) watches
   the first sessions of a new version. When the desktop shell or the window manager crashes again and
   again, or three sessions in a row end within two minutes, Asima Updater opens and offers going
   back to the backup.
8. **Going back by hand** in Asima Updater (Versions): "Make it the current version" swaps current
   and backup; "Start it once" starts the backup at the next start only (`bootctl set-oneshot`); a
   failed update can be tried again. Asima Rescue can start the backup once too. Going back never
   touches `/home`.
9. **Migration steps, also across skipped versions (D52):** an update carries the one-time steps
   for `/etc`, `/var`, the ESP and the subvolumes of its whole window: every step newer than its
   `MIN_FROM` (one year before it) up to its own version (`builder/migrations/<version>-<name>.sh`).
   Early in the new version's first start `asimaos-migrate.service` runs the steps newer than the
   version the settings are in (`/var/lib/asimaos/state-version`, noted at the first start of a fresh
   installation or reset, which needs no steps), each once, in version order. They must keep the
   previous version working, because going back reuses the same state. A computer older than
   `MIN_FROM`, or one that missed a change of the release key, updates through Asima Rescue or the
   USB stick's "Repair this computer", which brings the same steps; when its settings are older than
   they reach, Asima Maintenance → System changes suggests Reset settings. **Settings go
   back with a failed update:** when the backup takes over after an update whose migration steps ran,
   `asimaos-finish-update` marks it (`@data/flags/rollback`), and at the next start the early mount
   puts the `/etc` changes and `@var` back from the snapshot taken before the update, keeping the
   journal and crash records of the failed starts. Any failure there only logs; the start continues.
10. **Settings files after an update:** files the person changed whose factory default the update
    changed too are listed (`/var/lib/asimaos/etc-new-defaults`) and marked in Asima Maintenance →
    System changes.
11. **Everything on for everyone (D26):** NTSYNC and uinput load at every start, controllers are open
    to the person at the computer (`70-asimaos-gaming.rules`), no split-lock slow-down, libvirt's
    sockets and Podman's user socket are enabled.

## 9. Factory reset

The reset works like Android's (D7):

- **Result:** a fresh installation of the currently installed version.
- **Deleted:** all user files (`@home`), `@root`, all snapshots, all `/etc` changes (`@data`), `@var`,
  containers, VMs, Flatpaks, the Nix store, and the previous OS version. The language, keyboard,
  time zone and computer name go back to what Asima OS ships (English, the US layout, UTC,
  `asimaos`); `asimaos-keyboard-sync.service`, which runs at every start, brings the start-up layout
  back to US too, so the new passphrase is typed with it.
- **Kept:** only the current OS image, its UKI and the boot files on the ESP.
- **The fresh system's account** is asked for in Asima Maintenance before the reset starts (D10:
  there is no first-start setup). On encrypted disks the new passphrase and a new recovery key are
  enrolled first; the recovery key is shown once.
- **Method (D12):** in the initramfs at the next start, delete and recreate the subvolumes, TRIM all
  free space, and remove every old key slot. Without encryption (D9) the TRIM is what makes old data
  unrecoverable on SSDs.

## 10. Asima Rescue

One signed UKI, `EFI/Asima/asimaos-rescue.efi`, last in the start menu
(`builder/stages/55-rescue.sh`). The same kernel; its initramfs is a whole small system that never
switches to the installed one: systemd runs it from RAM, with `cryptsetup`, `veritysetup`,
`btrfs-progs`, `mokutil`, `openssl`, `systemd-cryptenroll`, `bootctl`, `aosupd`, the Rescue engine
(`asimaos-rescue-engine`, which runs `asimaos-engine rescue`) and the Rescue app. The app draws on the firmware framebuffer
(simpledrm, Qt `linuxfb`, software rendering), so no graphics driver is needed; GPU drivers,
networking, firmware, Mesa and LLVM are left out, and so are the drivers a PC's rescue never needs
(sensors, Bluetooth, server storage, network filesystems), the other Qt styles and screen backends,
and the tools that come with its libraries. ICU's data keeps only what an English app that shows
text needs: the build filters it with ICU's own tools (`icupkg`, `genccode`) and links the library
again under its own name, without the other languages, transliteration, character set converters
and the large line-break dictionaries (D55). A root shell is on
tty2 and the serial console; the disk still needs its passphrase or recovery key. It reads the
start menu with `aosupd boot list --plain`, the same code as Asima Updater. What it can do:
[`apps/DESIGN.md`](../apps/DESIGN.md#asima-rescue).

## 11. What goes into `/usr`

| Area | Packages |
| --- | --- |
| Boot, crypto, filesystems | systemd, Arch `linux` (D23), plymouth with the Asima OS start-up theme (D51; Plymouth's two-step module and images), zram-generator, mkinitcpio, btrfs-progs, cryptsetup, erofs-utils, dosfstools, exfatprogs, mokutil, gnupg, tpm2-tss, tpm2-tools, nvme-cli, amd-ucode, intel-ucode (loaded early by the `microcode` initramfs hook), sudo |
| Hardware, audio, security, power | linux-firmware, pipewire (+ pulse, alsa, jack), wireplumber, rtkit, sof-firmware, apparmor, nftables, networkmanager (DNS through systemd-resolved), bluez, fwupd, power-profiles-daemon, upower, ddcutil, i2c-tools, fprintd, v4l-utils, cups + cups-browsed + avahi |
| Graphics | mesa, vulkan-radeon, vulkan-intel, intel-media-driver; nvidia-open (prebuilt by Arch), nvidia-utils, nvidia-prime, switcheroo-control |
| Gaming plumbing | ntsync, gamemode (with Asima OS's hook that pauses the controller mouse during a game), scx-scheds + scx-tools (`scx_loader`, `scx_lavd` in gaming mode from the start, D44), Asima OS's uinput and controller rules (`70-asimaos-gaming.rules`, D26) |
| Developer plumbing (D28) | gdb, strace, perf, python, git, openssh, curl, wget, rsync, jq, ripgrep, restic, smartmontools, nano, man pages. Compilers and IDEs come from Distrobox, Nix or Flathub |
| Virtualization and sandboxes | qemu-desktop, libvirt (modular daemons, socket-activated, D26), dnsmasq, swtpm, edk2-ovmf, virtiofsd, podman, podman-docker, podman-compose, passt, distrobox, flatpak (Flathub preconfigured), fuse3, fuse2; snapd (built from its AUR package), nix and waydroid (D28) |
| Guest tools (D22) | spice-vdagent + phodav, qemu-guest-agent, virtualbox-guest-utils, open-vm-tools, hyperv: each starts only inside its own kind of virtual machine |
| Languages, input, accessibility | glibc-locales (every language precompiled), fcitx5 with m17n (Indic), Chinese, Mozc (Japanese) and Hangul, orca + espeak-ng |
| Desktop (D4, "KDE standard") | **Plasma 6.8:** every member of Arch's `plasma-meta` except `kup` and `plasma-firewall`, listed one by one. **Core KDE apps:** dolphin, konsole, ark, filelight, kate, okular, gwenview, spectacle, elisa, haruna, kcalc, kdeconnect, kwalletmanager, khelpcenter. Noto fonts (Bengali, Devanagari, CJK, emoji); Tesseract English data for Spectacle; Baloo indexes file names only. KDE's standard look, nothing changed (D48) |
| **Not in `/usr`** | Browsers, office suites, Steam and every other GUI app outside the KDE standard set: Flatpak |

**How apps get in:**

| Engine | In `/usr` | State | Out of the box |
| --- | --- | --- | --- |
| Flatpak | `flatpak`, `xdg-desktop-portal-kde` | `@flatpak` | Flathub set up, no apps installed |
| AppImage | `fuse2`, `fuse3` | `@home` | Asima App Center keeps them in `~/Applications` with a menu entry (D56) |
| Snap (D28) | `snapd` (pinned AUR revision, built in stage 12) with AppArmor profiles | `@var` | `snapd.socket` on, starts when first used |
| Nix (D28) | Nix daemon, `@nix` mount | `@nix` | `nix-daemon.socket` on; Asima App Center → Nix (D56) |
| Waydroid (D28) | `waydroid` | `@var` | Android is downloaded when set up in Asima App Center (D56) |

**KDE standard out of the box:** Plasma with System Settings, Discover (Flatpak only) with Flatpak
permission settings, System Monitor, printing, Plasma Vault, remote desktop (KRdp) and the welcome
app; Dolphin, Ark, Filelight, Konsole, Kate, Okular, Gwenview, Spectacle, Elisa, Haruna, KCalc, KDE
Connect, KWallet Manager, Help Center. **Left out on purpose:** KDE Partition Manager (it can
repartition Asima OS's write-once disk), Kup (Asima Maintenance does backups), plasma-firewall (Asima
Shield is the firewall) and the rest of KDE Gear; they are one click away in Discover. KDE full would
add 2.2 GB and 374 packages to every image and update file (KDE standard: 666 packages, 3.8 GB).

**Signed system extensions (D55):** programs can be added to `/usr` without changing the verified
image, as `systemd-sysext` images, and only signed ones are used. `asimaos-sysext.service` takes
the place of systemd's own units (`systemd-sysext.service` and `systemd-confext.service` are
masked, as they would also merge folders and change `/etc`): before merging it opens every image
in `/etc/extensions`, `/run/extensions`, `/var/lib/extensions` and `/usr/local/lib/extensions`
under the policy `root=signed+absent:usr=signed+absent`, which sets dm-verity up with the image's
signature, checked against the build's Secure Boot certificate (`/usr/lib/verity.d/asimaos.crt`)
or one an administrator added in `/etc/verity.d`. Folders, other files and images that fail are
moved to `/var/lib/asimaos/refused-extensions` with the reason, and Asima Maintenance → Security
lists them. An extension fits every Asima OS of its level (`SYSEXT_LEVEL=1` in os-release);
`builder/make-sysext NAME DIR` makes a signed one from a folder with a `usr/` tree.

## 12. Self-maintenance

| Service | Schedule | Job |
| --- | --- | --- |
| `asimaos-scrub.timer` | weekly (on AC power) | `btrfs scrub`: checks all data, repairs metadata from its second copy, lists damaged files so they can be restored from a backup or a snapshot |
| `fstrim.timer` | weekly | TRIM through LUKS to the SSD |
| `asimaos-balance.timer` | monthly (on AC power) | Gives back block groups under 20 % use (`-dusage=20 -musage=20`) |
| `asimaos-ssd-audit.timer` | weekly | Logs SSD wear (SMART `data_units_written`, SATA attribute 241, `percentage_used`) |
| `asimaos-snapshot.timer` | daily | Snapshots of `/home` and of the settings and accounts when one is due (daily, weekly, monthly or off, `/etc/asimaos/snapshots.conf`); the newest 14 kept, only 2 while free space is under 15 % |
| `asimaos-backup.timer` | daily | restic backup to a folder or a restic address, once set up (`/etc/asimaos/backup.conf`, password in `/etc/asimaos/restic.pw`, 0600); keeps 7 daily and 4 weekly; every run restores one file and compares it |
| zram + `systemd-oomd` | always | Compressed swap in memory (as big as the memory, at most 8 GiB, zstd); closes the app that keeps memory under heavy pressure (Fedora's defaults) |
| Emergency reserve | every start | Section 5 |

Before every update `asimaos-updaterd` takes read-only snapshots of `@home`, `@data`, `@var` and
`@root` in `@snapshots/update/<version>/` (the last two updates kept). Restoring a backup puts one
person's folder into a new `Restored YYYY-MM-DD` folder in their home; nothing is overwritten. The
scrub, balance and SSD jobs are `/usr/lib/asimaos/asimaos-maintenance` and write
`/var/lib/asimaos/maintenance/*.json` for Asima Maintenance → Health.

## 13. Hardware support and virtual machines

**Extra kernel modules (D55, stage 16):** v4l2loopback (one virtual camera, which video apps see
only while a program such as OBS Studio sends to it; it loads at start-up), vhba with CDEmu
(virtual CD/DVD drives for disc images, for older games; for the person at the computer),
xpadneo, xone and xpad-noone (below). They are built against the image's own kernel with the same
snapshot's `linux-headers`, which only the build container gets, and signed with the build's
Secure Boot key like NVIDIA's, so they load under lockdown too.

**Game controllers of every kind (D55):** XInput controllers and 2.4 GHz receivers (`xpad`, here
xpad-noone in place of the kernel's: Xbox 360 and original Xbox, wired and through the 360
receiver; XInput devices it doesn't know by ID are taught to it by `70-asimaos-controllers.rules`
from their USB interface, class ff/5d/01); Xbox One and Series controllers and accessories over
USB, and Microsoft's Xbox Wireless Adapter (`xone`; the adapter is taken from the Wi-Fi driver
`mt76x2u`, which knows its chip, by that rule alone, so USB Wi-Fi sticks with the chip keep their
driver); Xbox controllers over Bluetooth (`xpadneo`); PlayStation, Nintendo and DirectInput
controllers (the kernel's `hid-playstation`, `hid-nintendo`, `hid-generic`). The adapter's firmware
is Microsoft's: Asima Gaming downloads it from Windows Update when the person asks (the files and
SHA-256 sums xone's own install uses) into `/var/lib/asimaos/firmware`, where the kernel also looks
(`firmware_class.path` on the boot image's command line).

**Flatpak apps' caches in memory (D55, off unless switched on in Asima App Center):** a tmpfs at
`/run/asimaos/appcache` with its own limit (15 % of the memory), a folder per person made as they
sign in (`user@.service` drop-in), and per app the person's own Flatpak override:
`XDG_CACHE_HOME` there and access to that folder only. Game launchers and Bottles keep their
caches on the disk.

| Hardware | Support |
| --- | --- |
| AMD and Intel GPUs | Mesa, Vulkan and OpenGL |
| NVIDIA GTX 16xx / RTX 20xx and newer | nvidia-open, prebuilt by Arch; a release is blocked until Flathub has the matching driver extension |
| Older NVIDIA | nouveau (OpenGL) and NVK (Vulkan): desktop fine, gaming weak |
| Laptops with two GPUs | switcheroo-control, nvidia-prime |
| Controllers | Asima OS's controller rules and uinput |
| RGB lights the kernel drives | Asima Gaming → Lighting (D34) |
| A second GPU for virtual machines | Asima Dev; the IOMMU from a signed boot add-on, only on PCs that use it |

**Asima OS in a virtual machine (D22):** QEMU/KVM (virt-manager, GNOME Boxes, Proxmox), VirtualBox,
VMware and Hyper-V. The VM must start in UEFI mode (VirtualBox "Enable EFI", VMware "UEFI", Hyper-V
"Generation 2"); with Secure Boot on, the Asima OS key is approved once on the blue screen (Hyper-V:
the "Microsoft UEFI Certificate Authority" template). At least 4 GB RAM and a 48 GB disk. The
initramfs has no `autodetect`, so the virtual disk drivers (virtio, AHCI, NVMe, VMware PVSCSI,
Hyper-V storvsc) are always there; graphics use virtio-gpu, vmwgfx and hyperv_drm. The guest tools
start only inside their own kind of VM (`ConditionVirtualization=` or their device), and so do the
desktop agents of VirtualBox and VMware (user-unit drop-ins).
