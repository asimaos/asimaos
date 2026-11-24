//! Asima Rescue engine (`asimaos-engine rescue`, inside Rescue as `asimaos-rescue-engine`). Runs
//! entirely in RAM, as root, and never starts the installed system. Driven by the Asima Rescue GUI;
//! usable from a terminal.
//!
//! Events (one JSON object per line), like the installer engine: step {id, state} · progress
//! {value, detail} · log {message} · code {code} · recovery-key {key} · error {message} · done {message}

use crate::common::{self, ok, read_file, read_line, run, stop, Out};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const USAGE: &str = "\
asimaos-rescue-engine status --json
asimaos-rescue-engine unlock --json --secret-stdin          passphrase or recovery key on stdin
asimaos-rescue-engine check | repair | free-space | start-previous | approve-key | remove-tpm --json
asimaos-rescue-engine restore SNAPSHOT --json                e.g. @snapshots/home-2026-11-16-0300
asimaos-rescue-engine reinstall FILE --json                  a signed .aosupd update file
asimaos-rescue-engine factory-reset --json --user NAME --fullname TEXT --secrets-stdin
                                                            stdin: account password, then disk passphrase
asimaos-rescue-engine wipe --json --serial-tail XXXX         erases the whole disk, OS included (D14)";

const RUN: &str = "/run/asimaos-rescue";
const RELEASE_KEY: &str = "/usr/share/asimaos/keys/release.pub";
const LOADER: &str = "4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";

fn run_dir() -> PathBuf {
    PathBuf::from(RUN)
}
fn top() -> PathBuf {
    run_dir().join("pool") // the pool's top level (subvolid=5) once available
}
fn keyfile() -> PathBuf {
    run_dir().join("unlock.key") // RAM only, root only, for this rescue session
}

/// The Asima OS disk: the one whose ESP this rescue system was started from.
struct Disk {
    disk: String,
    esp: String,
    part: String,
    encrypted: bool,
}

fn efivar_string(name: &str, guid: &str) -> Option<String> {
    let d = fs::read(format!("/sys/firmware/efi/efivars/{name}-{guid}")).ok()?;
    let units: Vec<u16> = d.get(4..)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Some(String::from_utf16_lossy(&units).chars().filter(|c| !c.is_control()).collect())
}

fn find_disk() -> Option<Disk> {
    let uuid: String = efivar_string("LoaderDevicePartUUID", LOADER)
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    let by_uuid = Path::new("/dev/disk/by-partuuid").join(uuid.to_lowercase());
    let (disk, esp) = if !uuid.is_empty() && by_uuid.exists() {
        let esp = fs::canonicalize(&by_uuid).ok()?.to_string_lossy().into_owned();
        (common::disk_of(&esp)?, esp)
    } else {
        let label = ["crypt_pool", "asimaos_pool"]
            .into_iter()
            .map(|l| Path::new("/dev/disk/by-partlabel").join(l))
            .find(|p| p.exists())?;
        let part = fs::canonicalize(label).ok()?.to_string_lossy().into_owned();
        let disk = common::disk_of(&part)?;
        let esp = common::part_of(&disk, "ESP").unwrap_or_default();
        (disk, esp)
    };
    let (part, encrypted) = match common::part_of(&disk, "crypt_pool") {
        Some(p) => (p, true),
        None => (common::part_of(&disk, "asimaos_pool")?, false),
    };
    Some(Disk { disk, esp, part, encrypted })
}

impl Disk {
    fn pool_device(&self) -> String {
        if self.encrypted {
            "/dev/mapper/crypt_pool".into()
        } else {
            self.part.clone()
        }
    }

    fn unlocked(&self) -> bool {
        !self.encrypted || Path::new("/dev/mapper/crypt_pool").exists()
    }

    fn need_pool(&self) -> Result<()> {
        if !self.unlocked() {
            return stop("Unlock the disk first");
        }
        fs::create_dir_all(top())?;
        if !ok("mountpoint", &["-q", &top().to_string_lossy()]) {
            run("mount", &["-t", "btrfs", "-o", "subvolid=5,noatime", &self.pool_device(), &top().to_string_lossy()])?;
        }
        Ok(())
    }

    fn mount_esp(&self) -> Result<PathBuf> {
        let esp = run_dir().join("esp");
        fs::create_dir_all(&esp)?;
        if !ok("mountpoint", &["-q", &esp.to_string_lossy()]) {
            run("mount", &["-t", "vfat", &self.esp, &esp.to_string_lossy()])?;
        }
        Ok(esp)
    }

    /// The start menu (the same code as Asima Updater): "ROLE VERSION LEFT DONE ID" lines, current first.
    fn menu(&self) -> Result<Vec<Vec<String>>> {
        let esp = self.mount_esp()?;
        Ok(run("aosupd", &["boot", "list", "--plain", "--esp", &esp.to_string_lossy()])?
            .lines()
            .map(|l| l.split_whitespace().map(String::from).collect::<Vec<_>>())
            .filter(|f| f.len() >= 5)
            .collect())
    }
}

/// The version to treat as this computer's own: the current one, or the backup when the current
/// one keeps failing (no tries left). (version, entry id)
fn good_version(menu: &[Vec<String>]) -> Option<(String, String)> {
    let role = |r: &str, f: &dyn Fn(&Vec<String>) -> bool| {
        menu.iter().find(|e| e[0] == r && f(e)).map(|e| (e[1].clone(), e[4].clone()))
    };
    role("current", &|e| e[2] != "0").or_else(|| role("backup", &|_| true)).or_else(|| role("current", &|_| true))
}

/// "VERSION UNPACKED-BYTES" of an update file Asima OS signed (header and manifest checked).
fn update_info(file: &str) -> Option<(String, u64)> {
    let j: Value = serde_json::from_str(&run("aosupd", &["inspect", "--key", RELEASE_KEY, file]).ok()?).ok()?;
    let v = j["version"].as_str()?.to_string();
    let n = j["unpackedSize"].as_u64().unwrap_or(0);
    common::valid_version(&v).then_some((v, n))
}

/// Room an install needs, like Asima Updater: 1.5 × the unpacked size, at least 10 GB; a
/// reinstall of the same version replaces files that are there, so half of that.
pub fn space_needed(unpacked: u64, same: bool) -> u64 {
    let need = (unpacked * 3 / 2).max(10 << 30);
    if same {
        need / 2
    } else {
        need
    }
}

fn df(path: &Path, column: &str) -> u64 {
    run("df", &["-B1", &format!("--output={column}"), &path.to_string_lossy()])
        .ok()
        .and_then(|s| s.lines().last()?.trim().parse().ok())
        .unwrap_or(0)
}

/// Recovery keys may be typed without dashes or with spaces: put them back.
pub fn tidy_secret(s: &str) -> String {
    let s: String = s.chars().filter(|c| *c != ' ').collect();
    if s.len() == 64 && s.chars().all(|c| "cbdefghijklnrtuv".contains(c)) {
        s.as_bytes().chunks(8).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
    } else {
        s
    }
}

struct Opts {
    arg: String,
    secret_stdin: bool,
    secrets_stdin: bool,
    user: String,
    fullname: String,
    serial_tail: String,
}

pub fn main(args: &[String], out: &mut Out) -> Result<()> {
    let action = args.first().cloned().unwrap_or_default();
    let mut o = Opts {
        arg: String::new(),
        secret_stdin: false,
        secrets_stdin: false,
        user: String::new(),
        fullname: String::new(),
        serial_tail: String::new(),
    };
    let mut it = args.iter().skip(1);
    let need = |it: &mut std::iter::Skip<std::slice::Iter<String>>, name: &str| -> Result<String> {
        it.next().cloned().ok_or_else(|| common::Stop(format!("{name} needs a value")).into())
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => out.json = true,
            "--secret-stdin" => o.secret_stdin = true,
            "--secrets-stdin" => o.secrets_stdin = true,
            "--user" => o.user = need(&mut it, "--user")?,
            "--fullname" => o.fullname = need(&mut it, "--fullname")?,
            "--serial-tail" => o.serial_tail = need(&mut it, "--serial-tail")?,
            s if s.starts_with('-') => return stop(format!("unknown option: {s}")),
            s => o.arg = s.to_string(),
        }
    }
    if !matches!(
        action.as_str(),
        "status"
            | "unlock"
            | "check"
            | "repair"
            | "free-space"
            | "restore"
            | "reinstall"
            | "start-previous"
            | "approve-key"
            | "remove-tpm"
            | "factory-reset"
            | "wipe"
    ) {
        println!("{USAGE}");
        std::process::exit(2);
    }
    let disk = find_disk().ok_or_else(|| common::Stop("No Asima OS disk was found on this computer".into()))?;
    let done = |out: &Out, msg: &str| {
        if out.json {
            out.event(json!({ "event": "done", "message": msg }));
        } else {
            println!("{msg}");
        }
    };
    match action.as_str() {
        "status" => println!("{}", status(&disk)?),
        "unlock" => {
            if !disk.encrypted {
                done(out, "This disk isn't encrypted.");
                return Ok(());
            }
            if disk.unlocked() {
                done(out, "Your disk is unlocked.");
                return Ok(());
            }
            if !o.secret_stdin {
                return stop("--secret-stdin is required");
            }
            fs::create_dir_all(run_dir())?;
            let secret = tidy_secret(
                &read_line(&mut std::io::stdin().lock())
                    .ok_or_else(|| common::Stop("no passphrase on standard input".into()))?,
            );
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut f =
                    fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(keyfile())?;
                std::io::Write::write_all(&mut f, secret.as_bytes())?;
            }
            if !ok(
                "cryptsetup",
                &["open", "--type", "luks", "--key-file", &keyfile().to_string_lossy(), &disk.part, "crypt_pool"],
            ) {
                let _ = fs::remove_file(keyfile());
                return stop("That didn't unlock the disk");
            }
            done(out, "Your disk is unlocked.");
        }
        "check" => check(&disk, out, &done)?,
        "repair" => {
            disk.need_pool()?;
            out.step("repair", "active", "repair");
            // Scrub rewrites damaged blocks from Btrfs' second metadata copy: the safe repair.
            let log = fs::File::create(run_dir().join("scrub.log"))?;
            let _ = Command::new("btrfs")
                .args(["scrub", "start", "-B", &top().to_string_lossy()])
                .stdout(log.try_clone()?)
                .stderr(log)
                .status();
            out.step("repair", "done", "");
            out.progress(0.6, "");
            out.step("verify", "active", "verify");
            let _ = ok("umount", &["-q", &top().to_string_lossy()]);
            if ok("btrfs", &["check", "--readonly", &disk.pool_device()]) {
                out.step("verify", "done", "");
                done(out, "The file system is repaired.");
            } else {
                return stop(
                    "Some errors remain. Copy your files to another drive (they can still be read) and reinstall.",
                );
            }
        }
        "free-space" => {
            disk.need_pool()?;
            out.step("reserve", "active", "reserve");
            let ballast = top().join("@reserve/.emergency_ballast");
            let Ok(meta) = fs::metadata(&ballast) else { return stop("The emergency space is already in use") };
            fs::remove_file(&ballast)?;
            // SAFETY: sync has no preconditions.
            unsafe { libc::sync() };
            out.step("reserve", "done", "");
            out.progress(1.0, "");
            done(
                out,
                &format!(
                    "{} GB were freed. Asima OS has room to start; delete some files soon.",
                    meta.len() / 1_000_000_000
                ),
            );
        }
        "restore" => {
            disk.need_pool()?;
            let name = o.arg.strip_prefix("@snapshots/").unwrap_or("");
            if name.is_empty()
                || name.starts_with('.')
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
                || !top().join(&o.arg).is_dir()
            {
                return stop("That snapshot doesn't exist");
            }
            out.step("save", "active", "save");
            let stamp = run("date", &["+%Y-%m-%d-%H%M"])?.trim().to_string();
            let t = top();
            run(
                "btrfs",
                &[
                    "subvolume",
                    "snapshot",
                    "-r",
                    &t.join("@home").to_string_lossy(),
                    &t.join(format!("@snapshots/home-before-restore-{stamp}")).to_string_lossy(),
                ],
            )?;
            out.step("save", "done", "");
            out.progress(0.4, "");
            out.step("restore", "active", "restore");
            run(
                "btrfs",
                &[
                    "subvolume",
                    "snapshot",
                    &t.join(&o.arg).to_string_lossy(),
                    &t.join("@home.restoring").to_string_lossy(),
                ],
            )?;
            run("btrfs", &["subvolume", "delete", "-c", &t.join("@home").to_string_lossy()])?;
            fs::rename(t.join("@home.restoring"), t.join("@home"))?;
            // SAFETY: sync has no preconditions.
            unsafe { libc::sync() };
            out.step("restore", "done", "");
            out.progress(1.0, "");
            done(out, &format!("Your files are back as they were. Today's files are kept in the snapshot home-before-restore-{stamp}."));
        }
        "reinstall" => reinstall(&disk, &o.arg, out, &done)?,
        "start-previous" => {
            out.step("boot", "active", "boot");
            let menu = disk.menu()?;
            let Some(backup) = menu.iter().find(|e| e[0] == "backup") else {
                return stop("There is no backup version on this computer");
            };
            if !ok("bootctl", &["set-oneshot", &backup[4]]) {
                return stop("The start menu couldn't be changed");
            }
            out.step("boot", "done", "");
            out.progress(1.0, "");
            done(out, &format!("Asima OS {} (the backup) starts next, once. Restart now.", backup[1]));
        }
        "approve-key" => {
            out.step("key", "active", "key");
            let esp = disk.mount_esp()?;
            let key = esp.join("EFI/BOOT/asimaos.der");
            if !key.is_file() {
                return stop("The Asima OS key isn't on this computer's boot partition");
            }
            let code = digits(8)?;
            let hash = run("mokutil", &[&format!("--generate-hash={code}")])?;
            let hash_file = run_dir().join("mok.hash");
            fs::create_dir_all(run_dir())?;
            fs::write(&hash_file, hash.trim())?;
            let queued =
                ok("mokutil", &["--import", &key.to_string_lossy(), "--hash-file", &hash_file.to_string_lossy()]);
            let _ = fs::remove_file(&hash_file);
            if !queued {
                return stop("The key couldn't be queued for approval");
            }
            out.event(json!({ "event": "code", "code": format!("{} {}", &code[..4], &code[4..]) }));
            out.step("key", "done", "");
            out.progress(1.0, "");
            done(out, "At the next start, type this code on the blue screen to approve the Asima OS key.");
        }
        "remove-tpm" => {
            if !disk.encrypted {
                return stop("This disk isn't encrypted");
            }
            out.step("tpm", "active", "tpm");
            if !ok("systemd-cryptenroll", &["--wipe-slot=tpm2", &disk.part]) {
                return stop("Automatic unlock couldn't be removed");
            }
            out.step("tpm", "done", "");
            out.progress(1.0, "");
            done(out, "Automatic unlock is off. Set it up again in Asima Maintenance → Security.");
        }
        "factory-reset" => factory_reset(&disk, &o, out, &done)?,
        "wipe" => wipe(&disk, &o.serial_tail, out, &done)?,
        _ => unreachable!(),
    }
    Ok(())
}

/// Eight digits for the blue screen (MokManager reads the keyboard with the US layout).
fn digits(n: usize) -> Result<String> {
    let mut f = fs::File::open("/dev/urandom")?;
    let mut code = String::new();
    while code.len() < n {
        let mut b = [0u8; 16];
        std::io::Read::read_exact(&mut f, &mut b)?;
        // 250 and above are dropped so that every digit is equally likely.
        code.extend(b.iter().filter(|x| **x < 250).map(|x| char::from(b'0' + x % 10)));
    }
    code.truncate(n);
    Ok(code)
}

fn status(disk: &Disk) -> Result<String> {
    let menu = disk.menu().unwrap_or_default();
    let current = menu.iter().find(|e| e[0] == "current");
    let (version, _) = good_version(&menu).unwrap_or_default();
    // The other working version: the one "Start the backup once" starts.
    let previous = menu
        .iter()
        .find(|e| (e[0] == "current" || e[0] == "backup") && e[1] != version)
        .map(|e| e[1].clone())
        .unwrap_or_default();
    let reason = match current {
        Some(c) if c[2] == "0" => format!(
            "Asima OS {} didn't finish starting {} times in a row.",
            c[1],
            if c[3] == "-" { "3" } else { c[3].as_str() }
        ),
        _ if efivar_string("LoaderEntrySelected", LOADER).unwrap_or_default().contains("rescue") => {
            "Asima Rescue was chosen in the start menu.".into()
        }
        _ => String::new(),
    };
    let tpm = Path::new("/sys/class/tpm/tpm0").exists();
    let enrolled = disk.encrypted
        && run("systemd-cryptenroll", &[&disk.part])
            .unwrap_or_default()
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some("tpm2"));
    let (mut free, mut total, mut reserve, mut snaps) = (0, 0, 0, Vec::new());
    if disk.unlocked() {
        disk.need_pool()?;
        total = df(&top(), "size");
        free = df(&top(), "avail");
        reserve = fs::metadata(top().join("@reserve/.emergency_ballast")).map(|m| m.len()).unwrap_or(0);
        let mut names: Vec<String> = fs::read_dir(top().join("@snapshots"))
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        for n in names {
            let kind = if n.starts_with("update-") { "update" } else { "daily" };
            snaps.push(json!({ "id": format!("@snapshots/{n}"), "title": n, "kind": kind, "size": 0 }));
        }
    }
    // Update files on USB sticks (mounted read-only under RUN/usb): only ones Asima OS signed.
    let mut files = Vec::new();
    for line in run("lsblk", &["-nrpo", "NAME,RM,FSTYPE"]).unwrap_or_default().lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 || f[1] != "1" {
            continue;
        }
        let dev = f[0];
        let mnt = run_dir().join("usb").join(Path::new(dev).file_name().unwrap_or_default());
        fs::create_dir_all(&mnt)?;
        if !ok("mountpoint", &["-q", &mnt.to_string_lossy()])
            && !ok("mount", &["-o", "ro", dev, &mnt.to_string_lossy()])
        {
            continue;
        }
        let model = common::disk_of(dev).map(|d| common::lsblk_value(&d, "MODEL")).unwrap_or_default();
        for path in find_updates(&mnt, 3) {
            let p = path.to_string_lossy().into_owned();
            let Some((v, n)) = update_info(&p) else { continue };
            let older = !version.is_empty() && common::older(&v, &version);
            files.push(json!({ "path": p, "version": v, "size": fs::metadata(&path).map(|m| m.len()).unwrap_or(0), "unpacked": n, "older": older, "device": model }));
        }
    }
    let sb = fs::read("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c")
        .ok()
        .and_then(|d| d.get(4).copied())
        == Some(1);
    Ok(json!({
        "version": version,
        "previousVersion": previous,
        "reason": reason,
        "disk": { "path": disk.disk, "model": common::lsblk_value(&disk.disk, "MODEL"),
                  "size": run("lsblk", &["-dbno", "SIZE", &disk.disk]).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0),
                  "serial": common::lsblk_value(&disk.disk, "SERIAL") },
        "encrypted": disk.encrypted,
        "unlocked": disk.unlocked(),
        "secureBoot": if sb { "on" } else { "off" },
        "tpm": tpm,
        "tpmEnrolled": enrolled,
        "freeBytes": free,
        "totalBytes": total,
        "reserveBytes": reserve,
        "snapshots": snaps,
        "updateFiles": files,
    })
    .to_string())
}

/// .aosupd files up to `depth` folders down.
fn find_updates(dir: &Path, depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() && depth > 1 {
            out.extend(find_updates(&p, depth - 1));
        } else if t.is_file() && p.extension().is_some_and(|x| x == "aosupd") {
            out.push(p);
        }
    }
    out
}

fn check(disk: &Disk, out: &Out, done: &dyn Fn(&Out, &str)) -> Result<()> {
    if !disk.unlocked() {
        return stop("Unlock the disk first");
    }
    let _ = ok("umount", &["-q", &top().to_string_lossy()]);
    out.step("fs", "active", "fs");
    fs::create_dir_all(run_dir())?;
    let log = fs::File::create(run_dir().join("check.log"))?;
    let fs_ok = Command::new("btrfs")
        .args(["check", "--readonly", &disk.pool_device()])
        .stdout(log.try_clone()?)
        .stderr(log)
        .status()
        .is_ok_and(|s| s.success());
    out.step("fs", "done", "");
    out.progress(0.4, "");
    disk.need_pool()?;
    out.step("images", "active", "images");
    let esp = disk.mount_esp()?;
    let mut bad = Vec::new();
    let menu = disk.menu()?;
    for e in menu.iter().filter(|e| e[0] != "failed") {
        let v = &e[1];
        let uki = fs::read(esp.join(format!("EFI/Asima/asimaos_{v}.efi"))).unwrap_or_default();
        let hash = usr_hash(&uki);
        let good = hash.is_some_and(|h| {
            ok(
                "veritysetup",
                &[
                    "verify",
                    &top().join(format!("@slots/usr_{v}.erofs")).to_string_lossy(),
                    &top().join(format!("@slots/verity_{v}.img")).to_string_lossy(),
                    &h,
                ],
            )
        });
        if !good {
            bad.push(v.clone());
        }
    }
    out.step("images", "done", "");
    out.progress(0.75, "");
    out.step("boot", "active", "boot");
    let boot_ok = esp.join("EFI/BOOT/BOOTX64.EFI").is_file()
        && esp.join("loader/loader.conf").is_file()
        && menu.iter().any(|e| e[0] == "current");
    out.step("boot", "done", "");
    out.step("space", "active", "space");
    let free = df(&top(), "avail");
    out.step("space", "done", "");
    out.progress(1.0, "");
    let mut msg = String::new();
    msg += if fs_ok { "The file system is fine. " } else { "The file system has errors: use Repair. " };
    if bad.is_empty() {
        msg += "Both Asima OS versions are intact. ";
    } else {
        msg += &format!("Damaged version(s): {} — reinstall from a USB update file. ", bad.join(" "));
    }
    if !boot_ok {
        msg += "Boot files are missing. ";
    }
    if free < 3 << 30 {
        msg += "The disk is nearly full: freeing the emergency space lets Asima OS start again.";
    }
    done(out, msg.trim_end());
    Ok(())
}

/// The /usr verity root hash in a boot image's command line (asimaos.usrhash=…).
pub fn usr_hash(uki: &[u8]) -> Option<String> {
    let needle = b"asimaos.usrhash=";
    let at = uki.windows(needle.len()).position(|w| w == needle)? + needle.len();
    let hash: String = uki[at..].iter().take_while(|b| b.is_ascii_hexdigit()).map(|b| *b as char).collect();
    (!hash.is_empty()).then_some(hash)
}

fn reinstall(disk: &Disk, file: &str, out: &Out, done: &dyn Fn(&Out, &str)) -> Result<()> {
    disk.need_pool()?;
    if !Path::new(file).is_file() {
        return stop("The update file was not found");
    }
    let Some((v, n)) = update_info(file) else { return stop("This file is not a genuine Asima OS update") };
    // Older versions are allowed here (D35): Asima Rescue is for getting a computer working again.
    let installed = good_version(&disk.menu()?).map(|g| g.0).unwrap_or_default();
    if !installed.is_empty() && common::older(&v, &installed) {
        out.say(&format!("Asima OS {v} is older than the installed {installed}."));
    }
    let need = space_needed(n, v == installed);
    let free = df(&top(), "avail");
    if free < need {
        return stop(format!(
            "Not enough free space: {} GB needed, {} GB free. Use \"Free up space\" first, or delete files after starting Asima OS.",
            need.div_ceil(1 << 30),
            free >> 30
        ));
    }
    let esp = disk.mount_esp()?;
    // aosupd checks the signature and every part before it writes anything, then writes the way
    // Asima Updater does and makes the version "current" (three tries; the current one becomes
    // the backup). Its events are passed on; its "done" is not (the boot step follows), and its
    // error message is the precise one.
    let t = top();
    let mut child = Command::new("aosupd")
        .args(["install", "--json", "--key", RELEASE_KEY])
        .arg("--slots")
        .arg(t.join("@slots"))
        .arg("--esp")
        .arg(&esp)
        .arg("--staged")
        .arg(t.join("@slots/staged"))
        .arg(file)
        .stdout(Stdio::piped())
        .spawn()
        .context("aosupd could not be started")?;
    let (mut written, mut said) = (String::new(), false);
    for line in BufReader::new(child.stdout.take().context("no output")?).lines().map_while(Result::ok) {
        let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
        match ev["event"].as_str() {
            Some("done") => written = ev["version"].as_str().unwrap_or_default().to_string(),
            Some("error") => {
                said = true;
                if out.json {
                    println!("{line}");
                } else {
                    eprintln!("{}", ev["message"].as_str().unwrap_or_default());
                }
            }
            _ => {
                if out.json {
                    println!("{line}");
                }
            }
        }
    }
    let status = child.wait()?;
    if !status.success() {
        if said {
            std::process::exit(1);
        }
        return stop("The update could not be written");
    }
    if !common::valid_version(&written) {
        return stop("The update could not be written");
    }
    out.step("boot", "active", "boot");
    // A firmware "start once" choice from before would skip the reinstalled version.
    let _ = ok("bootctl", &["set-oneshot", ""]);
    let _ = ok("aosupd", &["boot", "rescue", "--esp", &esp.to_string_lossy()]);
    // SAFETY: sync has no preconditions.
    unsafe { libc::sync() };
    out.step("boot", "done", "");
    out.progress(1.0, "");
    done(out, &format!("Asima OS {written} is reinstalled. Restart to start it."));
    Ok(())
}

fn factory_reset(disk: &Disk, o: &Opts, out: &Out, done: &dyn Fn(&Out, &str)) -> Result<()> {
    disk.need_pool()?;
    if !common::valid_username(&o.user) {
        return stop("The username isn't valid");
    }
    if o.fullname.is_empty() || o.fullname.contains(['\n', '\r', ':']) {
        return stop("The name isn't valid");
    }
    if !o.secrets_stdin {
        return stop("--secrets-stdin is required");
    }
    let mut stdin = std::io::stdin().lock();
    let password = read_line(&mut stdin).ok_or_else(|| common::Stop("no password on standard input".into()))?;
    let passphrase = read_line(&mut stdin).unwrap_or_default();
    if password.chars().count() < 4 {
        return stop("The password needs at least 4 characters");
    }
    out.step("account", "active", "account");
    let Some((version, _)) = good_version(&disk.menu()?) else {
        return stop("No Asima OS version was found in the start menu");
    };
    let hash = common::run_input("openssl", &["passwd", "-6", "-stdin"], format!("{password}\n").as_bytes(), &[])?
        .trim()
        .to_string();
    let usr = run_dir().join("usr");
    fs::create_dir_all(&usr)?;
    if !ok("mountpoint", &["-q", &usr.to_string_lossy()]) {
        run(
            "mount",
            &[
                "-o",
                "loop,ro",
                "-t",
                "erofs",
                &top().join(format!("@slots/usr_{version}.erofs")).to_string_lossy(),
                &usr.to_string_lossy(),
            ],
        )?;
    }
    let factory = usr.join("share/factory/etc");
    let uid = crate::install::free_uid(&read_file(&factory.join("passwd")));
    let (mut keep, mut rkey) = (String::new(), String::new());
    if disk.encrypted {
        if passphrase.chars().count() < 8 {
            return stop("The disk passphrase needs at least 8 characters");
        }
        let slots = |d: &str| -> Vec<String> {
            run("systemd-cryptenroll", &[d])
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.split_whitespace().next().filter(|s| s.parse::<u32>().is_ok()).map(String::from))
                .collect()
        };
        let before = slots(&disk.part);
        let k = format!("--unlock-key-file={}", keyfile().display());
        common::run_input("systemd-cryptenroll", &[&k, "--password", &disk.part], b"", &[("NEWPASSWORD", &passphrase)])
            .map_err(|_| common::Stop("The new passphrase couldn't be added".into()))?;
        rkey = run("systemd-cryptenroll", &[&k, "--recovery-key", &disk.part])
            .ok()
            .and_then(|t| common::recovery_key(&t))
            .unwrap_or_default();
        if rkey.is_empty() {
            return stop("The new recovery key couldn't be made");
        }
        keep = slots(&disk.part).into_iter().filter(|s| !before.contains(s)).collect::<Vec<_>>().join(",");
    }
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() / 86400;
    let plan = run_dir().join("reset.plan");
    fs::write(
        &plan,
        format!(
            "USERNAME={u}\nUSER_ID={uid}\nPASSWD_LINE={u}:x:{uid}:{uid}:{f}:/home/{u}:/bin/bash\nSHADOW_LINE={u}:{hash}:{days}:0:99999:7:::\nENCRYPTED={e}\nKEEP_SLOTS={keep}\n",
            u = o.user,
            f = o.fullname,
            e = if disk.encrypted { "yes" } else { "no" }
        ),
    )?;
    out.step("account", "done", "");
    out.progress(0.2, "");
    out.step("wipe", "active", "wipe");
    let _ = ok("umount", &["-q", &top().to_string_lossy(), &run_dir().join("esp").to_string_lossy()]);
    let log = run_dir().join("reset.log");
    let logf = fs::File::create(&log)?;
    let reset = Command::new("/usr/lib/asimaos/asimaos-reset")
        .args([
            &disk.pool_device(),
            &plan.to_string_lossy().into_owned(),
            &version,
            &factory.to_string_lossy().into_owned(),
        ])
        .stdout(logf.try_clone()?)
        .stderr(logf)
        .status()?;
    if !reset.success() {
        let last = read_file(&log).lines().last().unwrap_or("").to_string();
        return stop(format!("The reset failed: {last}"));
    }
    for (id, state) in [("wipe", "done"), ("keys", "active"), ("keys", "done"), ("trim", "active"), ("trim", "done")] {
        out.step(id, state, id);
    }
    let _ = fs::remove_file(&plan);
    let _ = fs::remove_file(keyfile());
    let _ = ok("umount", &["-q", &usr.to_string_lossy()]);
    if !rkey.is_empty() {
        out.event(json!({ "event": "recovery-key", "key": rkey }));
    }
    out.progress(1.0, "");
    done(out, &format!("Asima OS {version} is fresh. Restart and sign in as {}.", o.user));
    Ok(())
}

/// The last four characters of the disk's serial number (ERASE when it has none) confirm the wipe.
pub fn wipe_word(serial: &str) -> String {
    if serial.chars().count() >= 4 {
        serial.chars().skip(serial.chars().count() - 4).collect()
    } else {
        "ERASE".into()
    }
}

fn wipe(disk: &Disk, tail: &str, out: &Out, done: &dyn Fn(&Out, &str)) -> Result<()> {
    let serial = common::lsblk_value(&disk.disk, "SERIAL");
    if tail.to_uppercase() != wipe_word(&serial).to_uppercase() {
        return stop("The serial number doesn't match this disk");
    }
    for m in [top(), run_dir().join("esp"), run_dir().join("usr")] {
        let _ = ok("umount", &["-q", &m.to_string_lossy()]);
    }
    out.step("keys", "active", "keys");
    if disk.encrypted {
        let _ = ok("cryptsetup", &["close", "crypt_pool"]);
        // Destroying every key slot makes the data unreadable at once, even with old keys.
        let _ = ok("cryptsetup", &["erase", "--batch-mode", &disk.part]);
    }
    let _ = fs::remove_file(keyfile());
    out.step("keys", "done", "");
    out.progress(0.1, "");
    out.step("discard", "active", "discard");
    out.progress(0.15, &format!("Erasing {}", disk.disk));
    let _ = ok("wipefs", &["-a", &disk.disk]);
    if !ok("blkdiscard", &["--secure", "--force", &disk.disk]) && !ok("blkdiscard", &["--force", &disk.disk]) {
        // No discard support: overwrite with zeros (slow on large disks).
        let _ = ok(
            "dd",
            &["if=/dev/zero", &format!("of={}", disk.disk), "bs=16M", "oflag=direct", "status=none", "conv=fsync"],
        );
    }
    // SAFETY: sync has no preconditions.
    unsafe { libc::sync() };
    out.step("discard", "done", "");
    out.progress(1.0, "");
    done(out, "Nothing is left on this disk. To use the computer again, install an operating system from a USB stick.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescue_helpers() {
        let menu = vec![
            "current 2026.12.01.1 0 3 asimaos-current+0-3".split(' ').map(String::from).collect::<Vec<_>>(),
            "backup 2026.11.22.1 - - asimaos-backup".split(' ').map(String::from).collect(),
        ];
        assert_eq!(good_version(&menu).unwrap().0, "2026.11.22.1"); // the current one keeps failing
        let fine = vec!["current 2026.12.01.1 2 1 asimaos-current+2-1".split(' ').map(String::from).collect::<Vec<_>>()];
        assert_eq!(good_version(&fine).unwrap().0, "2026.12.01.1");
        assert_eq!(space_needed(4 << 30, false), 10 << 30);
        assert_eq!(space_needed(10 << 30, false), 15 << 30);
        assert_eq!(space_needed(10 << 30, true), (15 << 30) / 2);
        assert_eq!(
            tidy_secret("fkcrtjbg ndhvhjrl bucdbhit tirlukce hkjgrdlg kbtnbilh djvbrkfu ihfcehcn"),
            "fkcrtjbg-ndhvhjrl-bucdbhit-tirlukce-hkjgrdlg-kbtnbilh-djvbrkfu-ihfcehcn"
        );
        assert_eq!(tidy_secret("my pass phrase"), "mypassphrase");
        assert_eq!(usr_hash(b"...root=x asimaos.usrhash=ab12ef rw..."), Some("ab12ef".into()));
        assert_eq!(wipe_word("S4EVNF0M123456"), "3456");
        assert_eq!(wipe_word("AB"), "ERASE");
    }
}
