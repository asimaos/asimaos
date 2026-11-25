# What works today

What Asima OS can do, what it can't by design, and every test with its result. The first release
is ready when every test passes, including on real AMD and Intel PCs.

The results so far come from the internal test builds made before the first Asima OS build (named
below by their version or as "beta N"); every suite runs again on the first Asima OS build.

**Status:** ✅ passed (where) · 🟢 passed in QEMU or checked by the build, with one part still
open (named in the row, often real hardware) · 🟡 partly: built, or passed only in part · ⏳ not
built or not tested yet. The automated suites behind the results are listed [in section 8](#8-automated-suites),
the results on real PCs [in section 9](#9-on-real-hardware).

## 1. At a glance

| Asima OS … | Status | Tests |
| --- | --- | --- |
| **Never breaks** | | |
| swaps the whole OS in one go; there are no big upgrades | ✅ | UPD-Test-1, SysApp-Test-4 |
| goes back by itself when an update fails to start (crash, freeze, no login screen) | ✅ | OS-Test-5 |
| goes back by hand, for one start or for good | ✅ | the update suite |
| offers going back when the desktop keeps crashing after sign-in | ⏳ | SAFE-Test-1 |
| keeps its system files from being changed secretly (dm-verity) | ✅ | every boot test |
| takes only signed updates of its own build, never older or expired ones | ✅ | SysApp-Test-3 |
| never runs out of space during an update; keeps a 2 GB emergency reserve | 🟡 | SysApp-Test-3, Gate 2 |
| grows or shrinks without repartitioning, for the SSD's whole life | 🟡 | SysApp-Gate 1, ROLL-Test-1 |
| **Recovery** | | |
| puts every setting back to the defaults, keeping files and apps | 🟡 | SysApp-Test-13 |
| resets to a fresh installation of the current version, keeping nothing | ✅ | SysApp-Test-6 |
| repairs itself without a USB stick (Asima Rescue) | ✅ QEMU · ⏳ real firmware | SysApp-Test-9 |
| starts again after a lost boot partition, from the USB stick ("Repair this computer") | ✅ | BRK-Test-3 |
| takes snapshots before every update and on a schedule | ✅ before updates · 🟡 scheduled | the update suite, services-test |
| backs up to a drive, a network share or a restic address, with restore tests | 🟡 | SysApp-Test-7 |
| looks after itself: scrub, TRIM, balance, SSD wear, out-of-memory guard | 🟡 | OS-Test-10 |
| **Security** | | |
| encrypts the disk by default, or not, as chosen at install | ✅ | OS-Test-1 |
| unlocks with the TPM, also after updates, and refuses after tampering | 🟡 in QEMU; not yet on a real PC (D43 in the next build) | OS-Test-3, TPM-Test-2 |
| starts with Secure Boot through Microsoft's shim and the Asima OS key | ✅ QEMU and a real PC | OS-Test-2, section 9 |
| loads only signed drivers where the Asima OS key is approved | 🟢 | OS-Test-4 |
| uses fingerprint and face only when the person chooses (D37) | 🟡 | BIO-Test-1, BIO-Test-2 |
| has firewall zones and a VPN kill-switch | 🟡 | SysApp-Test-10 |
| **Development** | | |
| runs any distribution's tools in containers, and Nix programs | 🟡 | APP-Test-1 |
| makes virtual machines, Windows 11 too, and gives one a graphics card | ⏳ | VM-Test-5, GPU-Test-1 |
| runs Android apps (Waydroid) | 🟡 | APP-Test-1 |
| **Gaming** | | |
| runs Steam, Proton, Heroic and Lutris from Flathub | 🟡 | APP-Test-2, OS-Test-8 |
| keeps NVIDIA's Flatpak driver in step with the system's | 🟢 | OS-Test-9 |
| has NTSYNC, uinput and the sched-ext schedulers on for everyone | ✅ | OS-Test-8 |
| works with controllers, two GPUs, HDR and VRR | 🟡 real hardware | OS-Test-8, SysApp-Test-10 |
| **Everyday use and install** | | |
| is KDE Plasma 6 with the KDE standard apps and KDE's standard look | 🟢 | every boot test, LOOK-Test-1 |
| installs apps from Discover (Flathub) | ✅ | APP-Test-2 |
| is one ISO: try it live, then install; the installer refuses the wrong disks | ✅ | SysApp-Test-1, the ISO suites |
| continues an interrupted update; reinstalls the running version | ✅ · 🟢 | UPD-Test-1, UPD-Test-2 |
| installs and runs in virtual machines | ✅ QEMU · ⏳ VirtualBox, VMware, Hyper-V | VM-Test-1 to 4 |
| is built by one command from a pinned snapshot, with VM tests | ✅ | DevApp-Test-6 |

## 2. Limits by design

- No dual boot on the same disk: Asima OS takes the whole disk (D31).
- No central release key (D25): a system takes updates only from the build it came from. If a build's
  release key is lost, its PCs keep working, and "Repair this computer" installs a newer USB stick's
  version on them, keeping files and settings.
- No hibernation (sleep works).
- No packages installed on the system itself: Flatpak, Snap, AppImage, Nix and containers instead;
  system changes come only with updates.
- No drivers added by hand (DKMS, VirtualBox's host drivers, some USB Wi-Fi drivers): only the
  drivers Asima OS ships.
- Automatic going back covers failures up to the login screen; after sign-in, the safety net only
  offers it.
- Damaged personal files are found but not repaired (one data copy): restore them from a backup or
  a snapshot.
- A factory reset keeps nothing. Back up first.
- The OS never announces updates (D2).
- x86_64 only (D1).
- After a firmware or Secure Boot database update, the recovery key may be needed once.
- Secure Boot needs the key approved once on the blue screen, until Asima OS has its own
  Microsoft-signed shim.
- Games whose anti-cheat blocks Linux don't run.
- Older NVIDIA cards (before GTX 16xx / RTX 20xx): the desktop is fine, gaming is weak.

## 3. Gates

Gates prove the risky mechanics before the parts that depend on them are built.

| Gate | What must be proven | Status |
| --- | --- | --- |
| Gate 0.0: bootstrap | Throwaway test keys for the four signing identities; a pinned builder with systemd ≥ 262 and ukify; a QEMU fixture with Secure Boot firmware (2011 + 2023 Microsoft CAs) and an emulated TPM (swtpm) | 🟢 beta 5: the pinned builder, QEMU with the 4 MB Secure Boot OVMF (Microsoft's 2011 + 2023 CAs, or only the 2023 CA) and swtpm; each build makes its own keys (D25) |
| Gate 1: boot slots | Under dm-flakey and dm-log-writes power-cut replay: boot counting (`+3-0`), `panic=10`, the hardware watchdog and the login-screen health check recover from kernel panics, hard freezes and display crash loops | ✅ beta 5 in QEMU: boot counting falls back after a broken login screen; a kernel panic (`panic=10`) and a total freeze (hardware watchdog `i6300esb`) during trial starts each count as a failed try, and the third try is marked good; a power cut while writing an update leaves the old version starting (dm-flakey replay itself not used) |
| Gate 2: storage | EROFS + dm-verity `/usr`; `@slots` mounted read-only while its subvolume stays writable; the 2 GB ballast is truncated in early boot when data or metadata space runs out; `@snapshots` read-write for root, read-only at `/home/.snapshots` | 🟢 beta 3 in QEMU: EROFS + verity, `@slots` read-only, the ballast made at the first start and released when space runs out (parser checked; the release itself not yet forced in a VM), `/home/.snapshots` read-only |
| Gate 3: kernel trust | With the standard kernel (D23) and `lockdown=integrity` + `module.sig_enforce=1` in the signed UKI: NVIDIA modules re-signed with the Asima OS key after stripping load; unsigned or pre-strip-signed ones fail with `-EKEYREJECTED` | ✅ beta 5 in QEMU: NVIDIA modules signed with the build's key pass the kernel's check under `lockdown=integrity` once the key is approved as a MOK (machine keyring); lockdown is switched on in the initramfs only where that is true |
| Gate 4: Secure Boot + TPM on hardware | Path A (own keys, `sbctl enroll-keys --microsoft`) and Path B (Microsoft-signed shim + Asima OS key, D16) on AMD fTPM and Intel PTT machines; the pool unlocks with the TPM (systemd-pcrlock, PCR 4 + 7, D43) | 🟡 QEMU: Path B on firmware that trusts only the 2023 CA, TPM unlock with the pcrlock policy (2026.11.24.1); Path A (own keys, `--own-keys`) 4/4 in QEMU (2026.11.24.1); real AMD and Intel hardware not yet |
| SysApp-Gate 1: updates | The update file is checked and rejected when tampered; activation survives power cuts (dm-flakey); an elastic update grows and shrinks the OS (13 GB → 67 GB → 35 GB) without running out of space | 🟢 beta 5: file checks (service tests) and a real power cut in the middle of an update in QEMU: the old version starts and the same file finishes the update; the elastic size test not yet |
| SysApp-Gate 2: blessing and forensics | Login-screen health check, `panic=10`, watchdog and boot counting fall back, and Asima Health keeps the crash log | ✅ beta 5 in QEMU: login-screen health check, panic, freeze and boot counting fall back; Asima Maintenance lists the failed starts and the kernel crash log (`efi_pstore` → systemd-pstore) |
| SysApp-Gate 3: installer and TPM | Installer disk guards, 10 subvolumes + 2 GB `@reserve`, the recovery key tested right after enrolment, two-phase TPM key rotation on 2023-CA firmware | 🟡 guards, 11 subvolumes (`@nix` since beta 5), recovery key; TPM enrolment on a VM (beta 5); two-phase rotation not built |
| DevApp-Gate 1: driver signing | Test signing keys; `verify-kconfig` checks the standard kernel (D23); nvidia-open re-signed after stripping; under `lockdown=integrity` post-strip-signed modules load and pre-strip-signed ones fail | 🟢 beta 5: nvidia-open signed after Arch's strip (stage 15) loads under lockdown; `verify-kconfig` not built |
| DevApp-Gate 2: signed UKI | A test UKI with CPU microcode and `.sbat`, signed for Secure Boot; TPM unlock in QEMU | 🟢 beta 5: signed UKIs with `.sbat`; TPM unlock in QEMU passes |
| DevApp-Gate 3: sealing | EROFS + verity from the build; a synthetic 67 GB image to check `UNPACKED_SIZE` handling | 🟡 EROFS + verity (build stage 40); 67 GB test not yet |

## 4. Core OS tests

| Test | Checks | Pass when | Status |
| --- | --- | --- | --- |
| OS-Test-1 | One shared UKI opens the pool | The same signed UKI opens the pool on the disk it started from, with TRIM allowed: encrypted `crypt_pool`, or plain `asimaos_pool` (D9) | ✅ in QEMU: encrypted with discards (every boot test) and plain (`--iso --plain`) |
| OS-Test-2 | Both Secure Boot paths | Path B (Microsoft-signed shim + Asima OS key, the default, D16) and Path A (own keys) both start on firmware that trusts only the 2023 Microsoft CAs | 🟢 beta 5 in QEMU: shim → systemd-boot → UKI on firmware that trusts only the 2023 CA (Fedora's shim is signed with both CAs), the Asima OS key as a MOK; the blue MokManager screen by hand on a real PC (beta 5, [section 9](#9-on-real-hardware)); Path A (own keys in the firmware, Asima Maintenance → Security) built, its VM test (`--own-keys`) runs with the next build |
| OS-Test-3 | TPM unlock | The systemd-pcrlock policy (PCR 4 + 7, D43) unlocks the pool, keeps doing so after an update, and never opens it for Asima Rescue | 🟢 QEMU (2026.11.24.1: the TPM opened the disk with no passphrase, Rescue can't); on the owner's PC with the next build. It replaced the signed PCR 11 policy, which never passed on the owner's PC |
| OS-Test-4 | Driver signatures and lockdown | With the standard kernel's own options (D23), unsigned or pre-strip-signed `.ko` files are refused with `-EKEYREJECTED` under `lockdown=integrity`, and every driver Asima OS ships loads | 🟢 beta 5 in QEMU: lockdown (integrity) on, the signed NVIDIA driver passes the check; an unsigned module was not provoked |
| OS-Test-5 | Panic, freeze, broken login screen | `panic=10`, the watchdog (`RuntimeWatchdogSec=30s`) and the `+3-0` counter fall back to the previous version cleanly | ✅ beta 5 in QEMU (`--iso --update`): a kernel crash and a total freeze during trial starts, then a good third try; a broken update (no login screen) tried 3 times, then the backup starts and becomes current again |
| OS-Test-6 | Blessing at the login screen | An unattended start is marked good once the plasma-login-manager screen (D11) has stayed up for 15 s | ✅ beta 3 in QEMU: `asimaos-desktop-health` marks a start good after the login screen stayed up 15 s |
| OS-Test-7 | 10 subvolumes and `@reserve` | All 10 mount; `@slots` read-only at the mount, writable as a subvolume; `@snapshots` read-write for root and read-only for users | ✅ beta 5 in QEMU: all 11 mount (`@nix` added); `@slots` read-only and not compressed twice; `/home/.snapshots` read-only for users |
| OS-Test-8 | Gaming and hybrid GPUs | Steam/Proton from Flatpak (32-bit graphics from the Flatpak runtime, D5), NTSYNC, `/dev/uinput` (on for everyone, D26) and switching between two GPUs work | 🟡 beta 3: NTSYNC (0666), `/dev/uinput` for the person at the computer, no split-lock slow-down (QEMU); Steam and hybrid GPUs need hardware |
| OS-Test-9 | Flatpak NVIDIA driver match | The Flathub GL extension matches the host nvidia-open; Flatpak apps keep GPU acceleration | 🟢 beta 3: every build checks Flathub for the extension (620.70.06: found); a release stops without it |
| OS-Test-10 | Self-maintenance | Weekly scrub, TRIM through LUKS, monthly balance, the SSD wear log and systemd-oomd run and pass | 🟡 beta 3: the scrub, balance, SSD-wear and TRIM timers, zram and systemd-oomd are on (QEMU); their first scheduled runs are not observed yet |

## 5. App tests

| Test | App | Scenario | Pass when | Status |
| --- | --- | --- | --- | --- |
| SysApp-Test-1 | Live Installer | The live medium and mounted disks | Refuses those disks. *Resuming an install after a power cut is not required (D21).* | ✅ refuses them (the ISO suites) |
| SysApp-Test-2 | Asima Hub | Mirror or protocol change; sandbox | Downloads without root (into Downloads), checked against the feed's SHA-256; Flatpak updates it first when hosting changes | 🟡 built in beta 5 (in `apps/asimaos-hub` since D32), tested with sample data and the signed feed on GitHub Pages; no version published in the feed yet |
| SysApp-Test-3 | Asima Updater: updates | Bad files; an elastic update | Rejects tampered, foreign, cut-off, damaged, older, expired and too-early files; grows 13 GB → 67 GB → 35 GB without running out of space | 🟡 all rejections pass (31 service checks); the elastic test is not run |
| SysApp-Test-4 | Asima Updater: updates | Power cut while writing a new version (dm-flakey replay) | Starts cleanly into the new or the previous version; applying the same file again finishes the update (D21) | 🟢 beta 5 in QEMU: the power is cut while the update is written; the old version starts and is still current, the same file then finishes the update (dm-flakey replay not used) |
| SysApp-Test-5 | Asima Maintenance: health | Panic, hard freeze or broken login screen | Watchdog and boot counting fall back; Asima Health keeps the pstore crash log | ✅ beta 5 in QEMU: boot counting, the health checks and the watchdog fall back; Asima Maintenance → Health lists the failed starts and the kernel crash log |
| SysApp-Test-6 | Asima Maintenance: reset | Factory reset (D13: one reset) | The computer is a fresh installation of the current version, with the account chosen beforehand, new disk keys and nothing else kept: language, keyboard (also at start-up), time zone and computer name are the shipped ones | ✅ beta 5 in QEMU (`--reset`): staged in Asima Maintenance, run at the start: only the fresh account, nothing old, the same version, the old passphrase refused |
| SysApp-Test-7 | Asima Maintenance: backups | A damaged single-copy file | Scrub reports it; restic restores it | 🟡 beta 3: scrub reports damaged files, restic backups with a restore test pass the service tests; the damaged-file scenario not run |
| SysApp-Test-8 | Asima Maintenance: security | Recovery key and TPM key rotation | The recovery key is tested before anything else; the old TPM slot stays until a restart proves the new one | 🟡 QEMU (2026.11.24.1): TPM set-up and unlock, the recovery key at install; the two-step rotation is built, its check failed once and is fixed, to test again |
| SysApp-Test-9 | Asima Rescue | Start in RAM from the ESP; reinstall from a USB update file | Starts without the installed `/usr`, unlocks the pool, reinstalls Asima OS | ✅ beta 5 in QEMU (`--rescue`): Rescue starts from RAM, unlocks with the passphrase, reinstalls from a USB drive, Asima OS starts verified afterwards; it can't unseal the TPM key |
| SysApp-Test-10 | Asima Manager: Shield; Asima Gaming: GPUs | Firewall zone switch; two GPUs; external monitor brightness | The nftables zone switches atomically; GPU switching and `ddcutil` work | 🟡 zones applied by the service (service tests, and at every start in the boot tests); hardware parts not tested |
| SysApp-Test-11 | Asima Maintenance: Flatpak; Asima Manager: power | Flatpak cache in RAM; 80 % charge limit, or Lenovo's battery care | Sandboxed apps use `<app ID>/cache`; charging stops at the limit, also after a restart | 🟡 the charge limit and Lenovo's battery care (service checks on a test tree); didn't work on the owner's Lenovo with 2026.11.21.1, fixed for the next build; the Flatpak cache not tested |
| SysApp-Test-12 | Asima Maintenance: System changes | One broken `/etc` file | That one file returns to the factory version without a reset | 🟡 restore is staged (service checks) and applied at the next start; the start-up step has not run on a VM yet |
| SysApp-Test-13 | Asima Maintenance: Reset settings | Every setting back to the defaults in one step | `/etc` and the person's desktop and app settings return to this version's defaults at the next start, and the desktop is plain KDE again (themes and widgets installed since are moved aside too); files, fonts, accounts, networks, language, keyboard, time zone and other apps' settings stay; the old settings are kept in a folder | 🟡 in the source: service checks and the person's part on a test tree; the VM test (`--reset`) runs with the next build |

## 6. Build and signing tests

| Test | Area | Scenario | Pass when | Status |
| --- | --- | --- | --- | --- |
| DevApp-Test-1 | Build | The standard kernel lacks something Asima OS needs (D23) | `verify-kconfig` stops the build with an error | ⏳ |
| DevApp-Test-2 | Signing | Re-signing NVIDIA's prebuilt modules: pre-strip vs post-strip | Post-strip-signed modules load under lockdown; pre-strip-signed ones fail with `-EKEYREJECTED` | 🟢 beta 5: modules signed after the strip load under lockdown; a module signed before the strip was not provoked |
| DevApp-Test-3 | Signing | Signed UKI | `ukify inspect` shows `.sbat`, and the Secure Boot signature verifies | 🟢 beta 5 |
| DevApp-Test-4 | Release check | Flathub has no matching NVIDIA driver extension | `check-nvidia` blocks the release | 🟢 beta 3: build stage 15 checks Flathub (betas warn, a release stops) |
| DevApp-Test-5 | Build | Hermetic `/usr` | `/etc` lives in `/usr/share/factory/etc`; no pacman in the image | ✅ build stage 30, checked at every boot test |
| DevApp-Test-6 | Build | QEMU gate with Secure Boot and TPM | Signed UKI and Rescue start under OVMF Secure Boot (2023 CAs) + swtpm, unlock and reach `boot-complete.target` | ✅ beta 5 (`--trust`): signed UKI and Rescue start under the 4 MB Secure Boot OVMF with only the 2023 CA + swtpm; the pool unlocks with the TPM and the start reaches `boot-complete.target` |
| DevApp-Test-7 | Build | Private keys and the repository (D25: each build has its own keys) | No private key ever enters a repository or a published archive (D50: they are kept in an encrypted archive) | ✅ `build/` and `keys/` are git-ignored; each build makes its own keys (D25); a local guard refuses key files and key contents on every push (tested with fake keys) |
| DevApp-Test-8 | Build | Repeatable builds | Two builds from the same snapshot give the same package tree | ⏳ not verified yet (EROFS images already use fixed timestamps) |

## 7. Tests added later

| Test | From | Pass when | Status |
| --- | --- | --- | --- |
| UPD-Test-1 | D21 | After a power cut while writing an update, applying the same file again keeps the finished parts, writes the rest, and the new version starts | ✅ beta 5 in QEMU: a power cut during the update, then the same file finishes it and the new version starts |
| UPD-Test-2 | D21 | "Reinstall this version" writes a fresh copy of the running version, it starts afterwards, and an older version is still refused | 🟢 beta 3: "Reinstall this version" in the service tests and the GUI test (a fresh copy, three tries, older versions refused) |
| UPD-Test-3 | D31 | An update file with a part of a kind the installed updater doesn't know installs; the part is checked and kept in `@slots/staged/<version>/parts/<kind>/` for the new version; damaged files never crash the reader | ✅ service tests (unit test and `asimaos-updaterd` end to end, 400 damaged files); first shipped in Beta 7 |
| APP-Test-1 | D28 | Snap and Nix start when used, `/snap` and `/nix` (own subvolume) exist, Waydroid's binder works, 13 programs keep root rights | ✅ beta 5 (disk image test); installing a snap, a Nix program and Android online not tested yet |
| APP-Test-2 | D28 | Flathub is set up out of the box; an app installs and starts | ✅ beta 5 (`--flathub`) |
| BRK-Test-1 | D29 | `/boot` stays closed for the desktop; updates, "mark as good" and Asima Maintenance still reach the ESP | ✅ beta 5 (disk image test) |
| BRK-Test-2 | D29 | The disk guard refuses `rm -r` of system folders and `wipefs` of the Asima OS disk without a person at the keyboard | ✅ beta 5 (disk image test) |
| BRK-Test-3 | D29 | After the ESP is lost and an account deleted, `asimaos-repair` on the USB stick makes the PC start again with the account back; with `--install-this-version` it also installs the stick's version, files kept | ✅ beta 5 (`--repair`); installing the stick's version: test written, runs with the next build |
| KBD-Test-1 | D29 | A new system keyboard layout is stored for the start-up passphrase and used at the next start | ✅ beta 5 (`--trust`, German layout) |
| VM-Test-1 | D22 | QEMU/KVM: install from the ISO, start, shared clipboard, resizing with the window, shared folder (SPICE WebDAV), shutdown from the host | 🟡 beta 3: install and start pass; the guest agent answers and the other VM types' tools stay off; clipboard, resizing and shared folders by hand |
| VM-Test-2 | D22 | VirtualBox (EFI): the same checks with VirtualBox's guest tools | ⏳ |
| VM-Test-3 | D22 | VMware (UEFI): the same checks with open-vm-tools | ⏳ |
| VM-Test-4 | D22 | Hyper-V (Generation 2): install, start, key-value exchange and shutdown from the host | ⏳ |
| TPM-Test-2 | Beta 7 | On a PC whose TPM driver comes up late, TPM unlock still opens the disk: the start-up image waits for the chip (`tpm2.target`) and "enter-initrd" is measured first | ❌ on the owner's PC with 2026.11.21.1 (22 November): the chip is found, but loading the sealed key fails, and systemd doesn't count the start as measured, so a signed PCR 11 policy can't pass there. Fixed by D43 (pcrlock, PCR 4 + 7); to test again with the next build |
| TPM-Test-3 | D49 | TPM unlock with a PIN (the optional way): set up in Asima Maintenance → Security; the start asks for the PIN and opens the disk with it alone; after a wrong PIN the passphrase still opens it; the PIN can be changed, and switched back to automatic unlock | ⏳ the VM test (`--trust`) checks setting it up, the PIN alone and a wrong PIN; your PC to come |
| LOOK-Test-1 | D48, D51 | The start-up screen is Asima OS's own (the mark and the name; also with the tests' serial console), its passphrase prompt takes the passphrase or the recovery key in the installation's layout, Esc shows the messages; the desktop starts with KDE's standard look and wallpaper | ⏳ |
| BIO-Test-1 | D37 | Admin tasks: the Asima OS password dialog appears instead of KDE's; the password works; the fingerprint and face buttons appear only when set up and approve only when tapped; a token is used once; sudo still asks for the password | 🟡 tokens, accounts and refusals tested in `faced-e2e`; the dialog on real hardware ⏳ |
| BIO-Test-2 | D37, D47 | A normal webcam: off until switched on; a photo (no blinks) never unlocks; never at the login screen; paused after three failed tries; the lock screen's Face button there only while a face is set up; guided set-up with turns, nods and blinks | 🟡 photo, login screen, pause and the Face button tested with a still picture in `faced-e2e`; blinks and the guide in unit tests; a real webcam and the blink thresholds ⏳ |
| ESP-Test-1 | Beta 7 | Asima Updater's and Asima Maintenance's services end after five quiet minutes, and the boot partition is no longer mounted anywhere; the next call starts them again | 🟡 idle exit tested in `updaterd-e2e`; on an installed system ⏳ |
| SAFE-Test-1 | Beta 7 | A version whose desktop crashes three times (or three sessions end within two minutes) opens Asima Updater with the offer to go back | ⏳ |
| ETC-Test-1 | Beta 7 | After an update that changes the default of a file the person changed, Asima Maintenance marks it "New default" | 🟡 unit test; after a real update ⏳ |
| SIGN-Test-1 | Beta 7 | `SHA256SUMS.sig` verifies with OpenSSL and aosupd; a file signed with `release-next` is accepted where both keys are trusted | 🟡 unit test; the build's own check |
| BOOT-Test-1 | Beta 7 | An update with a changed boot loader: the next start uses the trial copy (BootNext), and the one after uses it as the default; a broken one is dropped after two starts that don't reach Asima OS, with the old loader still starting | 🟡 unit test (trial, promotion, give-up, unchanged loader); OVMF in a VM ⏳ |
| ROLL-Test-1 | Beta 7 | An update with a migration step that fails to start: after the fallback, the next start has `/etc` and `/var` from before the update, the failed starts' journal kept, `/home` untouched | 🟡 the mark tested in `updaterd-e2e`; early-mount in a VM ⏳ |
| UPD-Test-Skip | Beta 12 | Updates that skip versions (D52): only the migration steps newer than the settings' version run, each once, in version order; a fresh installation runs none; settings older than the update's window run every step it has, and System changes suggests Reset settings; "Repair this computer" stages the stick's steps | ⏳ in `updaterd-e2e` (not run yet); a real skip in a VM |
| GPU-Test-1 | Beta 7 | With two graphics cards: keeping the second one installs the add-on, the next start binds it to vfio-pci and a VM made with it starts and shows on its monitor; giving it back undoes everything | ⏳ real hardware only |
| VM-Test-5 | Beta 7 | Asima Dev makes a Windows 11 virtual machine (UEFI, Secure Boot, TPM) from an ISO that installs, and a Linux one | ⏳ |
| HUB-Test-1 | D39 | Asima Hub uses a feed only when the feed key signed it: a feed changed after signing, unsigned, signed with another key or in the old format is refused; update files without https are not offered; the published feed verifies with the key built into the Hub | ✅ `hub-feed-e2e` (10 checks) and the real feed with the built-in key |
| HUB-Test-2 | D39 | Get Asima OS: "What you'll get on this PC" on a gaming PC, an old PC, a sandbox, a container and a PC that isn't 64-bit x86 (nobody refused; only the processor, a legacy BIOS start and a small disk block); the ISO written to a stand-in for the stick, read back and compared; a changed ISO or no download is refused; the test mode writes only to its test file | 🟡 `hub-get-e2e` (15 checks); a real USB stick through UDisks ⏳ |
| HUB-Test-3 | D39 | Asima Hub's Flatpak bundle: builds against the KDE runtime 6.11 from Flathub, installs for the person, starts and reads the live signed feed with its built-in key; inside the sandbox the PC check says what it can't read (start mode, Secure Boot) and reads the rest; UDisks answers and other system services stay hidden; the cards keep their surfaces without a GPU (Qt's software renderer) | ✅ on the build VM (Fedora, 2026-11-21); Discover installing the bundle and a real USB stick ⏳ |
| PAD-Test-1 | Beta 10 | Gamepad as mouse (Asima Gaming → Controllers): the right stick moves the pointer, the triggers click, the left stick scrolls, A, B and the D-pad press Enter, Esc and the arrows; it pauses while Steam or a game runs | 🟡 unit tests (which devices are game controllers, the stick curve); didn't work on the owner's PC with 2026.11.21.1 (the switch did nothing), written for the next build; a real controller ⏳ |

## 8. Automated suites

| Suite | Runs | Result (build 2026.11.21.1, 2026-11-21) |
| --- | --- | --- |
| `testbed/boot-test.py` | Disk image in QEMU: checks (Snap, Nix, Waydroid's binder, the disk guard, `/boot` closed, the `asimaos` command, the start marked good), restart, persistence, Asima Rescue started once | 43/43 |
| `testbed/boot-test.py --trust` | Firmware with only the 2023 CA, the Asima OS key as a MOK, swtpm: lockdown, the signed NVIDIA driver, the start-up keyboard layout, TPM unlock with the pcrlock policy (D43), automatically and with a PIN (D49); Rescue can't unseal | 12/12 (2026.11.21.1); 13/15 with the pcrlock policy (2026.11.24.1: setting TPM unlock up again failed, fixed since) |
| `testbed/boot-test.py --secureboot` | Secure Boot enforced (Microsoft's keys + the build's certificate in db) | 18/18 |
| `testbed/boot-test.py --repair` | ESP formatted, an account deleted; `asimaos-repair` from the USB stick | 4/4 |
| `testbed/boot-test.py --reset` | The factory reset at the start | 4/4 |
| `testbed/boot-test.py --rescue` | Asima Rescue: unlock, reinstall from a USB drive | 3/3 |
| `testbed/boot-test.py --flathub` | An app from Flathub (internet) | 3/3 |
| `testbed/boot-test.py --iso --update V2 V3` | Encrypted install → power cut during the update → crash and freeze during trial starts → going back and forward → the broken V3 falls back by itself; failed starts and the crash log listed | not run on this build yet; 79/79 on Beta 5 |
| `testbed/boot-test.py --iso --plain` | Live session → install without encryption (D9) → installed system → restart | 57/57 |
| `apps/build.sh services-test` | 29 unit tests; `asimaos-updaterd`, `asimaos-maintenanced`, `asimaos-managerd`, `asimaos-gamingd`, `asimaos-faced` end to end; each app against its service; Asima Hub's feed and Get Asima OS | all passed (139 checks, 2026-11-21) |

## 9. On real hardware

The project owner's PC: a Lenovo IdeaPad Gaming 3 laptop with an AMD Ryzen 5 6600H, an NVIDIA
GeForce RTX 3050 (4 GB), 16 GB of memory and a 512 GB SSD.

| Build | Worked | Didn't |
| --- | --- | --- |
| Beta 5 (2026.11.18.1) | Secure Boot through shim, with the Asima OS key approved in the blue MokManager screen; the NVIDIA driver under kernel lockdown; Android apps (Waydroid); updates, going back and the automatic fallback; Repair from the USB stick; a Nix program; a snap | TPM unlock |
| Beta 7 (2026.11.21.1) | Secure Boot with the build's new key | TPM unlock ([TPM-Test-2](#7-tests-added-later): the start isn't counted as measured there; D43 replaces the policy in the next build) |
