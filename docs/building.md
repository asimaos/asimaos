# Building and testing

Everything is built in rootless Podman containers from a pinned Arch Linux archive snapshot, so a
build needs no sudo and gives the same packages every time.

## 1. Requirements

| Need | Why |
| --- | --- |
| Linux on x86_64 with **rootless Podman** | The two build containers (`builder/`, `apps/Containerfile`) |
| **KVM** (`/dev/kvm`), `qemu-system-x86_64`, **edk2-ovmf** | The automatic boot tests and `testbed/run-qemu.sh` (the scripts use Fedora's OVMF paths in `/usr/share/edk2/ovmf/`) |
| `python3` | `testbed/boot-test.py` |
| `dnf`, `rpm2cpio`, `cpio` (a Fedora host) | Optional: Fedora's Microsoft-signed shim for Secure Boot (`builder/fetch-shim.sh`). Without them the build still works and systemd-boot starts directly |
| About 60 GB free disk space | Package cache (3 GB), the image tree (9 GB), staging (9 GB), outputs (about 30 GB, of which the encrypted disk image is 18 GB) |
| Network access | The Arch archive snapshot, Rust crates, the face models and the face tests' two sample photos, Flathub (NVIDIA check). When the archive can't be reached, `ASIMAOS_OFFLINE=1 ./build.sh` builds from the packages an earlier build of the same snapshot left in `build/pkgcache` (a local repository; signatures still checked) |
| Memory | `apps/build.sh` runs one compile job per 1.5 GB of free memory, at most one per core, so a build never runs the machine out of memory; `ASIMAOS_JOBS=N` sets the number |

The container images (`localhost/asimaos-builder:…`, `localhost/asimaos-apps-builder:…`) are made
automatically on first use. **Run one build container at a time:** each one relabels the project
folder for SELinux (`:Z`), which cuts off a container that is already running.

## 2. Configuration and keys

- **`builder/asimaos.conf`**: the archive snapshot date, the version (`YYYY.MM.DD.N`) and its label (shown
  only inside the system, never in the start menu), the package list, and TEST-ONLY passwords for
  the prebuilt disk image.
- **`build/keys/`** is made on the first build. It never goes into any repository (D25); keep it
  in an encrypted backup:
  - `release.key` / `release.pub`: Ed25519, signs the update files. An installed system only
    accepts updates signed with the key of the build it came from.
  - `secureboot.key` / `secureboot.crt`: RSA-2048, signs systemd-boot, every UKI and Asima
    Rescue. Its certificate is approved once on the blue MokManager screen.
  - `release-next.key` / `release-next.pub` (optional): a prepared next release key. When
    it exists, builds ship its public half too, so their systems also accept updates signed with
    it; later builds can then sign with it and the old key can be retired without reinstalling.
    Make it with `aosupd keygen build/keys/release-next`.
  - `feed.key` / `feed.pub`: Ed25519, signs Asima Hub's feed (news, releases, app lists) on the
    website, never the OS. Unlike the others it is not made by the build: it is the project's one
    feed key (the website's `scripts/make-feed --new-key` makes it), and its public half is built
    into Asima Hub.

  **Back this folder up.** If it is lost, systems installed from this build can only move to a
  newer version by reinstalling. On a new build machine, restore `build/keys/` from the encrypted
  archive (secret keys `0600`) before the first build, or the build makes new keys.

## 3. Build

```bash
apps/build.sh dist     # the Asima OS apps and services → build/asimaos-dist
apps/build.sh translations   # collect new app strings into apps/i18n/asimaos_<language>.ts
./build.sh             # every stage → build/out
./build.sh 50 55 60    # or only some stages, in the order given
```

| Stage | Does |
| --- | --- |
| (host) `builder/fetch-shim.sh` | Fedora's Microsoft-signed shim into `build/shim/` (RPM signature checked); skipped when present |
| `10-rootfs.sh` | Installs the pinned packages into `build/rootfs`, checks the kernel has every option Asima OS needs (`builder/check-kernel`, from the kernel's own configuration), then copies `builder/rootfs/` over them |
| `12-snapd.sh` | Snap (D28): builds snapd from the AUR revision pinned in `builder/asimaos.conf` (once; the package is kept in `build/aur/`), installs it and gives snap-confine its capabilities |
| `15-asimaos.sh` | The Asima OS apps and services, the build's keys (release, Secure Boot), the NVIDIA modules signed with the Secure Boot key, the NVIDIA–Flathub check, services enabled at start, system defaults, root rights taken from unused programs, the disk guard's links |
| `16-modules.sh` | Extra kernel modules (D55): v4l2loopback and vhba from Arch's DKMS packages; xpadneo, xone and xpad-noone (as `xpad`, in place of the kernel's) from pinned commits (`ASIMAOS_XPADNEO`, `ASIMAOS_XONE`, `ASIMAOS_XPAD_NOONE`), built against the image's kernel with `linux-headers` from the same snapshot (only in the build container), signed with the Secure Boot key, installed in `updates/asimaos` |
| `20-initrd.sh` | The generic initramfs (mkinitcpio, no `autodetect`) |
| `30-finalize.sh` | `os-release`, the image's `/var` folders listed for systemd-tmpfiles (`asimaos-var.conf`), `/etc` → `/usr/share/factory/etc`, pacman removed |
| `40-seal-usr.sh` | `/usr` as an EROFS image (LZ4HC) + dm-verity hash tree |
| `50-uki.sh` | The UKI and systemd-boot, signed with the build's Secure Boot key |
| `55-rescue.sh` | Asima Rescue: a signed UKI with a small system that runs from RAM |
| `60-disk.sh` | A whole-disk image (LUKS2 + Btrfs pool) for quick tests |
| `70-iso.sh` | The hybrid live ISO with the installer |
| `80-update.sh` | The same version as a signed `.aosupd` update file, checked like Asima Updater does |

**Outputs in `build/out/`:** `asimaos-<version>-x86_64.iso`, `asimaos-<version>-x86_64.aosupd`
(+ `.json` report), `asimaos-disk.raw`, the `/usr` image and its verity data, the UKIs, the signed
systemd-boot and `shim/` (shim, MokManager, `asimaos.der`). For publishing: `SHA256SUMS` and its
signature `SHA256SUMS.sig`, the release key as `asimaos-release-key.pem` with its
fingerprint in `asimaos-release-key.txt`, and `asimaos-<version>-packages.tsv`, every package with
its licences and sources (also in the image and on the ISO).

**Size limits:** `builder/asimaos.conf` sets the largest /usr image, ISO and update file; a build
that outgrows one fails, so growth is a decision. Each build ends with the time every stage took.

**Partial rebuilds:** `builder/rootfs/` is copied in stage 10, so after changing a file there rebuild from
stage 10 (or copy the file into `build/rootfs/` as well and rebuild from stage 20). After changing
the apps or services: `apps/build.sh dist && ./build.sh 15 30 40 50 55 60 70 80`.

## 4. Test

```bash
apps/build.sh services-test   # unit tests + end-to-end on a private D-Bus bus, GUIs against the services
apps/build.sh e2e             # the installer GUI → real engine, dry run (needs an OS build)
apps/build.sh screenshots     # render every app screen, light and dark, into apps/screenshots/ (not in git)
python3 testbed/boot-test.py    # QEMU tests, see below
```

| `testbed/boot-test.py` | Runs |
| --- | --- |
| (no option) | The disk image: the installed system, everything on, guest tools, health checks, start menu, restart, Asima Rescue started once |
| `--secureboot` | The same with Secure Boot enforced: OVMF with Microsoft's keys plus the build's certificate in db |
| `--iso` | The live session → encrypted install → the installed system → restart |
| `--iso --plain` | The same without encryption (D9) |
| `--iso --update V2 V3` | After the encrypted install: a power cut while updating to V2, then V2 again; a kernel crash and a freeze during V2's trial starts (hardware watchdog); start the backup once, go back and forward; the broken V3 must fall back by itself; Asima Maintenance lists the failed starts and the crash |
| `--trust` | Firmware that trusts only Microsoft's 2023 CA, the Asima OS key as a MOK, a TPM: lockdown, the signed NVIDIA driver, the start-up keyboard layout, TPM unlock with signed PCR 11; Asima Rescue and the running system can't unseal |
| `--repair` | The ESP formatted and an account deleted after a snapshot; `asimaos-repair --install-this-version` from the USB stick; it starts again with the stick's version, the account back and the files kept |
| `--own-keys` | Own Secure Boot keys: prepared with Secure Boot on, the Platform Key cleared (Setup Mode, as the person does in the firmware), the keys written, then a start with Secure Boot on |
| `--reset` | Reset settings (the system's and a person's settings back to the defaults at the start), then the factory reset staged in Asima Maintenance and run at the start |
| `--rescue` | Asima Rescue unlocks the disk and reinstalls from a USB drive |
| `--flathub` | Flathub out of the box: an app installs and starts (the VM needs the internet) |
| `--menu-shot` | A picture of the disk image's start menu |

On a slower host, for example a build machine that is itself a virtual machine with about 8 GB
of memory, set `ASIMAOS_TEST_SLOW=3` so every wait lasts three times as long. A test that stops
early stops all its VMs too.

The Secure Boot test needs `virt-fw-vars` once:

```bash
python3 -m venv build/tools/virt-fw
build/tools/virt-fw/bin/pip install --no-deps virt-firmware
build/tools/virt-fw/bin/pip install cryptography pefile
```

**Test versions for the update tests** are the same image with a later version number, made after a
normal build:

```bash
ASIMAOS_VERSION_OVERRIDE=2026.11.18.2 ./build.sh 30 40 50 80                          # a good update
ASIMAOS_VERSION_OVERRIDE=2026.11.18.3 ASIMAOS_TEST_BREAK=login ./build.sh 30 40 50 80  # broken on purpose
./build.sh 30                                                                         # back to normal
python3 testbed/boot-test.py --iso --update 2026.11.18.2 2026.11.18.3
```

`ASIMAOS_NOTES_EXTRA` adds a line to a version's release notes.

## 5. Try it

```bash
testbed/run-qemu.sh build/out/asimaos-<version>-x86_64.iso   # live ISO + a 64 GB test disk (sparse)
testbed/run-qemu.sh                                            # the installed test disk
testbed/run-qemu.sh --disc updates.iso                         # … with a DVD holding update files
testbed/run-qemu.sh --secureboot --fresh build/out/asimaos-<version>-x86_64.iso   # approve the key (MokManager)
```

Any other virtual machine works too: UEFI firmware, at least 4 GB memory and a 48 GB disk (the
installer's minimum; updates need 10 GB free).

**Test credentials (TEST only):** the live session has no password. The prebuilt disk image:
disk passphrase `asimaos`, console login `root` / `asimaos`. Installed systems use the account and
passphrase chosen in the installer (root is locked).

## 6. Troubleshooting

- **"Permission denied" in a build container**: another container relabelled the folder, or the
  Flatpak build was writing into `build/` while the container started; run one build at a time
  and start again.
- **"No space left on device" in stage 60**: the encrypted disk image is written in full (about
  17 GB) next to a temporary copy of the same size, so stage 60 needs about 35 GB free. Old test
  disks (`build/test-*.raw`) can always be deleted; a test disk left by a stopped test keeps the old
  disk image's blocks.
- **Deleting files under `build/`** (they belong to sub-UIDs of the container): use
  `podman unshare rm -rf build/rootfs`.
- **The firmware can't read the virtual DVD** right after a large file was deleted: btrfs is still
  freeing space, in bursts. The ISO tests wait until the disk has been quiet for 20 s, and start
  the VM again (up to three tries) when the firmware says "No bootable option".
- **The Secure Boot tests stop at once at `virt-fw-vars`**: its Python environment in
  `build/tools/virt-fw` remembers the folder it was made in, so it breaks when the checkout moves.
  Make it again with the three commands in §4.
- **Secure Boot tests and the TPM**: use the 4 MB OVMF build (`OVMF_CODE_4M.secboot.qcow2`); the
  2 MB build measures nothing into the TPM, so TPM unlock can't work there.

### Lessons that cost time

- systemd runs every `Exec*=` line in its own mount namespace (with `PrivateMounts=`): a mount in
  `ExecStartPre=` is gone in `ExecStart=`. Mount and `exec` in one wrapper (`asimaos-esp`).
- An installed `/var` starts empty: stage 30 writes tmpfiles lines for every `/var` folder of the image.
- QEMU's emulated TPM is ready at once; a PC's firmware TPM may come up seconds later. The
  initramfs needs `tpm2.target` and `systemd-tpm2-generator`, or TPM unlock fails only on real
  hardware. Read what a build really contains (the initramfs list).
- `systemd-cryptenroll --tpm2-pcrlock=` also binds the slot to a signed PCR 11 policy whenever a
  public key lies in `/run/systemd` (systemd-stub puts a boot image's `.pcrpkey` there): Asima
  Maintenance passes `--tpm2-public-key=` (empty) for the pcrlock policy alone. Try a new slot with
  `cryptsetup open --test-passphrase --token-only --token-id N`; its debug output names the policy.
- A TPM slot with a PIN: systemd-cryptsetup asks for a "LUKS2 token PIN" (credential
  `cryptsetup.luks2-pin`, not `cryptsetup.tpm2-pin`); its prompts go to the last `console=` (the
  screen), never to the tests' serial log, so a test with the wrong credential just waits. One wrong
  PIN can use up swtpm's three tries (lockout, 1000 s each); the passphrase still opens the disk.
- A `console=ttyS0` on the kernel command line puts Plymouth in text mode unless
  `plymouth.ignore-serial-consoles` is there too.
- `busctl` gives up after 25 s: use `--timeout=` for long calls (TPM set-up takes minutes).
- `pgrep -f` / `pkill -f` patterns match the calling shell itself: write them as `[x]yz`.
- The apps container has no `cmp`: compare files with `sha256sum`.
- A build machine that is itself a VM (about 8 GB, nested KVM) is too slow for the tests' default
  waits: run them with `ASIMAOS_TEST_SLOW=3`, one at a time, and check no test VM is left running.
