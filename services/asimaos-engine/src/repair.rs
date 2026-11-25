//! "Repair this computer" (`asimaos-engine repair`, on the USB stick as `asimaos-repair`).
//!
//!   asimaos-repair                       list the disks that hold Asima OS
//!   asimaos-repair /dev/DISK [options]   make the Asima OS on DISK start again
//!
//! It never touches anyone's files. It:
//!   1. opens the pool (the passphrase or the recovery key, when it is encrypted)
//!   2. checks the boot partition (ESP); only when it can't be repaired is it formatted again
//!   3. puts back the boot loader (with the Microsoft-signed shim), the boot image of each kept
//!      version (from @slots/boot, or this USB stick's own when it is the same version), Asima
//!      Rescue and the start menu: the newest version current, the one before it as the backup
//!   4. with --restore-account: the settings and accounts (/etc) as they were in the newest
//!      snapshot; the ones there now are kept aside as @data.before-repair-<date>
//!
//! With --install-this-version, before 2: this USB stick's own Asima OS is written into the
//! system slots and becomes the current version, the newest one already there its backup; files
//! and settings stay.

use crate::common::{self, ask, ask_secret, ok, read_file, read_line, run, run_input, stop, Out};
use anyhow::Result;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
asimaos-repair                       list the disks that hold Asima OS
asimaos-repair /dev/DISK [options]   make the Asima OS on DISK start again

Options:
  --yes                   skip the questions (the graphical installer asks itself)
  --json                  machine-readable progress, one JSON object per line (like asimaos-install)
  --passphrase-stdin      read the passphrase or recovery key from standard input
  --restore-account       also bring back the settings and accounts from the newest snapshot
  --install-this-version  also install this USB stick's version, keeping files and settings
  --probe                 only open the pool and report what is there (with --json: the versions,
                          the version the settings come from, the newest snapshot); changes nothing
  --source DIR            the USB stick's Asima OS files (default /run/initramfs/live/asimaos)
  --interactive           no disk given: choose one from a list, and say what else to do
                          (the installer's \"Repair\" button opens this)";

const WORK: &str = "/run/asimaos-repair";
const MAPPER: &str = "asimaos-repair-pool";

struct Opts {
    target: String,
    yes: bool,
    pass_stdin: bool,
    restore_account: bool,
    install_stick: bool,
    probe: bool,
    interactive: bool,
    source: PathBuf,
}

/// Unmounts and closes what repair opened, however it ends; in the guided mode the window stays
/// open until the person has read the result.
struct Cleanup {
    interactive: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let work = Path::new(WORK);
        let _ = ok("umount", &["-q", &work.join("esp").to_string_lossy()]);
        let _ = ok("umount", &["-q", &work.join("top").to_string_lossy()]);
        if Path::new("/dev/mapper").join(MAPPER).exists() {
            let _ = ok("cryptsetup", &["close", MAPPER]);
        }
        let _ = fs::remove_dir_all(work);
        if self.interactive {
            ask("\nPress Enter to close this window. ");
        }
    }
}

/// The two newest versions (newest first) of a list.
pub fn newest_two(mut versions: Vec<String>) -> Vec<String> {
    versions.sort_by(|a, b| if common::older(a, b) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less });
    versions.dedup();
    versions.truncate(2);
    versions
}

/// With the stick's version installed: it first, then the newest other one.
pub fn with_stick(versions: &[String], stick: &str) -> Vec<String> {
    let mut keep = vec![stick.to_string()];
    keep.extend(versions.iter().filter(|v| *v != stick).take(1).cloned());
    keep
}

pub fn main(args: &[String], out: &mut Out) -> Result<()> {
    let mut o = Opts {
        target: String::new(),
        yes: false,
        pass_stdin: false,
        restore_account: false,
        install_stick: false,
        probe: false,
        interactive: false,
        source: PathBuf::from("/run/initramfs/live/asimaos"),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yes" => o.yes = true,
            "--json" => out.json = true,
            "--passphrase-stdin" => o.pass_stdin = true,
            "--restore-account" => o.restore_account = true,
            "--install-this-version" => o.install_stick = true,
            "--probe" => o.probe = true,
            "--source" => {
                o.source = PathBuf::from(it.next().ok_or_else(|| common::Stop("--source needs a folder".into()))?)
            }
            "--interactive" => o.interactive = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            s if s.starts_with('-') => return stop(format!("unknown option {s}")),
            s => {
                if !o.target.is_empty() {
                    return stop("only one disk");
                }
                o.target = s.to_string();
            }
        }
    }
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } != 0 {
        return stop("run it as root (sudo)");
    }
    let _cleanup = Cleanup { interactive: o.interactive };
    let stick_version = read_file(&o.source.join("VERSION")).trim().to_string();

    if o.target.is_empty() && o.interactive {
        println!("Repair this computer: Asima OS starts again, nobody's files are touched.\n");
        let disks = common::asimaos_disks();
        if disks.is_empty() {
            return stop("no disk with Asima OS was found on this computer");
        }
        for (i, d) in disks.iter().enumerate() {
            println!("  {}. {d}  {} {}", i + 1, common::lsblk_value(d, "SIZE"), common::lsblk_value(d, "MODEL"));
        }
        let n: usize = ask("\nWhich disk? (number) ").trim().parse().unwrap_or(0);
        if n < 1 || n > disks.len() {
            return stop("stopped; nothing was changed");
        }
        o.target = disks[n - 1].clone();
        o.restore_account = ask("Also bring back the settings and accounts from the newest snapshot? (y/N) ")
            .to_lowercase()
            .starts_with('y');
        o.install_stick = ask(&format!(
            "Also install this USB stick's Asima OS {stick_version} (your files and settings stay)? (y/N) "
        ))
        .to_lowercase()
        .starts_with('y');
    }
    if o.target.is_empty() {
        let disks = common::asimaos_disks();
        // The repair screens (the installer's "Repair" button): the disks, and this stick's version.
        if out.json {
            let list: Vec<serde_json::Value> = disks
                .iter()
                .map(|d| {
                    json!({
                        "path": d,
                        "size": common::lsblk_value(d, "SIZE"),
                        "model": common::lsblk_value(d, "MODEL"),
                        "encrypted": common::part_of(d, "crypt_pool").is_some(),
                    })
                })
                .collect();
            out.event(json!({ "event": "disks", "disks": list, "stick": stick_version }));
            return Ok(());
        }
        if disks.is_empty() {
            println!("No disk with Asima OS was found.");
        }
        for d in disks {
            println!("{d}  {} {}", common::lsblk_value(&d, "SIZE"), common::lsblk_value(&d, "MODEL"));
        }
        return Ok(());
    }

    // ── 1. check and open ──
    out.step("check", "active", &format!("Find Asima OS on {}", o.target));
    let esp_part = common::part_of(&o.target, "ESP");
    let (pool_part, encrypted) = match common::part_of(&o.target, "crypt_pool") {
        Some(p) => (Some(p), true),
        None => (common::part_of(&o.target, "asimaos_pool"), false),
    };
    let (Some(esp_part), Some(pool_part)) = (esp_part, pool_part) else {
        return stop(format!("{} has no Asima OS (no ESP and pool partitions)", o.target));
    };
    if !o.source.join("systemd-bootx64.efi").is_file() {
        return stop(format!("the USB stick's Asima OS files are missing ({})", o.source.display()));
    }
    let work = PathBuf::from(WORK);
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(work.join("esp"))?;
    fs::create_dir_all(work.join("top"))?;
    let top = work.join("top");
    let mut pool_dev = pool_part.clone();
    if encrypted {
        out.step("unlock", "active", "Open the encrypted pool");
        let secret = if o.pass_stdin {
            read_line(&mut std::io::stdin().lock())
                .ok_or_else(|| common::Stop("no passphrase on standard input".into()))?
        } else {
            ask_secret(&format!("Passphrase or recovery key for {pool_part}: "))?
        };
        if run_input("cryptsetup", &["open", "--key-file=-", &pool_part, MAPPER], secret.as_bytes(), &[]).is_err() {
            return stop("the pool could not be opened: wrong passphrase or recovery key");
        }
        pool_dev = format!("/dev/mapper/{MAPPER}");
        out.step("unlock", "done", "");
    }
    if !ok("mount", &["-t", "btrfs", "-o", "subvolid=5", &pool_dev, &top.to_string_lossy()]) {
        return stop("the pool could not be opened (Btrfs)");
    }
    let slots = top.join("@slots");
    if !slots.is_dir() {
        return stop("the pool holds no Asima OS (@slots is missing)");
    }
    // The kept versions: a complete system image and a boot image to start it, newest first.
    let mut versions = Vec::new();
    for e in fs::read_dir(&slots)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(v) = name.strip_prefix("usr_").and_then(|n| n.strip_suffix(".erofs")) else { continue };
        if !common::valid_version(v) || !slots.join(format!("verity_{v}.img")).is_file() {
            continue;
        }
        if slots.join(format!("boot/asimaos_{v}.efi")).is_file()
            || (v == stick_version && o.source.join(format!("uki/asimaos_{v}.efi")).is_file())
        {
            versions.push(v.to_string());
        } else {
            out.say(&format!("Asima OS {v} has no boot image to start it with; it is left out"));
        }
    }
    // What the repair screens show before anything changes: the versions on the disk, the version
    // the settings come from (D52) and the newest snapshot of the settings and accounts.
    if o.probe {
        let mut all = versions.clone();
        all.sort_by(|a, b| if common::older(a, b) { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less });
        let mut snaps: Vec<String> = fs::read_dir(top.join("@snapshots/data"))
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        snaps.sort();
        let settings = read_file(&top.join("@var/lib/asimaos/state-version")).trim().to_string();
        out.event(json!({
            "event": "probe",
            "versions": all,
            "stick": stick_version,
            "settings": (!settings.is_empty()).then_some(settings),
            "snapshot": snaps.last(),
            "encrypted": encrypted,
        }));
        out.step("check", "done", "");
        return Ok(());
    }
    let mut versions = newest_two(versions);
    if o.install_stick {
        versions = with_stick(&versions, &stick_version);
    }
    if versions.is_empty() {
        return stop(
            "no version can be started; install this USB stick's version (--install-this-version), or reinstall",
        );
    }
    out.say(&format!("versions to start: {}", versions.join(" ")));
    out.step("check", "done", "");

    // ── 1b. this USB stick's version into the system slots ──
    if o.install_stick {
        install_stick(&o.source, &stick_version, &slots, &top, &versions, out)?;
    }

    // ── 2. the boot partition ──
    out.step("esp", "active", "Check the boot partition");
    let _ = ok("fsck.vfat", &["-a", &esp_part]);
    let esp = work.join("esp");
    if !ok("mount", &["-t", "vfat", "-o", "umask=0077", &esp_part, &esp.to_string_lossy()]) {
        if !o.yes && ask(&format!("The boot partition {esp_part} can't be repaired. Format it again (only it; your files stay)? Type YES: ")) != "YES" {
            return stop("stopped; nothing was changed");
        }
        if !ok("mkfs.vfat", &["-F", "32", "-n", "ESP", &esp_part]) {
            return stop("the boot partition could not be formatted");
        }
        if !ok("mount", &["-t", "vfat", "-o", "umask=0077", &esp_part, &esp.to_string_lossy()]) {
            return stop("the boot partition could not be opened");
        }
        out.say("the boot partition was formatted again");
    }
    out.step("esp", "done", "");

    // ── 3. boot loader, boot images, Rescue, start menu ──
    out.step("boot", "active", "Put back the start menu");
    rebuild_esp(&o.source, &esp, &slots, &versions)?;
    let menu = fs::read_dir(esp.join("loader/entries"))?
        .flatten()
        .filter_map(|e| {
            read_file(&e.path()).lines().find_map(|l| l.strip_prefix("title").map(|t| t.trim().to_string()))
        })
        .collect::<Vec<_>>();
    out.say(&format!("start menu: {}", menu.join(", ")));
    out.step("boot", "done", "");

    // ── 4. settings and accounts from the newest snapshot ──
    if o.restore_account {
        out.step("account", "active", "Bring back the settings and accounts");
        let mut snaps: Vec<String> = fs::read_dir(top.join("@snapshots/data"))
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        snaps.sort();
        let Some(newest) = snaps.last() else {
            return stop("there is no snapshot of the settings and accounts to bring back");
        };
        let aside = format!("@data.before-repair-{}", run("date", &["-u", "+%Y-%m-%d-%H%M"])?.trim());
        fs::rename(top.join("@data"), top.join(&aside))?;
        let from = top.join("@snapshots/data").join(newest);
        if !ok("btrfs", &["subvolume", "snapshot", &from.to_string_lossy(), &top.join("@data").to_string_lossy()]) {
            fs::rename(top.join(&aside), top.join("@data"))?;
            return stop("the snapshot could not be brought back");
        }
        out.say(&format!(
            "settings and accounts are back as they were on {newest} (UTC); the ones from before are in {aside}"
        ));
        out.step("account", "done", "");
    }

    out.step("finish", "active", "Finish");
    // SAFETY: sync has no preconditions.
    unsafe { libc::sync() };
    out.step("finish", "done", "");
    if out.json {
        out.event(json!({ "event": "done", "version": versions[0] }));
    } else {
        println!(
            "Done. {} starts Asima OS {} again. Take out the USB stick and restart.",
            o.target, versions[0]
        );
    }
    Ok(())
}

/// Writes the USB stick's system image into @slots (room made first; temporary names until complete).
fn install_stick(src: &Path, v: &str, slots: &Path, top: &Path, keep: &[String], out: &Out) -> Result<()> {
    out.step("install", "active", &format!("Install Asima OS {v} from this USB stick"));
    let (usr, verity, uki) = (
        src.join(format!("slots/usr_{v}.erofs")),
        src.join(format!("slots/verity_{v}.img")),
        src.join(format!("uki/asimaos_{v}.efi")),
    );
    if !common::valid_version(v) || !usr.is_file() || !verity.is_file() || !uki.is_file() {
        return stop("this USB stick's own system image is missing");
    }
    // Versions that won't be in the start menu any more make room first.
    for e in fs::read_dir(slots)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(old) = name.strip_prefix("usr_").and_then(|n| n.strip_suffix(".erofs")) else { continue };
        if keep.iter().any(|k| k == old) {
            continue;
        }
        for f in [format!("usr_{old}.erofs"), format!("verity_{old}.img"), format!("boot/asimaos_{old}.efi")] {
            let _ = fs::remove_file(slots.join(f));
        }
        out.say(&format!("Asima OS {old} was removed to make room"));
    }
    let size = |p: &Path| fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let need = (size(&usr) + size(&verity)).saturating_sub(size(&slots.join(format!("usr_{v}.erofs"))));
    let free = run("btrfs", &["filesystem", "usage", "-b", &top.to_string_lossy()]).ok().and_then(|s| {
        s.lines()
            .find_map(|l| l.trim().strip_prefix("Free (estimated):")?.split_whitespace().next()?.parse::<u64>().ok())
    });
    if free.is_some_and(|f| f <= need + (1 << 30)) {
        return stop(format!(
            "the disk has too little free space for Asima OS {v} (it needs {} GB)",
            need / (1 << 30) + 1
        ));
    }
    // Stored as it is (the image is compressed already), under a temporary name until it is complete.
    let _ = ok("chattr", &["+m", &slots.to_string_lossy()]);
    for (from, name) in [(&usr, format!("usr_{v}.erofs")), (&verity, format!("verity_{v}.img"))] {
        let partial = slots.join(format!(".{name}.partial"));
        if fs::copy(from, &partial).is_err() {
            let _ = fs::remove_file(&partial);
            return stop(format!("Asima OS {v} could not be written ({name})"));
        }
        fs::File::open(&partial)?.sync_all()?;
        fs::rename(&partial, slots.join(&name))?;
    }
    fs::create_dir_all(slots.join("boot"))?;
    fs::copy(&uki, slots.join(format!("boot/asimaos_{v}.efi")))?;
    // The stick's migration steps (D52), as an update file brings them: the new version's first
    // start runs those newer than the version the settings come from.
    let steps = src.join("migrations");
    if steps.is_dir() {
        let staged = slots.join("staged").join(v).join("migrations");
        let _ = fs::remove_dir_all(&staged);
        fs::create_dir_all(&staged)?;
        for e in fs::read_dir(&steps)?.flatten().filter(|e| e.path().is_file()) {
            fs::copy(e.path(), staged.join(e.file_name()))?;
        }
    }
    out.say(&format!(
        "Asima OS {v} is installed next to {}; files and settings stay",
        keep.get(1).map_or("nothing else", |s| s.as_str())
    ));
    out.step("install", "done", "");
    Ok(())
}

/// The boot loader (with shim when the stick has it), each kept version's boot image, Rescue and
/// the start menu: the newest version current (on trial), the one before it the backup.
fn rebuild_esp(src: &Path, esp: &Path, slots: &Path, versions: &[String]) -> Result<()> {
    let _ = fs::remove_dir_all(esp.join("EFI/Asima"));
    let _ = fs::remove_dir_all(esp.join("loader"));
    for d in ["EFI/BOOT", "EFI/systemd", "EFI/Asima", "loader/entries"] {
        fs::create_dir_all(esp.join(d))?;
    }
    let shim = src.join("shim");
    if shim.join("shimx64.efi").is_file()
        && shim.join("mmx64.efi").is_file()
        && shim.join("asimaos.der").is_file()
    {
        fs::copy(shim.join("shimx64.efi"), esp.join("EFI/BOOT/BOOTX64.EFI"))?;
        fs::copy(shim.join("mmx64.efi"), esp.join("EFI/BOOT/mmx64.efi"))?;
        fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/BOOT/grubx64.efi"))?;
        fs::copy(shim.join("asimaos.der"), esp.join("EFI/BOOT/asimaos.der"))?;
    } else {
        fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/BOOT/BOOTX64.EFI"))?;
    }
    fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/systemd/systemd-bootx64.efi"))?;
    for v in versions {
        let kept = slots.join(format!("boot/asimaos_{v}.efi"));
        if kept.is_file() {
            fs::copy(&kept, esp.join(format!("EFI/Asima/asimaos_{v}.efi")))?;
        } else {
            fs::copy(src.join(format!("uki/asimaos_{v}.efi")), esp.join(format!("EFI/Asima/asimaos_{v}.efi")))?;
            let _ = fs::create_dir_all(slots.join("boot"));
            let _ = fs::copy(src.join(format!("uki/asimaos_{v}.efi")), &kept);
        }
    }
    if src.join("uki/asimaos-rescue.efi").is_file() {
        fs::copy(src.join("uki/asimaos-rescue.efi"), esp.join("EFI/Asima/asimaos-rescue.efi"))?;
    }
    let aosupd = which("aosupd").ok_or_else(|| common::Stop("aosupd is missing on this USB stick".into()))?;
    let e = esp.to_string_lossy().into_owned();
    // The older one first, marked good, so the newest becomes current (on trial) with it as backup.
    if versions.len() > 1 {
        run(&aosupd, &["boot", "promote", "--esp", &e, "--version", &versions[1]])?;
        for entry in fs::read_dir(esp.join("loader/entries"))?.flatten() {
            if entry.file_name().to_string_lossy().starts_with("asimaos-current+") {
                fs::rename(entry.path(), esp.join("loader/entries/asimaos-current.conf"))?;
            }
        }
    }
    run(&aosupd, &["boot", "promote", "--esp", &e, "--version", &versions[0]])?;
    run(&aosupd, &["boot", "rescue", "--esp", &e])?;
    let conf = run(&aosupd, &["boot", "loader-conf", "--esp", &e])?;
    fs::write(esp.join("loader/loader.conf"), conf)?;
    // SAFETY: sync has no preconditions.
    unsafe { libc::sync() };
    Ok(())
}

pub fn which(program: &str) -> Option<String> {
    std::env::var("PATH")
        .ok()?
        .split(':')
        .map(|d| Path::new(d).join(program))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_to_start() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            newest_two(v(&["2026.11.18.1", "2026.11.22.1", "2026.11.21.1"])),
            v(&["2026.11.22.1", "2026.11.21.1"])
        );
        assert_eq!(
            with_stick(&v(&["2026.11.21.1", "2026.11.18.1"]), "2026.12.01.1"),
            v(&["2026.12.01.1", "2026.11.21.1"])
        );
        assert_eq!(
            with_stick(&v(&["2026.12.01.1", "2026.11.21.1"]), "2026.12.01.1"),
            v(&["2026.12.01.1", "2026.11.21.1"])
        );
        assert_eq!(with_stick(&[], "2026.12.01.1"), v(&["2026.12.01.1"]));
    }
}
