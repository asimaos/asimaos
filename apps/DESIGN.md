# Asima OS apps

What each Asima OS app does, how the apps are built, how they look and behave, and the contracts
with their services. All code here is original Asima OS code. Other image-based distributions were
studied for lessons only (see [Lessons](#lessons-from-the-reference-distributions)); nothing is
copied or forked.

## The apps

**One job per app (D27).** Each app has its own system service, so it can be developed on its own
and, once it works, left alone: a change to one app or its service never touches another.

| App | Where | Job |
| --- | --- | --- |
| [Asima Updater](#asima-updater) (`asimaos-updater`, `asimaos-updaterd`) | `/usr` | Update files and the versions in the start menu: install, reinstall, go back. Never looks for updates online (D2) |
| [Asima Maintenance](#asima-maintenance) (`asimaos-maintenance`, `asimaos-maintenanced`) | `/usr` | Health, Secure Boot and the disk's keys, snapshots and backups, changed system files, Flatpak clean-up, the problem report, factory reset |
| [Asima Manager](#asima-manager) (`asimaos-manager`, `asimaos-managerd`, `asimaos-faced`) | `/usr` | This computer's settings: power and battery, display and keyboard light, fingerprint and face, hardware, Asima Shield (the firewall) |
| [Asima Gaming](#asima-gaming) (`asimaos-gaming`, `asimaos-gamingd`) | `/usr` | Power mode and CPU scheduler for games, overlay, controllers, a handheld's own controls, RGB lighting (D34) |
| [Asima Dev](#asima-dev) (`asimaos-dev`) | `/usr` | Containers, virtual machines with a graphics card of their own, Nix programs, Android apps (D34); runs as you, so it has no service |
| [Asima Hub](#asima-hub) (`io.github.asimaos.Hub`) | Its own repository; Windows and Linux; **never preinstalled** (D3) | Everything online, on any PC: news, a PC check, the ISO and the USB stick; on Asima OS also updates and app groups (D39) |
| [Asima OS Installer](#asima-os-installer) (`asimaos-installer`) | The ISO only | Whole-disk installation |
| [Asima Rescue](#asima-rescue) (`EFI/Asima/asimaos-rescue.efi`) | The ESP | The repair system, entirely in RAM |
| [The password dialog](#the-asima-os-password-dialog-asimaos-polkit-agent-d37) (`asimaos-polkit-agent`) | `/usr` | Approving admin tasks, with fingerprint and face buttons (D37) |

## Stack

| Layer | Choice | Why |
| --- | --- | --- |
| UI | **Qt 6 + QML** (Qt Quick, Controls "Basic" as the neutral base) | Plasma is built on Qt 6, so the apps add almost nothing to the image and integrate with the desktop (file dialogs, icons, colour scheme). |
| Design system | **Asima UI** (`ui/`, QML module `Asima.Ui`) | One look for every Asima OS app: modern, calm, readable; independent of the Plasma theme, but follows its light/dark setting. |
| App logic | C++ (`<app>/src/`), shared plumbing in `common/` (service client, power modes, device readings and the sample devices) and `sysinfo/` (hardware facts) | Small glue: exposes data and actions to QML. |
| Privileged work | **One service per app** (Rust, zbus, polkit; `services/`): `asimaos-updaterd`, `asimaos-maintenanced`, `asimaos-managerd`, `asimaos-gamingd`, plus `asimaos-faced` for face sign-in. They share `asimaos-svc` (polkit checks, system paths, snapshots, ending after five quiet minutes) and `asimaos-update` (the update format and the start menu) | Built and tested on a private D-Bus bus. GUIs never run as root; every privileged action is a D-Bus call authorised by polkit for the caller. Status comes back as one JSON string, so the GUIs read it straight into QML. One service per app (D27): a change to one never touches another. |
| Build | `apps/build.sh` (rootless container, pinned snapshot) | Same toolchain as the OS image; no sudo. |

The desktop keeps KDE's standard look (D48). Asima UI is the apps' own design system: Asima blue
(D17), following Plasma's light/dark setting.

## Design principles

1. **Plain words first.** "Go back to this version", not "Set default boot entry". Technical detail goes in subtitles and tooltips.
2. **One owner per setting.** Every setting has exactly one owner: a daemon or Plasma. An Asima OS app changes it only through that owner's own interface (the power mode through power-profiles-daemon, just like Plasma's battery applet; the keyboard light through UPower), never by keeping a copy. Where Plasma has the full settings page (display layout, network, sleep timers), the app links there.
3. **One job per app (D27).** Each app does one thing (updates, maintenance, settings, gaming, development) with its own service, so it can be finished and left alone.
4. **Calm by default.** A healthy system looks healthy: green status, little text. Colour is reserved for state (ok, warning, error) and for the one primary action on a screen.
5. **Every action gives feedback.** Progress for anything slower than ~100 ms, a toast when done, a clear error with a next step when not.
6. **Destructive means deliberate.** Destructive actions use the danger colour, explain exactly what is lost, and need a typed confirmation (factory reset: type `RESET`).
7. **No password just to look.** Reading status never asks for a password. Authorisation is requested only when the user starts a privileged action.
8. **Honest about limits.** If something can't be known or done (e.g. the service isn't installed), say so instead of showing fake data. Demo data is labelled "Demo data".
9. **Keyboard and screen readers.** Visible focus ring on every control, `Accessible` names, contrast ≥ 4.5:1 for text (the Asima Blue accent `#0A6CD6` is 5.1:1 with white).

## Asima UI

### Tokens (`ui/Theme.qml`)

| Group | Values |
| --- | --- |
| Brand "Asima Blue" (D17) | accent `#0A6CD6` (hover `#085DBA`, pressed `#064E9C`); fill gradient sky `#38BDF8` → `#2F80ED` → `#1E3FAE`; hero gradient `#0A8FD9 → #2563EB → #1E3A8A` (deeper in dark mode) |
| Surfaces (light / dark) | window `#F3F4F7` / `#0E1015` · surface `#FFFFFF` / `#171B22` · border `#E4E7EC` / `#262C37` |
| Text (light / dark) | `#12151B` / `#EDF0F5` · muted `#5B6371` / `#A5ADBB` |
| Status | ok (green) · warn (yellow) · error (red) · info (indigo, so it never looks like the blue accent), each with a soft background tint; separate palettes for light and dark. Nothing orange. |
| Type | Noto Sans (Asima OS system font): 32 display · 26 title · 20 subtitle · 16 heading · 14 body · 13 small · 12 caption/overline |
| Spacing | 4 px grid (4, 8, 12, 16, 20, 24, 32) |
| Shape | radius 8 / 10 (controls) / 16 (cards) / 22 (heroes, dialogs) |
| Motion | 120 ms (hover), 200 ms (state), 360 ms (enter); ease-out cubic |

Light/dark follows Plasma's colour scheme automatically (`Theme.mode = "system"`).

**App icons** (`ui/assets/make-app-icons.py`): every Asima OS app has a white symbol on the Asima Blue tile of the Asima OS mark: Updater (an update arriving), Maintenance (a wrench), Manager (settings sliders), Gaming (a game controller), Dev (code brackets). The installer and update files keep the mark itself, which is also the OS logo (`LOGO=asimaos`).

### Components

| Component | Use |
| --- | --- |
| `Sidebar`, `NavItem` | App navigation with the app's icon and name, optional sections ("Danger zone") and a footer slot |
| `DPage` | Scrollable page: title, subtitle, header actions, centred content (max 1140 px) |
| `Card`, `StatTile` | Grouped content; metric tiles. Cards can be clickable |
| `ListRow` | Row with icon, title, subtitle, value or status pill, trailing actions |
| `StatusPill`, `StatusGlyph`, `StatusRing` | State at a glance: pill, vector ✓ ! ✕ i glyphs, circular progress |
| `DButton` | primary · secondary · soft · ghost · danger · onAccent; optional icon and busy spinner |
| `InfoBanner` | Inline info / warning / error / success message with optional action |
| `StepList`, `DProgressBar`, `Spinner` | Multi-step progress (updates, installs) |
| `DropZone` | Dashed drop target for files (update files) |
| `WizardRail`, `WizardPage` | Guided full-window flows (installer, Rescue) |
| `SelectCard`, `OptionList`, `ChoiceChips`, `DComboBox` | Choices: large cards, searchable lists, compact chips, drop-downs |
| `SegmentedTabs`, `DSwitch`, `DSlider`, `DCheckBox`, `DTextField`, `PasswordField` | Inputs |
| `QuickToggle` | Quick-settings tile: icon, title, current value; filled in the accent colour when on |
| `DiffView` | Unified diff with added/removed/hunk lines coloured |
| `ConfirmDialog`, `Toast` | Modal confirmation (optional typed word) and short feedback |
| `Icon`, `IconTile`, `AsimaMark` | Breeze icons tinted to the palette (`image://dicon/<name>/<rrggbb>`); the Asima OS mark, or an app's icon (`icon: "updater"`) |

Icons: only monochrome Breeze icons (actions/places/devices) are used, so tinting never loses detail. Status symbols are drawn as vectors (`StatusGlyph`).

Plurals: source strings mark regular English plurals as `%n file(s)`. Asima UI installs a fallback translator that renders them as "1 file" / "2 files"; real translations loaded later take precedence.

## Asima Updater

Update files and the versions in the start menu, nothing else, so it can stay unchanged while the
other apps grow.

| Page | Shows | Actions (all through `asimaos-updaterd`) |
| --- | --- | --- |
| Updates | Installed version; drop zone; checks of a chosen file (signature, newer, not expired, free space); release notes; step-by-step progress; restart | Verify file, install, reinstall this version, restart |
| Versions | The start menu's current, backup and failed versions with their boot status | Make current (go back or forward), start once, give a failed update another try |

No page checks for updates online (decision D2): updates come from files or from Asima Hub, the separate online Flatpak. The sidebar footer names the version that starts next, or the backup.

### Service contract: `io.github.asimaos.Updater1` (system bus, `asimaos-updaterd`)

| Member | Kind | Polkit action |
| --- | --- | --- |
| `Status() → s` (JSON: version, versions with boot counters, free space, Rescue, and the newest job: id, progress, detail, done, ok, message, which the `asimaos` command follows) | method | none (read-only) |
| `VerifyUpdate(h fd) → s` (JSON report, see below) | method | none (verification only, file passed as fd: the service never opens files in your home) |
| `ApplyUpdate(h fd) → s job` (a snapshot first; the new version becomes "current" in the start menu, the running one its backup) | method | `io.github.asimaos.updater.update` |
| `ReinstallVersion(h fd) → s job` (a fresh copy of the running version from its own file, D21) | method | `io.github.asimaos.updater.update` |
| `SetDefaultVersion(s version, b once)` | method | `io.github.asimaos.updater.rollback`: once = `bootctl set-oneshot` on its start menu entry; otherwise it becomes "current" (a failed update gets three new tries) |
| `JobProgress(s job, d progress, s step, s detail)` / `JobFinished(s job, b ok, s message)` | signals | — |

`asimaos-updaterd --finish-update` (after a start marked good, `asimaos-finish-update.service`) and `--run-migrations` (`asimaos-migrate.service`, at every start: the steps newer than the settings' version, D52) finish an update: the boot loader, the shim (with its MokManager, only on PCs that start through shim) and Rescue from the update are installed then. Each version's boot image is also kept in `@slots/boot` for "Repair this computer". The service opens the ESP (never mounted for the desktop) at `/boot` in its own mount namespace.

**Update files (`.aosupd`, D18)** carry the whole OS in one file: header, manifest, an Ed25519 signature over both, then the parts (zstd-compressed where that helps). The format is specified in [`services/asimaos-update/src/lib.rs`](../services/asimaos-update/src/lib.rs); `aosupd` makes (`keygen`, `pack`), checks (`inspect`, `verify`) and installs them (`install`, used by Asima Rescue). The release key is `/usr/share/asimaos/keys/release.pub`. A part of a kind the installed updater doesn't know (`aosupd pack --part KIND=FILE`) is checked like the others and kept in `@slots/staged/<v>/parts/<kind>/` for the new version's own code (since Beta 7, D31), so new kinds of parts never need a stepping-stone update. The MIME type `application/x-asimaos-update` opens them in Asima Updater (`asimaos-updater FILE` goes straight to the check).

`VerifyUpdate` checks, in this order: the file is a signed Asima OS update (`refused`: `not-signed`, `newer-format` or `damaged` when it isn't, `damaged` meaning cut off or changed on its way); then the rules (`failed`: `arch`, `newer`, `min-from`, `expired`, `space`); then, only when all of those pass, every part's checksum (`partsChecked`; a mismatch is `refused: damaged`). The report: `ok`, `signature`, `partsChecked`, `refused`, `failed` and `problems` (one sentence each, same order), `version`, `current`, `expires`, `unpackedSize`, `freeBytes`, `needBytes`, `notes` (release notes text), `reinstall` and `sameVersion` (the running version's own file, offered as "Reinstall this version", D21). Asima Updater shows one row per check, the error title from `refused`, and every problem.

`ApplyUpdate` verifies again, takes the snapshot set of the files and settings (`@snapshots/update/<v>`, the last two kept), then decompresses each part to `.<name>.partial`, hashing as it writes, and renames it into place only when the content checksum matches: the slot images (made writable only inside the service's private mount namespace), then the staged boot loader, Rescue, migration steps and parts of new kinds (`@slots/staged/<v>`), and the start menu last: the UKI goes to `EFI/Asima/asimaos_<v>.efi` and the new version becomes "Asima OS (current)" with three tries (`loader/entries/asimaos-current+3-0.conf`), the running one its backup (`services/asimaos-update/src/boot.rs`). It keeps those two versions and removes older ones. A damaged file leaves nothing behind. After the new version has started fine, `asimaos-finish-update.service` installs the staged boot loader and Rescue; `asimaos-migrate.service` runs the migration steps once at the first start.

Without the service (or outside Asima OS), Asima Updater shows read-only facts or the demo and says that changes need the service.

## Asima Maintenance

Looking after the installed system.

| Page | Shows | Actions (all through `asimaos-maintenanced`) |
| --- | --- | --- |
| Overview | Status hero, storage, security, last backup, snapshots, Flatpak apps, recent activity; a warning when another system (Windows) has put itself first in the firmware's start order; Asima Hub | Run health check, clean up Flatpak, open Asima Updater, get Asima Hub (its always-latest download in the browser, D54) or open it, start Asima OS first (moves the entry this start came from, `BootCurrent`, to the front of `BootOrder`; left alone while a new boot loader is on trial) |
| Health | Score ring, checks, automatic maintenance schedule | Run full check |
| Security | Secure Boot with the Asima OS key, own Secure Boot keys in the firmware (prepare, Setup Mode, write; the saved certificates to keep or leave out), other approved keys (earlier builds', another system's), disk encryption, automatic unlock (TPM), recovery key, system protection (dm-verity, lockdown, security modules, the firmware's revocation list through Discover's firmware updates) | Set up Secure Boot (one-time code for the blue MOK screen, D16), retire chosen old keys (removed and blocked in MOKX, one code for both questions on the blue screen), restart to firmware settings, set up automatic unlock (D10), or unlock with a PIN instead (D49: the optional way; change the PIN, switch between the two, turn off), change the disk passphrase, make a new recovery key (shown once; the old one stops working, because keys are never stored) |
| Snapshots & Backup | Daily and pre-update snapshots; backup destination, schedule, last run, restore test; on an encrypted disk, when the encryption header was last copied (each backup keeps a LUKS2 header copy, tag `asimaos-disk-header`, the newest 4) | Open snapshots in Dolphin, snapshot now, back up now, set up / change the destination |
| System changes | **Reset settings** for everyone: the system's settings and the person's desktop and preinstalled apps' settings back to this version's defaults; then every `/etc` file that differs from factory (changed/added/removed) with a diff | Reset settings: confirmed, then the computer restarts; the initramfs restores `/etc` (kept: the accounts, the computer's name, language, keyboard and time zone, networks, SSH host keys, added certificates, backups, VMs and the card kept for them, the snaps' mounts) and `asimaos-reset-settings.service` moves the person's KDE and app settings and the add-ons installed since (themes, colour schemes, icons and cursors, wallpapers, widgets, KWin scripts, service menus), so the desktop is plain KDE again (D48) (`~/.config/*rc` and the like, but the language, keyboard layouts, input methods, Plasma Vaults and KDE Wallet settings) to `~/.local/share/asimaos/old-settings/<date>/` before the login screen. A copy of `/etc`'s changes goes to `@data/old-settings/`; the newest three of each are kept. Flatpak apps (`~/.var/app`) and other installed apps are never touched. One file: restored at the next start, never under a running system |
| Reset | One reset (D13): back to a fresh installation of the current version, as if just installed; exactly what is deleted (everything) and kept (the current OS) | The fresh system's account and disk passphrase first (no first-start setup, D10), authorised with the current passphrase; then the new recovery key (shown once); then typed `RESET`. Runs in the initramfs at the next start (D12, fast) |

### Service contract: `io.github.asimaos.Maintenance1` (system bus, `asimaos-maintenanced`)

| Member | Kind | Polkit action |
| --- | --- | --- |
| `Status() → s` (JSON: version, storage, security, snapshots, backup with the restore state, the snapshot schedule, failed starts and kernel crashes, Rescue, reset, scheduled `/etc` restores, a scheduled settings reset, the start order: whether Asima OS is first) | method | none (read-only) |
| `RunHealthCheck() → s` (JSON checks: system-files, boot, space, filesystem, services, secure-boot, reserve, and scrub / ssd once those timers have run) | method | none |
| `SnapshotNow() → s name` | method | `io.github.asimaos.maintenance.backup` |
| `ConfigureBackup(s json {repository, password})` / `BackupNow()` | method | `io.github.asimaos.maintenance.backup` (auth_admin_keep): restic; the password goes to a root-only file |
| `SetSnapshotSchedule(s daily\|weekly\|monthly\|off)` | method | `io.github.asimaos.maintenance.backup`: how often snapshots of `/home` and of the settings and accounts are taken (`/etc/asimaos/snapshots.conf`; the newest 14 are kept) |
| `ListBackups() → s` / `RestoreBackup(s id, s user)` | method | `io.github.asimaos.maintenance.backup`: one person's folder from a backup into a new `Restored YYYY-MM-DD` folder in their home (`asimaos-restore.service`, which may write to `/home`); nothing is overwritten |
| `ProblemReport() → s` | method | `io.github.asimaos.maintenance.report` (auth_admin_keep): plain text (versions, health check, failed starts, crash logs, failed services, recent errors, hardware) for the person to read, edit, save or copy; never secrets |
| `ApproveAsimaKey() → s code` / `CancelKeyApproval()` | method | `io.github.asimaos.maintenance.secureboot` (auth_admin_keep): `mokutil --import` of the Asima OS key with a one-time numeric code |
| `PrepareOwnKeys()` / `EnrollOwnKeys(as drop)` | method | `io.github.asimaos.maintenance.secureboot` (auth_admin_keep): save db, KEK, dbx and make this computer's Platform Key and KEK; in Setup Mode write dbx, db, KEK and PK (leaving out the certificates whose SHA-256 is in `drop`); `Status().security.ownKeys` = { setupMode, prepared, own, certificates } |
| `RetireKeys(as fingerprints) → s code` / `CancelRetireKeys()` | method | `io.github.asimaos.maintenance.secureboot` (auth_admin_keep): the chosen approved keys (SHA-256 fingerprints from `Status().security.otherKeys`; never this version's own) to `mokutil --delete` and `mokutil --mokx --import`, one one-time code |
| `EnrollTpm(s passphrase)` / `EnrollTpmPin(s passphrase, s pin)` / `RemoveTpm()` | method | `io.github.asimaos.maintenance.secrets` (auth_admin): with Secure Boot on: `asimaos-pcrlock refresh --init`, then `systemd-cryptenroll --tpm2-device=auto --tpm2-public-key= --tpm2-pcrlock=` (the systemd-pcrlock policy, PCR 4 + 7, D43), tried at once; set up again, the old TPM slot is wiped only after the next start (`asimaos-maintenanced --tpm-rotation`); when an identical working slot exists, nothing changes. EnrollTpmPin (D49) adds `--tpm2-with-pin=yes --tpm2-argon2id-memory=512M` (PIN of at least 4 characters, in `NEWPIN`), tries the slot with the PIN on stdin and replaces the old TPM slots at once. `Status` reports `tpmPin`. RemoveTpm also removes the policy |
| `ChangePassphrase(s current, s new)` | method | `io.github.asimaos.maintenance.secrets` (auth_admin) |
| `NewRecoveryKey(s current) → s` | method | `io.github.asimaos.maintenance.secrets` (auth_admin, no keep): enrolls a new key, removes the old one, returns it once (keys are never stored, so there is no "reveal") |
| `RestoreEtc(s path)` (applied at the next start, never while /etc is mounted; account files are protected) | method | `io.github.asimaos.maintenance.etc` (auth_admin) |
| `StartAsimaFirst()` (the entry this start came from goes first in `BootOrder`) | method | `io.github.asimaos.maintenance.boot` (auth_admin_keep) |
| `ResetSettings() → u` ("Reset settings" for the caller, a person's account: schedules it, restarts; returns how many `/etc` entries are reset) | method | `io.github.asimaos.maintenance.etc` (auth_admin) |
| `PrepareFactoryReset(s json fresh) → s` / `CancelFactoryReset()` / `ScheduleFactoryReset()` | method | `io.github.asimaos.maintenance.reset` (auth_admin, no keep): stages the fresh account and new disk secrets, then the initramfs wipes and rebuilds |

`asimaos-maintenanced --snapshot-daily` (`asimaos-snapshot.timer`, when the schedule says a snapshot is due), `--backup-run` (`asimaos-backup.service`, the only part allowed to read `/home`) and `--restore-run` (`asimaos-restore.service`) do the scheduled work. Failed starts come from `asimaos-health` (`/var/lib/asimaos/failed-starts`), kernel crashes from systemd-pstore (`/var/lib/systemd/pstore`, `efi_pstore` is on).

## Asima Manager

The settings of this computer. The page list and the home page adapt to the device (`SysInfo::deviceType()`: DMI chassis type plus known handheld names).

| Page | Shows | Acts through |
| --- | --- | --- |
| Home | Device hero (drawn device, battery ring, chips), 4–6 quick settings, protection, live sensors | the pages below |
| Power & Battery | Charge, time left, draw, health, cycles; battery care (a charge limit, or on and off where the firmware sets the level, as on Lenovo laptops); power mode; wake sources | sysfs; `asimaos-managerd` (the battery's `charge_control_end_threshold`, Lenovo's `charge_types` "Long Life" or the IdeaPad conservation mode; kept in `/etc/asimaos/battery.conf` and set again at every start by `asimaos-battery.service`); power-profiles-daemon |
| Display | Links to Plasma's screen pages; the keyboard light where there is one; a pointer to Asima Gaming on devices with RGB lights (D34) | UPower (keyboard light) |
| Sign-in & Biometrics | Fingerprint enrolment (ring + stages; a reader plugged in later shows up), face enrolment (scan view), switching a normal webcam on after a warning (D37), guided face set-up (straight, turn both ways, nod both ways, three blinks: D47), where they are used, how admin tasks use them (the buttons in the password dialog) | fprintd; `asimaos-faced`; `asimaos-managerd` (where face sign-in may be used) |
| Hardware | Processor, graphics, memory, storage, screen, wireless, model, firmware, system; temperatures; fans; "Copy system report" | sysfs, procfs, DMI |
| Shield | The firewall's zone with what each allows: **Public** (nothing can connect in), **Home** (the default: + KDE Connect, finding printers and phones, game streaming), **Developer** (+ local dev servers on ports 3000–9000 and everything from VM and container bridges); VMs, containers and Android apps get their addresses and names over their own bridges in every zone; a VPN kill-switch (nothing leaves except through WireGuard, Tailscale or another tunnel) | `asimaos-managerd` (nftables) |

Security choices (D37, D47): fingerprint and face approve admin tasks only through their buttons in the Asima OS password dialog; a webcam face unlocks the screen and approves admin tasks after two blinks, and never signs in at the login screen. After a restart the password is needed once, so KWallet unlocks too. Biometric templates stay on the device; a factory reset deletes them.

Home also offers **Asima Hub** (D39), which is never part of the system (D3): "Get Asima Hub" opens
the Hub's always-latest download, `https://github.com/asimaos/asimaos-hub/releases/latest/download/AsimaHub-x86_64.flatpak`,
in the browser, and opening the file installs it with Discover; once the Hub is installed, the card
opens it instead. Asima Manager itself never goes online (D2).

Demo mode (`--demo`, or automatically outside Asima OS) shows a gaming handheld; `--state laptop` shows a laptop with an infrared camera. Screenshot states: `enrolling` (fingerprint), `face` (face scan), combinable as `laptop-face`.

### Service contract: `io.github.asimaos.Manager1` (system bus, `asimaos-managerd`)

| Member | Kind | Polkit action |
| --- | --- | --- |
| `Status() → s` (JSON: shield, charge limit and whether its level is fixed, where face sign-in is used) | method | none (read-only) |
| `Set(s area, s key, v value)`: `power/chargeLimit` (50–100 %; where the level is fixed, under 100 switches battery care on), `biometrics/useLogin`, `biometrics/useUnlock` (`/etc/asimaos/face.conf`) | method | `io.github.asimaos.manager.device` (allow_active: yes) |
| `SetShield(s key, v value)`: `zone` (public, home, developer), `killSwitch` | method | `io.github.asimaos.manager.shield` (auth_admin_keep); the nftables ruleset is replaced in one transaction, saved in `/etc/asimaos/shield.conf` and applied at every start by `asimaos-shield.service` (`asimaos-managerd --apply-shield`) before the network |
| `Changed(s area)` | signal | — |

Fan and power modes stay with power-profiles-daemon (one owner per setting). RGB lighting is `asimaos-gamingd`'s and Android apps are Asima Dev's (D34).

### Service contract: `io.github.asimaos.Face1` (system bus, `asimaos-faced`)

The service owns the camera (frames through `v4l2-ctl`) and the faceprints (`/var/lib/asimaos-face/<uid>.json`, root only); apps never see a camera frame. It finds faces with **YuNet** and compares them with **SFace** (open models from OpenCV Zoo, MIT and Apache-2.0, fetched at build time with pinned SHA-256 into `/usr/share/asimaos/face`), run by tract in pure Rust. A face aligned to SFace's five reference landmarks gives a 128-number faceprint; sign-in needs a cosine similarity of at least 0.45 on two frames in a row (SFace's own threshold is 0.363). An infrared camera is used when there is one. A normal webcam only after the person switched it on in Asima Manager (`WEBCAM=yes` in `/etc/asimaos/face.conf`, D37), and then with stricter rules: 0.55 on two frames, plus two blinks while the same person looks at it (D47), within eight seconds; a photo can't blink. A blink is the eyes looking clearly less open for one to ten frames: the openness is the vertical contrast of a patch around each eye (YuNet gives the eyes' centres, not the lids) against its usual value, a heuristic to tune on real cameras. Webcam faces unlock the screen and approve admin tasks, never the login screen. Setting a face up is guided (`EnrollGuide`): straight (and the usual nose height), a turn of at least 0.22 eye-distances to one side and then the other, a nod of 0.12 one way and the other, three blinks; faceprints from every pose are kept (up to 24). A still test picture gives straight looks only. After three failed tries, faces are paused for five minutes and the password is asked. The service ends after five quiet minutes.

| Member | Kind | Polkit action |
| --- | --- | --- |
| `Camera` → s, `CameraKind` → s (infrared, webcam, ""), `Webcam` → s (a normal webcam's name, used or not) | properties | none |
| `ListFaces() → as` (the calling user's looks) | method | none |
| `Enroll(s label)` / `Cancel()` / `DeleteAll()` | method | `io.github.asimaos.face.enroll` (allow_active: yes, own user only) |
| `VerifyFor(s user, s use) → b` (use: login, unlock; looks for up to 4 s, 6 s with a webcam; error `io.github.asimaos.Face1.NoFace` when it can't be used: nothing set up, a webcam at the login screen, paused after failed tries). `VerifyUser(s user)` is `VerifyFor(user, "unlock")` | method | only root (login manager) or that same user (lock screen) |
| `AdminMethods(s user) → as` ("fingerprint" with a reader and a finger enrolled, "face" with a face and a camera) | method | none |
| `AdminToken(s kind, s user) → s` (kind: fingerprint through fprintd as root, face; a one-time token valid for a minute) | method | only `/usr/lib/asimaos/asimaos-polkit-agent` (checked by the caller's executable), polkit `io.github.asimaos.face.admin` (allow_active: yes) |
| `ConsumeToken(s user, s token) → b` (used up either way) | method | only root (`pam_asimaos_face.so`) |
| `EnrollProgress(d progress, s hint)` (hint: look, turn, closer, farther, hold) / `EnrollDone(b ok, s message)` | signals | — |

**`pam_asimaos_face.so`** (`services/pam/`) asks `VerifyFor`: `auth sufficient pam_asimaos_face.so use=login` (login manager) and `use=unlock` in Plasma's `kde-face` service, the lock screen's Face method next to the password: `asimaos-faced` copies `/usr/share/asimaos/pam/kde-face` to `/etc/pam.d/kde-face` while someone has a face set up and removes it when nobody has, so the button shows only then (D47); the camera looks each time it is chosen. `use=admin` sits first in `/etc/pam.d/polkit-1`: it reads the password answer, hands a token back to `ConsumeToken`, and leaves any other answer to `pam_unix` as the password (D37). A token is good once, for a minute, for its account and **only from the dialog process it was given to**: the module passes the process at the other end of polkit's helper socket (`SO_PEERCRED`; the helper's parent with an older setuid helper) and `ConsumeToken(user, token, agent)` compares it. `AdminToken` answers only the Asima OS dialog (`/usr/lib/asimaos/asimaos-polkit-agent`) and only an untampered one: not traced (`TracerPid: 0`), non-dumpable (it calls `prctl(PR_SET_DUMPABLE, 0)` at its start, so no program of the session can attach to it or read its memory; its `/proc` files then belong to root), and started without code-loading variables (`LD_PRELOAD`, `LD_AUDIT`, `LD_LIBRARY_PATH`, Qt plugin and QML import paths, `GCONV_PATH`). The first sign-in after a start always uses the password (so KDE Wallet unlocks); without a face set up, it steps aside. To verify on real hardware: many infrared cameras need their emitter switched on (`linux-enable-ir-emitter`, used when installed), and the threshold may need tuning on real infrared pictures.

### The Asima OS password dialog (`asimaos-polkit-agent`, D37)

The session's polkit agent, in place of KDE's (a drop-in on `plasma-polkit-agent.service`; when
polkit refuses its registration it starts KDE's agent instead, and when it keeps failing systemd
starts KDE's agent through `asimaos-polkit-agent-fallback.service`). It shows the app's message, the
account that approves (a choice when there are several administrators), the password box, and a
**Fingerprint** and a **Face** button when `AdminMethods` lists them. A button asks `AdminToken`;
the token goes to polkit's helper in place of the password. Wrong answers start a fresh try. Built
with polkit-qt6; screenshot states: `dialog`, `dialog:fingerprint`, `dialog:error`.

## Asima Gaming

| Page | Shows | Acts through |
| --- | --- | --- |
| Gaming | Power mode, CPU scheduler (sched-ext `scx_lavd` / `scx_bpfland`, D44), overlay, GameMode, ntsync, shader cache, GPUs | power-profiles-daemon; Flatpak override for MangoHud; `asimaos-gamingd` for the scheduler (`scx_loader`; `scx_lavd` in gaming mode from the start, the choice kept in `/etc/scx_loader/config.toml`) |
| Controllers | Every gamepad with battery; live test (sticks, triggers, every button) on a drawn controller; gamepad as mouse on the desktop; rumble strength for every controller; supported families | evdev (`/dev/input`, readable through the uaccess rules for game devices); the user service `asimaos-controller-mouse` (`asimaos-gamingd --controller-mouse`: right stick moves the pointer, the triggers click, the left stick scrolls, A is Enter, B is Esc, the D-pad gives the arrow keys, through uinput; it pauses while Steam or a GameMode game runs) |
| Handheld (handhelds only) | Power limit presets and watts; refresh rate of the built-in screen; frame limit and FSR for Steam games; fan at full speed or automatic (where the driver allows it); a link to Plasma's screen rotation (with a rotation sensor); game mode (D46): "Start in game mode" (`gaming/gameMode`, polkit `io.github.asimaos.gaming.gamemode`, auth_admin_keep: `asimaos-gamingd` writes `/etc/plasmalogin.conf.d/90-asimaos-game-mode.conf`, `[Autologin] User=<the caller> Session=asimaos-game-mode Relogin=false`) after a warning that anyone who turns the device on gets into Steam. The session (`/usr/share/wayland-sessions/asimaos-game-mode.desktop`, also on the login screen) runs in gamescope (D55): `gamescope --steam --fullscreen -- asimaos-game-steam gamescope`, which runs Flatpak Steam with `-gamepadui -steamos3` (frame limit and FSR in Steam's quick-access menu; Discover on Steam's page when it isn't installed) and lends it `steamos-session-select` ("Switch to Desktop" ends Steam) and `steamos-update` (nothing to update) from `/usr/lib/asimaos/game-mode`; where gamescope stops within seconds, the session runs on `kwin_wayland --xwayland --no-lockscreen` instead; when Steam ends, the login screen comes back. The built-in controls (where InputPlumber manages them, D55): "appear as" a Steam Deck controller (Steam Input: back buttons, quick-access button, gyro), an Xbox Elite controller or a DualSense Edge (`handheld/controller`) | `asimaos-gamingd` (power limit, fan: hwmon `pwm1_enable` of `asus` or `thinkpad`, 0 full speed, 2 automatic); KScreen (`kscreen-doctor -j`, `output.NAME.mode.ID` at the same resolution); MangoHud for Flatpak Steam (`~/.var/app/com.valvesoftware.Steam/config/MangoHud/MangoHud.conf`: `fps_limit`, `no_display` when the overlay is off; the `MANGOHUD=1` override when either is on); Proton's `WINE_FULLSCREEN_FSR=1` as a Flatpak override; InputPlumber (`org.shadowblip.Input.CompositeDevice.SetTargetDevices`, and a copy of its file for the handheld in `/etc/inputplumber/devices.d` with the chosen `target_devices`) |
| Lighting (only with RGB lights, D34) | One colour and brightness for every light the kernel drives (DualSense light bar, joystick rings, RGB keyboards with a kernel driver), on or off, with a preview; the lights found | `asimaos-gamingd` (kernel multicolour LEDs) |

### Service contract: `io.github.asimaos.Gaming1` (system bus, `asimaos-gamingd`)

| Member | Kind | Polkit action |
| --- | --- | --- |
| `Status() → s` (JSON: the power limit's range and value; `lighting`: the lights with name and device, the colour and brightness) | method | none (read-only) |
| `Set(s area, s key, v value)`: `handheld/tdp` (W; ASUS firmware attributes or the AMD power cap), `handheld/fan` ("auto", "max"), `controllers/rumble` (0–100 %: `RUMBLE=` in `/etc/asimaos/gaming.conf`, FF_GAIN on every controller now and, through `70-asimaos-gaming.rules` running `asimaos-gamingd --rumble-gain`, on each as it connects), `gaming/scheduler` (lavd, bpfland, default via scx_loader), `lighting/color` ("#RRGGBB:percent", 0 % off; every multicolour LED, kept in `/etc/asimaos/lighting.conf`) | method | `io.github.asimaos.gaming.device` (allow_active: yes) |
| `Changed(s area)` | signal | — |

`asimaos-gamingd --apply-lighting [LED]` puts the saved colour back on a light; a udev rule
(`90-asimaos-lighting.rules`) runs it for every multicolour LED that appears, at start-up and when a
controller connects. Without a saved colour the lights keep the firmware's own.

## Asima Dev

Containers, virtual machines, Nix programs and Android apps. Everything here runs as you
(Distrobox on rootless Podman, libvirt's session, your own Nix profile, Waydroid's session), so
Asima Dev has no service. Setting Android up or removing it starts one of two system units through
systemd's `StartUnit` (polkit `org.freedesktop.systemd1.manage-units`, the administrator's password).

| Page | Shows | Acts through |
| --- | --- | --- |
| Containers | Every container with its image and state; Podman and Distrobox status | Create from Ubuntu, Fedora, Arch or Debian; open in Konsole (`distrobox enter`) |
| Virtual machines | Your virtual machines (`virsh -c qemu:///session`), with memory, cores and disk size; a form for a new one: Windows 11 (UEFI, Secure Boot, TPM 2.0 by swtpm, SATA and e1000e, which need no extra drivers) or Linux (UEFI, virtio), from an installer disc | `qemu-img`, `virsh define/start/shutdown/undefine` in your libvirt session, passt for the network; the screen opens in Virtual Machine Manager (Flathub) |
| Nix programs (D28) | Your Nix profile (`nix profile list`) and Nix's state | Install by name from nixpkgs (`nix profile install nixpkgs#NAME`), remove, update all (`nix profile upgrade --all`), free up space (`nix-collect-garbage --delete-older-than 30d`); `nix-command` and flakes are on in `/etc/nix/nix.conf` |
| Graphics card for VMs | Every graphics card: the one showing the screen stays with Asima OS; a second one can be kept for VMs (warning when its IOMMU group holds other devices); "Give it …" when making a VM | `asimaos-passthrough-on@ADDRESS.service` / `asimaos-passthrough-off.service` through systemd (administrator's password): `/etc/asimaos/passthrough.conf`, the signed IOMMU add-on in the ESP's `loader/addons/`, unlimited locked memory for the person's services; `asimaos-passthrough-bind.service` hands the card to vfio-pci at every start; `71-asimaos-vfio.rules` gives the person its VFIO group; VMs get `<hostdev managed='no'>` |
| Android apps (D28, D34) | Set up (downloads Android, about 1 GB), its progress, open Android, stop it, install an app file, remove Android with all its apps; a warning where only NVIDIA graphics draw Android in software | `asimaos-android-setup.service` (`waydroid init`, then its container at every start) and `asimaos-android-remove.service` through systemd; the rest as you (`waydroid` session) |

## Asima OS Installer

Full-window guided flow (`WizardRail` + `WizardPage`), shipped only on the live ISO next to the engine. Rail footnote: "Nothing on your disks changes until you press Install."

| Step | What the user does | Safety / honesty |
| --- | --- | --- |
| Welcome | Reads what Asima OS is; sees an automatic check of this computer (UEFI, Secure Boot, TPM 2.0, memory, a usable disk, power) | Can't continue without UEFI or a usable disk; "Try Asima OS first" closes the installer |
| Language | Picks the language: every one Linux has (glibc's UTF-8 locales, all on the system), searchable, in native names (বাংলা, हिन्दी, 日本語 …) with a note where the desktop isn't translated yet; the keyboard: every XKB layout, with a test field; the time zone: every place in tzdata's list, searched by city or country (in English or its own language), with the local time there | Keyboard and time zone follow the language until the user changes them: its country's layout and main time zone, or a table where that isn't right (India's English keyboard with the rupee, Toronto for Canada, …) |
| Disk | Picks a whole disk from cards (size, type, serial ending, what it contains) | The live USB stick, read-only disks, disks in use and disks under 48 GB are shown but can't be chosen, with the reason |
| Account | Enters their name; the username and computer name follow it (editable); sets a password | Names checked against the system's own accounts and the engine's rules; password at least 4 characters; this account is the administrator (D10) |
| Encryption | Keeps "Encrypt this disk" on (default) or switches it off (D9); unlocks with the account password (default, when ≥ 8 characters) or sets its own passphrase (strength meter, repeat) | Says that at startup the passphrase is typed with the chosen layout (stored in a firmware variable; US only where the firmware doesn't keep it); announces the recovery key; switched off: says plainly that encryption can't be added later. TPM unlock is offered later in Asima Maintenance |
| Summary | Reviews everything; changes any part | "Erase and install" (danger) asks to type the **last four characters of the disk's serial number** |
| Install | Watches real progress and steps; reads rotating tips | Progress comes from the disk's own write counters; on failure: what happened + "Try again" |
| Done | Copies or saves the **recovery key** (encrypted installs), confirms it is saved, restarts | The key is shown once and never stored; "Restart now" stays disabled until confirmed |

There is no first-start setup (D10): the installer creates the account; automatic unlock with the TPM and Secure Boot live in Asima Maintenance → Security.

**Repair this computer** (D55: Welcome → "Repair an installed Asima OS") has its own three steps in the same window; the rail's footnote says "Nobody's files are touched."

| Step | What the user does | Safety / honesty |
| --- | --- | --- |
| Disk | Picks the disk with Asima OS (`asimaos-repair --json` lists them); types its passphrase or recovery key when it is encrypted | `asimaos-repair DISK --probe` opens it and only reports what is there; a wrong passphrase is said here, before anything changes |
| What to do | Sees the versions on the disk, the version the settings come from (D52) and the newest snapshot of the settings and accounts; the start-up is always put back; chooses whether to install this USB stick's version and whether to bring back the settings and accounts | Says whether the stick's version is newer, the same or older than the disk's (going back: settings a newer version changed may need Reset settings); when no version can start, installing the stick's version is needed; bringing back the settings keeps the current ones aside on the disk |
| Repair | Watches the engine's steps; restarts | On failure: what happened, the engine's details and "Try again" (the passphrase is asked again) |

### Engine contract: `asimaos-install`

`asimaos-install`, `asimaos-repair` and `asimaos-rescue-engine` (`engines/`) are three-line starters for one Rust program, `asimaos-engine` (`services/asimaos-engine`): `asimaos-engine install|repair|rescue …` with the options and events below. It lies next to them, on the USB stick (never in the installed `/usr`) and inside Asima Rescue; `ASIMAOS_ENGINE` points the starters at another copy (the end-to-end test).

The GUI runs `asimaos-install DISK --yes --json --encrypt yes|no [--passphrase-stdin] --user NAME --fullname TEXT --password-stdin --hostname NAME --locale … --keymap … --timezone …` as root (through `pkexec` in the live session). Secrets go through stdin, one per line (the disk passphrase when encrypting, then the account password), never on the command line or in the environment. Progress comes back as one JSON object per line:

| Event | Fields |
| --- | --- |
| `step` | `id` (check, prepare, write, recovery (encrypted only), finish), `state` (active, done) |
| `progress` | `value` 0–1, `detail` (e.g. "Copying Asima OS · 2.9 of 5.0 GB") |
| `recovery-key` | `key` (shown once, never stored) |
| `log` / `error` / `done` | `message` / `message` / `version` |

The engine writes the user's language, keyboard (desktop layout plus a console keymap), time zone, computer name and the account (factory `passwd`/`group`/`shadow`/`gshadow` plus the user in `wheel`, `subuid`/`subgid` for rootless containers, a private home from `/etc/skel`) into the new system's `/etc` overlay (`@data`) and `@home`, and `asimaos/install.conf` (version, encrypted). Without encryption the pool is plain Btrfs with the partition label `asimaos_pool`. When the ISO carries `shim/shimx64.efi`, `shim/mmx64.efi` and `shim/asimaos.der`, the ESP gets shim → systemd-boot (as `grubx64.efi`) plus the Asima OS key certificate (D16). `--dry-run` runs every check and simulates the writes (a plain file can stand in for a disk), which lets the GUI be tested end to end without touching a disk.

## Asima Rescue

The repair system on the ESP: a signed UKI that runs entirely in RAM and never mounts the installed `/usr`. Full screen, dark by default, **keyboard-first** (Tab or arrow keys between tools, Enter opens one, Esc goes back), so it works with any keyboard and no mouse. It opens with why it started (e.g. three failed starts) and a grid of tools ordered from gentle to drastic:

| Tool | Does | Needs the pool unlocked |
| --- | --- | --- |
| Start Asima OS again | Restart | no |
| Start the previous version | Boot the other slot once | no |
| Check and repair the disk | File system, both OS versions (signatures, dm-verity), boot files, free space; repair; release the emergency reserve | yes |
| Restore my files | Home folder back to a snapshot; today's files saved as a snapshot first | yes |
| Reinstall Asima OS | From a signed `.aosupd` file on a USB stick (`aosupd install` checks the signature and every part before it writes). Any genuine version, older ones too after a warning (D35); free space like Asima Updater (1.5 × the unpacked size, at least 10 GB; half for the same version). Files, apps and settings stay | yes |
| Secure Boot and unlock | Approve the Asima OS key again (one-time code for the blue MOK screen); turn off automatic unlock | only for the TPM part |
| Factory reset | Same as in Asima Maintenance (D7, D12); the fresh account is chosen here (D10); new recovery key shown once | yes |
| Erase the whole disk | D14: destroy the encryption keys, then erase every block (the SSD's own erase command where possible); serial-number confirmation | no |

Rescue's boot image is never in the TPM's unlock policy (D43), so the TPM never unlocks the disk here: the passphrase or recovery key always does.

### Engine contract: `asimaos-rescue-engine`

Runs as root inside the rescue system. `status --json` prints one object (versions, reason, disk, encrypted/unlocked, Secure Boot, TPM, free space, reserve, `snapshots`, `updateFiles` found on USB sticks, each with its version, unpacked size and whether it is `older` than the installed one). Every action prints the installer's JSON event lines (`step`, `progress`, `log`, `error`, `done`) plus `code` (key approval) and `recovery-key` (factory reset):

```text
asimaos-rescue-engine unlock --json --secret-stdin
asimaos-rescue-engine check|repair|free-space|start-previous|approve-key|remove-tpm --json
asimaos-rescue-engine restore SNAPSHOT --json
asimaos-rescue-engine reinstall FILE --json
asimaos-rescue-engine factory-reset --json --user NAME --fullname TEXT --secrets-stdin   # password, then passphrase
asimaos-rescue-engine wipe --json --serial-tail XXXX
```

Built in beta 3: the GUI (`apps/asimaos-rescue`) and the engine (`asimaos-rescue-engine`, that is `asimaos-engine rescue`; its factory reset reuses `asimaos-reset`) run inside the Rescue UKI (`builder/stages/55-rescue.sh`), which runs a small system from RAM and draws on the firmware framebuffer (Qt `linuxfb`, software rendering). The QEMU tests start it, also with Secure Boot, and check that the engine finds the disk and its version; since Beta 5, `testbed/boot-test.py --rescue` also unlocks the disk inside it and reinstalls from an update file on a USB drive. The engine reads the start menu with `aosupd boot list --plain`.

## Asima Hub

Asima OS's online app, app ID `io.github.asimaos.Hub`, lives in its own repository,
[asimaos/asimaos-hub](https://github.com/asimaos/asimaos-hub), and is **never part of the system image** (D3): it
is a Flatpak, so the system apps never go on the internet (D2). It uses Asima UI (`apps/ui`, copied
into its repository). On Asima OS it talks to Asima Updater's service (`VerifyUpdate`,
`ApplyUpdate`, with the administrator's password) to install a downloaded update, and Asima Manager
offers its newest bundle (`https://github.com/asimaos/asimaos-hub/releases/latest/download/AsimaHub-x86_64.flatpak`).

## Building and screenshots

```bash
apps/build.sh                                   # build all apps into build/apps/
apps/build.sh screenshots                       # also render every screen, light + dark, into apps/screenshots/ (not in git)
apps/build.sh screenshots asimaos-gaming         # render only one app
build/apps/asimaos-updater/asimaos-updater --demo # any app with sample data (needs a display)
build/apps/asimaos-manager/asimaos-manager --demo --state laptop   # Asima Manager on a sample laptop
build/apps/asimaos-rescue/asimaos-rescue --demo   # Asima Rescue on a computer whose start failed
apps/build.sh e2e           # installer end to end (GUI → engine → GUI), dry run, checks the staged account
apps/build.sh services-test # services: unit tests, then end to end on a private D-Bus bus with a scratch
                            # system tree (signed updates, firewall, face sign-in with sample photos), and
                            # each app running against its service
```

Every screen is checked from rendered screenshots in both colour schemes before it counts as done.

## Lessons from the reference distributions

Studied, not copied:

- **A GUI is a thin client over an engine with a stable, read-only status contract**; privileged actions go through polkit (Shani Cassini over shani-deploy).
- **One owner per setting**; other places report and link (Cassini's read-only pages).
- **Ask for a password only when an action starts**; reading status never needs one.
- **Warn before actions that break other things**, e.g. firmware updates changing TPM measurements.
- **Safety invariants for system tools:** write boot entries to a temp file and rename atomically; verify before replacing; fail closed when verification fails (shani-deploy).
- **Installers ask about TPM and Secure Boot as their own steps** (GNOME os-installer as used by Shani OS).
- **Boot blessing can wait for a full desktop session** (KDE Linux) or only for a healthy login screen (Asima OS's choice, so unattended reboots get blessed).
- **Boot counting on hard failures (proven):** shani-deploy notes that `+3-0` boot counting may not fall back automatically on a hard failure with systemd 261 (systemd issue 40405). Asima OS relies on that fallback, and the VM tests proved it with two slots: a broken login screen, a kernel crash and a total freeze all fell back.
