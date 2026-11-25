//! Updates from `.aosupd` files (format: services/asimaos-update). This module adds what only an
//! installed system knows: the running version, free space, the start menu (current / backup),
//! which versions to keep, and the parts applied after the new version has started fine (boot
//! loader, Rescue, migrations).

use anyhow::{Context, Result};
use asimaos_svc::{version, Root};
use asimaos_update::{boot, Targets, Update};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub use asimaos_svc::system::{esp, SLOTS};

pub const KEY: &str = "/usr/share/asimaos/keys/release.pub";
/// A copy of each version's boot image stays next to its system image (`<slots>/boot/`), so
/// "Repair this computer" on the USB stick can rebuild a lost or damaged ESP without
/// losing a version.
pub const BOOT_COPIES: &str = "boot";
pub const MIGRATIONS_DONE: &str = "/var/lib/asimaos/migrations";
/// The factory /etc checksums of this version (stage 30), those last seen, the person's
/// own /etc files (the overlay's upper layer) and the list Asima Maintenance shows.
pub const FACTORY_SUMS: &str = "/usr/share/factory/etc.sha256";
pub const SEEN_SUMS: &str = "/var/lib/asimaos/factory-etc.sha256";
pub const ETC_UPPER: &str = "/run/initramfs/data/overlay/etc/upper";
pub const NEW_DEFAULTS: &str = "/var/lib/asimaos/etc-new-defaults";
/// The failed version whose changes to the system go back at the next start.
pub const ROLLBACK_FLAG: &str = "/run/initramfs/data/flags/rollback";
const GIB: u64 = 1024 * 1024 * 1024;

pub fn targets(root: &Root) -> Targets {
    Targets { slots: root.path(SLOTS), uki_dir: esp(root).join(boot::UKI_DIR), staged: root.path(SLOTS).join("staged") }
}

pub fn open(root: &Root, file: File) -> Result<Update> {
    // The current release key, and the next one when a build ships it.
    let keys = asimaos_update::load_trusted_keys(&root.path(KEY))
        .context("the Asima OS release key is missing from this system")?;
    Update::open_any(file, &keys)
}

pub struct Facts {
    pub running: String,
    pub free: u64,
}

/// Why a signed update can't be installed here: a code for Asima Updater and a sentence.
pub struct Problem {
    pub code: &'static str,
    pub message: String,
}

/// The checks Asima Updater shows before installing. `reinstall` asks for the running version
/// itself ("Reinstall this version", D21); otherwise only newer versions are accepted.
/// Returns (problems, bytes needed).
pub fn assess(m: &asimaos_update::Manifest, facts: &Facts, test_mode: bool, reinstall: bool) -> (Vec<Problem>, u64) {
    let mut problems = Vec::new();
    let mut add = |code, message| problems.push(Problem { code, message });
    if m.arch != "x86_64" {
        add("arch", format!("This update is for {} computers.", m.arch));
    }
    if reinstall {
        if m.version != facts.running {
            add(
                "not-running",
                format!("This file is Asima OS {}; the version running now is {}.", m.version, facts.running),
            );
        }
    } else if !version::newer(&m.version, &facts.running) {
        add(
            "newer",
            format!(
                "Asima OS {} is already installed; this update ({}) is not newer.",
                facts.running, m.version
            ),
        );
    }
    if let Some(min) = &m.min_from {
        if !reinstall && version::newer(min, &facts.running) {
            // More than a year of versions skipped (D52): the steps that bring the settings up to
            // date are no longer in the file.
            add(
                "min-from",
                format!(
                    "This update installs on Asima OS {min} or newer. Install it with Asima Rescue or with \"Repair this computer\" on the USB stick: both keep your files."
                ),
            );
        }
    }
    if let Some(e) = &m.expires {
        if asimaos_update::parse_date(e).unwrap_or(0) < asimaos_svc::today() {
            add("expired", format!("This update expired on {e}. Download a current one."));
        }
    }
    let need = if test_mode { m.unpacked } else { (m.unpacked * 3 / 2).max(10 * GIB) };
    // A reinstall replaces files that are already there: it needs room for one copy at a time.
    let need = if reinstall { need / 2 } else { need };
    if facts.free < need {
        add("space", format!("Not enough free space: {} GB needed.", need.div_ceil(GIB)));
    }
    (problems, need)
}

// ───────────────────────────────────────────────────────────────────────── versions

/// Versions with an image in the slots or an entry in the start menu, newest first.
pub fn installed_versions(root: &Root) -> Vec<String> {
    let mut v: BTreeSet<String> = boot::entries(&esp(root)).into_iter().map(|e| e.version).collect();
    if let Ok(rd) = fs::read_dir(root.path(SLOTS)) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if let Some(ver) = n.strip_prefix("usr_").and_then(|r| r.strip_suffix(".erofs")) {
                if version::valid(ver) {
                    v.insert(ver.to_string());
                }
            }
        }
    }
    let mut v: Vec<String> = v.into_iter().collect();
    v.sort_by(|a, b| version::compare(b, a).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Removes every version that is neither in the start menu nor running: its images, its boot
/// image and its staged parts. The menu holds the current version and its backup (and a failed
/// update until the next one), so two working versions stay, as the design says.
pub fn prune(root: &Root, running: &str) -> Result<Vec<String>> {
    let keep: BTreeSet<String> =
        boot::entries(&esp(root)).into_iter().map(|e| e.version).chain([running.to_string()]).collect();
    let mut removed = Vec::new();
    for ver in installed_versions(root).into_iter().filter(|v| !keep.contains(v)) {
        for f in [format!("usr_{ver}.erofs"), format!("verity_{ver}.img")] {
            let _ = fs::remove_file(root.path(SLOTS).join(f));
        }
        let _ = fs::remove_dir_all(root.path(SLOTS).join("staged").join(&ver));
        let _ = fs::remove_file(esp(root).join(boot::UKI_DIR).join(boot::uki_name(&ver)));
        let _ = fs::remove_file(root.path(SLOTS).join(BOOT_COPIES).join(boot::uki_name(&ver)));
        removed.push(ver);
    }
    // Leftovers of interrupted writes of versions that are gone.
    if let Ok(rd) = fs::read_dir(root.path(SLOTS)) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".partial") && !keep.iter().any(|v| n.contains(v.as_str())) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    Ok(removed)
}

/// Keeps a copy of `version`'s boot image in the slots (see [`BOOT_COPIES`]); the slots must be
/// writable. Nothing to do when the copy is there already.
pub fn keep_boot_copy(root: &Root, version: &str) -> Result<()> {
    let src = esp(root).join(boot::UKI_DIR).join(boot::uki_name(version));
    let dir = root.path(SLOTS).join(BOOT_COPIES);
    let dst = dir.join(boot::uki_name(version));
    if dst.is_file() && fs::metadata(&dst).map(|m| m.len()).ok() == fs::metadata(&src).map(|m| m.len()).ok() {
        return Ok(());
    }
    fs::create_dir_all(&dir)?;
    replace(&src, &dst).with_context(|| format!("can't keep a copy of the boot image of {version}"))
}

// ───────────────────────────────────────────────────────────────────────── after a good start

fn replace(src: &Path, dst: &Path) -> Result<()> {
    let tmp = dst.with_extension("partial");
    fs::copy(src, &tmp).with_context(|| format!("can't write {}", tmp.display()))?;
    File::open(&tmp)?.sync_all()?;
    fs::rename(&tmp, dst)?;
    Ok(())
}

/// Puts a staged boot loader and shim in place of the old ones (without a trial, and the
/// last step of a trial that worked).
fn install_loader(esp: &Path, from: &Path, done: &mut Vec<String>) -> Result<()> {
    let bootloader = from.join(LOADER_FILE);
    if bootloader.is_file() {
        // With shim (Secure Boot via the Asima OS key) systemd-boot is EFI/BOOT/grubx64.efi.
        let default =
            if esp.join("EFI/BOOT/grubx64.efi").is_file() { "EFI/BOOT/grubx64.efi" } else { "EFI/BOOT/BOOTX64.EFI" };
        replace(&bootloader, &esp.join(default))?;
        if esp.join("EFI/systemd").is_dir() {
            replace(&bootloader, &esp.join("EFI/systemd/systemd-bootx64.efi"))?;
        }
        fs::remove_file(&bootloader)?;
        done.push("boot loader".into());
    }
    // A new shim (for example one signed with Microsoft's 2023 certificate) replaces the old one
    // on PCs that start through shim; MokManager first, so the pair always matches what shim
    // calls. Elsewhere (the firmware starts systemd-boot directly) it is not needed.
    let (shim, mm) = (from.join("shimx64.efi"), from.join("mmx64.efi"));
    if shim.is_file() && mm.is_file() {
        if esp.join("EFI/BOOT/grubx64.efi").is_file() {
            replace(&mm, &esp.join("EFI/BOOT/mmx64.efi"))?;
            replace(&shim, &esp.join("EFI/BOOT/BOOTX64.EFI"))?;
            done.push("Secure Boot shim".into());
        }
        fs::remove_file(&shim)?;
        fs::remove_file(&mm)?;
    }
    Ok(())
}

const LOADER_FILE: &str = "systemd-bootx64.efi";
/// The trial copy on the ESP, and what this system remembers about it: "BOOTNUM ATTEMPTS".
pub const TRIAL_DIR: &str = "EFI/asimaos/trial";
pub const TRIAL_STATE: &str = "/var/lib/asimaos/loader-trial";

/// Once per good start while a new boot loader or shim waits in `staged`:
/// - nothing on trial yet: copy it to EFI/asimaos/trial, make a firmware boot entry for it and
///   start it once (BootNext);
/// - this start came from the trial copy: it works, so it replaces the old one;
/// - this start came from the old one (the trial didn't reach Asima OS, or the firmware skipped
///   it): try once more, then keep the old boot loader for good.
///
/// Errors mean no trial is possible here (no EFI, no ESP entry): the caller installs directly.
fn loader_trial(root: &Root, esp: &Path, staged: &Path) -> Result<Option<String>> {
    use asimaos_svc::efi;
    let new_loader = staged.join(LOADER_FILE).is_file();
    let new_shim = staged.join("shimx64.efi").is_file() && staged.join("mmx64.efi").is_file();
    let uses_shim = esp.join("EFI/BOOT/grubx64.efi").is_file();
    if !new_loader && !(new_shim && uses_shim) {
        // A shim for a PC that doesn't start through shim: not needed (as before).
        let _ = fs::remove_file(staged.join("shimx64.efi"));
        let _ = fs::remove_file(staged.join("mmx64.efi"));
        return Ok(None);
    }
    // The same files as installed (most updates): nothing to try, nothing to write.
    let same = |name: &str, current: &str| {
        let staged_file = staged.join(name);
        !staged_file.is_file() || fs::read(&staged_file).ok() == fs::read(esp.join(current)).ok()
    };
    let current_loader = if uses_shim { "EFI/BOOT/grubx64.efi" } else { "EFI/BOOT/BOOTX64.EFI" };
    if same(LOADER_FILE, current_loader)
        && (!uses_shim || (same("shimx64.efi", "EFI/BOOT/BOOTX64.EFI") && same("mmx64.efi", "EFI/BOOT/mmx64.efi")))
        && !root.path(TRIAL_STATE).exists()
    {
        for f in [LOADER_FILE, "shimx64.efi", "mmx64.efi"] {
            let _ = fs::remove_file(staged.join(f));
        }
        return Ok(None);
    }
    let trial = esp.join(TRIAL_DIR);
    let state = root.path(TRIAL_STATE);
    let remembered = fs::read_to_string(&state).ok().and_then(|t| {
        let mut it = t.split_whitespace();
        Some((u16::from_str_radix(it.next()?, 16).ok()?, it.next()?.parse::<u32>().ok()?))
    });
    let from_trial = efi::string(root, "LoaderImageIdentifier", efi::LOADER)
        .is_some_and(|p| p.to_lowercase().replace('/', "\\").contains("\\asimaos\\trial\\"));
    let forget = |num: u16| {
        efi::delete(root, &format!("Boot{num:04X}"), efi::GLOBAL);
        let _ = fs::remove_dir_all(&trial);
        let _ = fs::remove_file(&state);
    };
    if let Some((num, attempts)) = remembered {
        if from_trial {
            let mut done = Vec::new();
            install_loader(esp, staged, &mut done)?;
            forget(num);
            return Ok(Some(format!("{} (it started once on trial, now it is the default)", done.join(" and "))));
        }
        if attempts >= 2 {
            forget(num);
            let _ = fs::remove_file(staged.join(LOADER_FILE));
            let _ = fs::remove_file(staged.join("shimx64.efi"));
            let _ = fs::remove_file(staged.join("mmx64.efi"));
            return Ok(Some("the new boot loader didn't start on this computer; the one before stays".into()));
        }
        efi::set_boot_next(root, num)?;
        fs::write(&state, format!("{num:04X} {}\n", attempts + 1))?;
        return Ok(Some("the new boot loader gets one more trial start".into()));
    }
    // A shim this PC's firmware wouldn't start (signed by a certificate missing from its db, or
    // revoked in its dbx) is never tried: some firmware stops at a Secure Boot violation screen.
    // The shim installed stays (with its MokManager); a new boot loader is still tried behind it.
    let mut note = None;
    if uses_shim && !same("shimx64.efi", "EFI/BOOT/BOOTX64.EFI") && efi::secure_boot(root) == "on" {
        use asimaos_svc::authenticode::{firmware_trust, Trust};
        let trust = firmware_trust(root, &fs::read(staged.join("shimx64.efi"))?);
        let why = match trust {
            Ok(Trust::Revoked) => Some("its firmware has revoked it"),
            Ok(Trust::UnknownSigner) => Some("it is signed with a certificate the firmware doesn't have"),
            Ok(Trust::Unsigned) => Some("it isn't signed"),
            Ok(Trust::Trusted) | Err(_) => None, // when the firmware's lists can't be read, the trial start decides
        };
        if let Some(why) = why {
            let _ = fs::remove_file(staged.join("shimx64.efi"));
            let _ = fs::remove_file(staged.join("mmx64.efi"));
            let n = format!("this computer wouldn't start the update's Secure Boot shim ({why}): the one before stays");
            if same(LOADER_FILE, current_loader) {
                return Ok(Some(n));
            }
            note = Some(n);
        }
    }
    // A new trial: the boot chain as it will be, in its own folder (shim starts grubx64.efi and
    // MokManager from its own folder; systemd-boot finds the start menu on the ESP either way).
    anyhow::ensure!(efi::is_efi(root), "not started in UEFI mode");
    let guid = efi::string(root, "LoaderDevicePartUUID", efi::LOADER).context("the boot loader didn't name the ESP")?;
    let dev = if root.is_system() {
        asimaos_svc::system::esp_device().context("the ESP was not found")?
    } else {
        root.path("/dev/esp")
    };
    let part = efi::partition_of(root, &dev, &guid).context("the ESP's place on the disk is unknown")?;
    let _ = fs::remove_dir_all(&trial);
    fs::create_dir_all(&trial)?;
    let pick = |name: &str, current: &str| -> Result<()> {
        let src = if staged.join(name).is_file() { staged.join(name) } else { esp.join(current) };
        let dst = trial.join(if name == LOADER_FILE && uses_shim { "grubx64.efi" } else { name });
        replace(&src, &dst)
    };
    let first = if uses_shim {
        pick("shimx64.efi", "EFI/BOOT/BOOTX64.EFI")?;
        pick("mmx64.efi", "EFI/BOOT/mmx64.efi")?;
        pick(LOADER_FILE, "EFI/BOOT/grubx64.efi")?;
        "shimx64.efi"
    } else {
        pick(LOADER_FILE, "EFI/BOOT/BOOTX64.EFI")?;
        LOADER_FILE
    };
    let option = efi::load_option(
        "Asima OS (new boot loader, on trial)",
        &part,
        &format!("\\{}\\{first}", TRIAL_DIR.replace('/', "\\")),
    )
    .context("the ESP's GUID is not valid")?;
    let num = efi::free_boot_number(root).context("no free firmware boot entry")?;
    efi::write(root, &format!("Boot{num:04X}"), efi::GLOBAL, &option)?;
    efi::set_boot_next(root, num)?;
    fs::create_dir_all(state.parent().unwrap())?;
    fs::write(&state, format!("{num:04X} 1\n"))?;
    let msg = "new boot loader: it starts once on trial at the next start, the one before stays until then";
    Ok(Some(note.map(|n| format!("{n}; {msg}")).unwrap_or_else(|| msg.into())))
}

/// After a start that systemd-bless-boot marked good (asimaos-finish-update.service):
/// 1. the start menu settles: after a fallback the working version becomes current again;
/// 2. the boot loader, the shim and Rescue from an update are installed, only now that this
///    version has proven it starts. Returns what was done.
pub fn finish(root: &Root, running: &str) -> Result<Vec<String>> {
    let esp = esp(root);
    let mut done = Vec::new();
    let failed = boot::entries(&esp)
        .into_iter()
        .find(|e| e.role == boot::Role::Current && e.bad() && e.version != running)
        .map(|e| e.version);
    if boot::settle(&esp, running)? {
        done.push(format!("start menu: Asima OS {running} is current again after a failed update"));
        // Its migration steps changed the system, so the settings and system state go
        // back to the snapshot from before it, at the next start (asimaos-early-mount).
        if let Some(f) = failed.filter(|f| root.path(MIGRATIONS_DONE).join(f).exists()) {
            let flag = root.path(ROLLBACK_FLAG);
            fs::create_dir_all(flag.parent().unwrap())?;
            fs::write(&flag, format!("{f}\n"))?;
            done.push(format!("settings: back to how they were before the update to {f}, at the next start"));
        }
    }
    let me = boot::entries(&esp).into_iter().find(|e| e.version == running);
    let staged = root.path(SLOTS).join("staged").join(running);
    // Systems installed before boot images were kept in the slots get the copy now (quietly:
    // nothing changes for the person, and a failure only means the repair has one copy less).
    if me.as_ref().is_some_and(|e| e.blessed()) {
        let _ = keep_boot_copy(root, running);
    }
    if !staged.is_dir() || !me.is_some_and(|e| e.blessed()) {
        boot::sync_rescue(&esp)?;
        return Ok(done);
    }
    // A new boot loader or shim first starts once on trial (BootNext), and replaces the
    // old one only after that start worked. Without a usable firmware boot entry, as before.
    match loader_trial(root, &esp, &staged) {
        Ok(Some(msg)) => done.push(msg),
        Ok(None) => {}
        Err(e) => {
            eprintln!("Asima OS: no trial start for the new boot loader ({e}); installing it directly");
            install_loader(&esp, &staged, &mut done)?;
        }
    }
    // Asima Rescue keeps the name of the file it was packed from: the build packs
    // `asimaos-rescue_<version>.efi`.
    let rescue = [format!("asimaos-rescue_{running}.efi"), boot::RESCUE_UKI.to_string()]
        .into_iter()
        .map(|name| staged.join(name))
        .find(|p| p.is_file());
    if let Some(rescue) = rescue {
        fs::create_dir_all(esp.join(boot::UKI_DIR))?;
        replace(&rescue, &esp.join(boot::UKI_DIR).join(boot::RESCUE_UKI))?;
        fs::remove_file(&rescue)?;
        done.push("Asima Rescue".into());
    }
    boot::sync_rescue(&esp)?;
    Ok(done)
}

/// The version the system's settings (/etc and /var) are in: written at the first start of a
/// fresh installation or reset, and after an update's migration steps (D52). Going back to the
/// snapshot from before an update brings the earlier one back with /var.
pub const STATE_VERSION: &str = "/var/lib/asimaos/state-version";
/// Written when the settings come from a version older than the oldest step the update carried
/// (more than a year skipped, through Asima Rescue or the USB stick's repair): the steps it had
/// were run, and Asima Maintenance suggests Reset settings.
pub const SETTINGS_BEHIND: &str = "/var/lib/asimaos/settings-behind";
/// In an update's migration steps: the oldest version they bring up to date (its `MIN_FROM`).
pub const STEPS_FROM: &str = "steps-from";

/// The version a migration step belongs to: its name starts with it (`2026.12.10.1-name.sh`).
pub fn step_version(name: &str) -> Option<&str> {
    let v = name.split_once('-')?.0;
    version::valid(v).then_some(v)
}

/// The one-time migration steps an update brought, early in the new version's first start (D52):
/// every step newer than the version the settings come from, up to the running one, in version
/// order, each once. They must keep the previous version working too, because going back reuses
/// the same /etc and /var. A fresh installation or reset needs none: its settings are already the
/// running version's. Returns the number of steps run.
pub fn run_migrations(root: &Root, running: &str) -> Result<usize> {
    let state = root.path(STATE_VERSION);
    let Some(from) = fs::read_to_string(&state).ok().map(|s| s.trim().to_string()).filter(|s| version::valid(s)) else {
        write_state(root, running)?;
        return Ok(0);
    };
    // Settings of a newer version (going back by hand) stay as they are.
    if !version::newer(running, &from) {
        return Ok(0);
    }
    let dir = root.path(SLOTS).join("staged").join(running).join("migrations");
    let mut steps: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect())
        .unwrap_or_default();
    steps.retain(|(name, path)| {
        path.is_file()
            && step_version(name).is_some_and(|v| version::newer(v, &from) && !version::newer(v, running))
    });
    steps.sort_by(|a, b| {
        let (va, vb) = (step_version(&a.0).unwrap_or_default(), step_version(&b.0).unwrap_or_default());
        version::compare(va, vb).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0))
    });
    let done = root.path(MIGRATIONS_DONE).join("steps");
    fs::create_dir_all(&done)?;
    let mut ran = 0;
    for (name, path) in &steps {
        // A step that finished before an interrupted start is not run twice.
        if done.join(name).exists() {
            continue;
        }
        let ok = std::process::Command::new("/usr/bin/bash").arg(path).status().map(|st| st.success()).unwrap_or(false);
        anyhow::ensure!(ok, "migration step {name} failed");
        fs::write(done.join(name), format!("{running}\n"))?;
        ran += 1;
    }
    // The oldest version these steps bring up to date: settings older than that skipped steps
    // this update no longer carries.
    let steps_from = fs::read_to_string(dir.join(STEPS_FROM)).ok().map(|s| s.trim().to_string());
    if let Some(w) = steps_from.filter(|w| version::valid(w) && version::newer(w, &from)) {
        fs::write(root.path(SETTINGS_BEHIND), format!("{from} {w}\n"))?;
    }
    // Asima OS uses this mark to put the settings back if this version fails (finish).
    if ran > 0 {
        fs::write(root.path(MIGRATIONS_DONE).join(running), format!("{ran}\n"))?;
    }
    write_state(root, running)?;
    Ok(ran)
}

fn write_state(root: &Root, running: &str) -> Result<()> {
    let state = root.path(STATE_VERSION);
    fs::create_dir_all(state.parent().unwrap())?;
    fs::write(&state, format!("{running}\n"))?;
    Ok(())
}

/// Settings files whose factory default changed with this version while the person had their own
/// copy. Their own copy stays; Asima Maintenance lists them, so the new default can be
/// compared and taken. Notes stay until the person's copy is gone. Returns how many are new.
pub fn note_new_defaults(root: &Root) -> Result<usize> {
    let new = fs::read_to_string(root.path(FACTORY_SUMS)).unwrap_or_default();
    let seen = root.path(SEEN_SUMS);
    let old = fs::read_to_string(&seen).unwrap_or_default();
    if new.is_empty() || old == new {
        return Ok(0);
    }
    let parse = |t: &str| -> std::collections::HashMap<String, String> {
        t.lines()
            .filter_map(|l| l.split_once("  "))
            .map(|(h, p)| (p.trim_start_matches("./").to_string(), h.to_string()))
            .collect()
    };
    let (before, now) = (parse(&old), parse(&new));
    let upper = root.path(ETC_UPPER);
    let mine = |rel: &str| upper.join(rel).symlink_metadata().is_ok();
    let list = root.path(NEW_DEFAULTS);
    let mut notes: BTreeSet<String> = fs::read_to_string(&list)
        .unwrap_or_default()
        .lines()
        .filter(|l| mine(l.trim_start_matches("/etc/")))
        .map(String::from)
        .collect();
    let fresh: Vec<String> = now
        .iter()
        .filter(|(rel, hash)| before.get(*rel).is_some_and(|h| h != *hash) && mine(rel))
        .map(|(rel, _)| format!("/etc/{rel}"))
        .collect();
    let count = fresh.iter().filter(|f| !notes.contains(*f)).count();
    notes.extend(fresh);
    fs::create_dir_all(list.parent().unwrap())?;
    fs::write(&list, notes.into_iter().map(|n| n + "\n").collect::<String>())?;
    fs::write(&seen, new)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_var(path: &Path, text: &str) {
        let mut v = vec![7u8, 0, 0, 0];
        v.extend(text.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()));
        fs::write(path, v).unwrap();
    }

    #[test]
    fn boot_loader_trial() {
        use asimaos_svc::efi::{GLOBAL, LOADER};
        let dir = std::env::temp_dir().join(format!("ud-trial-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = Root(dir.clone());
        let esp = dir.join("esp");
        let staged = dir.join("staged");
        let vars = root.path("/sys/firmware/efi/efivars");
        for d in [esp.join("EFI/BOOT"), staged.clone(), vars.clone(), dir.join("sys/devices/disk/esp"), dir.join("dev")]
        {
            fs::create_dir_all(d).unwrap();
        }
        fs::create_dir_all(dir.join("sys/devices/disk/queue")).unwrap();
        fs::write(dir.join("sys/devices/disk/queue/logical_block_size"), "512\n").unwrap();
        for (f, v) in [("partition", "1"), ("start", "2048"), ("size", "1048576")] {
            fs::write(dir.join("sys/devices/disk/esp").join(f), v).unwrap();
        }
        fs::create_dir_all(dir.join("sys/class/block")).unwrap();
        std::os::unix::fs::symlink(dir.join("sys/devices/disk/esp"), dir.join("sys/class/block/esp")).unwrap();
        fs::write(dir.join("dev/esp"), "").unwrap();
        utf16_var(&vars.join(format!("LoaderDevicePartUUID-{LOADER}")), "C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
        fs::write(esp.join("EFI/BOOT/BOOTX64.EFI"), "old shim").unwrap();
        fs::write(esp.join("EFI/BOOT/mmx64.efi"), "old mm").unwrap();
        fs::write(esp.join("EFI/BOOT/grubx64.efi"), "old loader").unwrap();
        fs::write(staged.join(LOADER_FILE), "new loader").unwrap();

        // 0. The same boot loader as installed (most updates): no trial, the staged copy goes.
        fs::write(staged.join(LOADER_FILE), "old loader").unwrap();
        assert!(loader_trial(&root, &esp, &staged).unwrap().is_none());
        assert!(!staged.join(LOADER_FILE).exists() && !esp.join(TRIAL_DIR).exists());
        fs::write(staged.join(LOADER_FILE), "new loader").unwrap();

        // 1. A good start with a new loader waiting: it goes on trial, the old one stays.
        let msg = loader_trial(&root, &esp, &staged).unwrap().unwrap();
        assert!(msg.contains("trial"), "{msg}");
        assert_eq!(fs::read_to_string(esp.join("EFI/BOOT/grubx64.efi")).unwrap(), "old loader");
        assert_eq!(fs::read_to_string(esp.join(TRIAL_DIR).join("grubx64.efi")).unwrap(), "new loader");
        assert_eq!(fs::read_to_string(esp.join(TRIAL_DIR).join("shimx64.efi")).unwrap(), "old shim");
        assert!(vars.join(format!("Boot0100-{GLOBAL}")).is_file());
        assert_eq!(fs::read(vars.join(format!("BootNext-{GLOBAL}"))).unwrap(), vec![7, 0, 0, 0, 0, 1]);
        assert_eq!(fs::read_to_string(root.path(TRIAL_STATE)).unwrap(), "0100 1\n");

        // 2. The trial start reached Asima OS: the new loader becomes the default.
        utf16_var(&vars.join(format!("LoaderImageIdentifier-{LOADER}")), "\\EFI\\asimaos\\trial\\grubx64.efi");
        let msg = loader_trial(&root, &esp, &staged).unwrap().unwrap();
        assert!(msg.contains("now it is the default"), "{msg}");
        assert_eq!(fs::read_to_string(esp.join("EFI/BOOT/grubx64.efi")).unwrap(), "new loader");
        assert!(!esp.join(TRIAL_DIR).exists() && !root.path(TRIAL_STATE).exists());
        assert!(!vars.join(format!("Boot0100-{GLOBAL}")).exists() && !staged.join(LOADER_FILE).exists());

        // 3. Another new loader that never reaches Asima OS: one more try, then the old one stays.
        fs::write(staged.join(LOADER_FILE), "broken loader").unwrap();
        utf16_var(&vars.join(format!("LoaderImageIdentifier-{LOADER}")), "\\EFI\\BOOT\\grubx64.efi");
        loader_trial(&root, &esp, &staged).unwrap();
        assert!(loader_trial(&root, &esp, &staged).unwrap().unwrap().contains("one more"));
        assert!(loader_trial(&root, &esp, &staged).unwrap().unwrap().contains("didn't start"));
        assert_eq!(fs::read_to_string(esp.join("EFI/BOOT/grubx64.efi")).unwrap(), "new loader");
        assert!(!staged.join(LOADER_FILE).exists() && !esp.join(TRIAL_DIR).exists());

        // 4. With Secure Boot on, a new shim the firmware wouldn't start (here: not signed) is never
        //    tried; the installed shim and its MokManager stay.
        let mut pe = vec![0u8; 0x40 + 24 + 112 + 128];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c] = 0x40;
        pe[0x40..0x44].copy_from_slice(b"PE\0\0");
        pe[0x40 + 24..0x40 + 26].copy_from_slice(&0x20bu16.to_le_bytes());
        pe[0x40 + 24 + 108] = 16;
        fs::write(staged.join("shimx64.efi"), &pe).unwrap();
        fs::write(staged.join("mmx64.efi"), "new mm").unwrap();
        fs::write(vars.join(format!("SecureBoot-{GLOBAL}")), [6, 0, 0, 0, 1]).unwrap();
        let db = format!("db-{}", asimaos_svc::authenticode::SECURITY_DATABASE);
        fs::write(vars.join(db), [7, 0, 0, 0]).unwrap();
        let msg = loader_trial(&root, &esp, &staged).unwrap().unwrap();
        assert!(msg.contains("wouldn't start the update's Secure Boot shim"), "{msg}");
        assert!(!staged.join("shimx64.efi").exists() && !staged.join("mmx64.efi").exists());
        assert_eq!(fs::read_to_string(esp.join("EFI/BOOT/BOOTX64.EFI")).unwrap(), "old shim");
        assert!(!esp.join(TRIAL_DIR).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rescue_from_an_update() {
        let dir = std::env::temp_dir().join(format!("ud-rescue-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = Root(dir.clone());
        let v = "2026.12.01.1";
        let esp = esp(&root);
        fs::create_dir_all(esp.join(boot::UKI_DIR)).unwrap();
        boot::write_entry(&esp, boot::Role::Current, v, None).unwrap();
        fs::write(esp.join(boot::UKI_DIR).join(boot::uki_name(v)), "uki").unwrap();
        fs::write(esp.join(boot::UKI_DIR).join(boot::RESCUE_UKI), "old rescue").unwrap();
        let staged = root.path(SLOTS).join("staged").join(v);
        fs::create_dir_all(&staged).unwrap();
        // The build packs Rescue under its file's name, with the version in it.
        fs::write(staged.join(format!("asimaos-rescue_{v}.efi")), "new rescue").unwrap();
        let done = finish(&root, v).unwrap();
        assert!(done.iter().any(|d| d == "Asima Rescue"), "{done:?}");
        assert_eq!(fs::read_to_string(esp.join(boot::UKI_DIR).join(boot::RESCUE_UKI)).unwrap(), "new rescue");
        assert!(!staged.join(format!("asimaos-rescue_{v}.efi")).exists());
        assert!(esp.join(boot::ENTRIES_DIR).join("asimaos-rescue.conf").is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn new_defaults_of_changed_files() {
        let dir = std::env::temp_dir().join(format!("ud-etc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = Root(dir.clone());
        let sums = root.path(FACTORY_SUMS);
        fs::create_dir_all(sums.parent().unwrap()).unwrap();
        let upper = root.path(ETC_UPPER);
        fs::create_dir_all(upper.join("ssh")).unwrap();
        fs::write(upper.join("hostname"), "pc\n").unwrap(); // changed by the person
        fs::write(upper.join("ssh/sshd_config"), "x\n").unwrap();
        // First start: nothing to compare with.
        fs::write(&sums, "aa  ./hostname\nbb  ./ssh/sshd_config\ncc  ./locale.conf\n").unwrap();
        assert_eq!(note_new_defaults(&root).unwrap(), 0);
        // An update changes the defaults of sshd_config (changed by the person) and locale.conf (not).
        fs::write(&sums, "aa  ./hostname\nb2  ./ssh/sshd_config\nc2  ./locale.conf\n").unwrap();
        assert_eq!(note_new_defaults(&root).unwrap(), 1);
        assert_eq!(fs::read_to_string(root.path(NEW_DEFAULTS)).unwrap(), "/etc/ssh/sshd_config\n");
        // The same version again: nothing new, the note stays.
        assert_eq!(note_new_defaults(&root).unwrap(), 0);
        assert_eq!(fs::read_to_string(root.path(NEW_DEFAULTS)).unwrap(), "/etc/ssh/sshd_config\n");
        // The person took the factory version: the note goes at the next comparison.
        fs::remove_file(upper.join("ssh/sshd_config")).unwrap();
        fs::write(&sums, "a3  ./hostname\nb2  ./ssh/sshd_config\nc2  ./locale.conf\n").unwrap();
        assert_eq!(note_new_defaults(&root).unwrap(), 1);
        assert_eq!(fs::read_to_string(root.path(NEW_DEFAULTS)).unwrap(), "/etc/hostname\n");
        let _ = fs::remove_dir_all(dir);
    }
}
