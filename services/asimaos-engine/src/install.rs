//! Asima OS Installer engine (`asimaos-engine install`, on the ISO as `asimaos-install`).
//! Used from a terminal, or by the graphical Asima OS Installer (--json).
//!
//!   asimaos-install                          list disks and options
//!   asimaos-install /dev/DISK [options]      install Asima OS on the WHOLE disk
//!
//! Writes: 7 GB ESP + Btrfs pool with 11 subvolumes, from the same systemd-repart definitions the
//! image builder uses. Encrypted (default): the pool is LUKS2 (partition label crypt_pool) and a
//! recovery key is enrolled and shown once, never stored. Not encrypted: plain Btrfs (label
//! asimaos_pool). Automatic unlock with the TPM and Secure Boot are set up later in Asima Maintenance.
//!
//! JSON events: step {id: check, prepare, write, recovery (encrypting only), finish}, progress,
//! recovery-key {key}, log, error, done {version}.

use crate::common::{self, ask, ask_secret, ok, read_file, read_line, run, run_input, stop, Out};
use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const USAGE: &str = "\
asimaos-install                          list disks and options
asimaos-install /dev/DISK [options]      install Asima OS on the WHOLE disk

Options:
  --yes                      don't ask to type ERASE (the graphical installer confirms itself)
  --json                     machine-readable progress on stdout, one JSON object per line
  --encrypt yes|no           encrypt the disk (default yes; it can be skipped, decision D9)
  --passphrase-stdin         read the disk passphrase from standard input (needs --encrypt yes)
  --user NAME                create this account; it is the computer's administrator
  --fullname TEXT            the account's display name, e.g. \"Asha Roy\"
  --password-stdin           read the account password from standard input
  --hostname NAME            the computer's name on the network, e.g. asha-laptop (default asimaos)
  --locale ID                language, e.g. en_IN.UTF-8                 (default C.UTF-8)
  --keymap LAYOUT[:VARIANT]  keyboard layout (XKB), e.g. us, de, bd:probhat  (default us)
  --timezone ZONE            e.g. Asia/Kolkata                           (default UTC)
  --source DIR               installer files                 (default /run/initramfs/live/asimaos)
  --dry-run                  run every check and simulate the writes; no disk is touched

Secrets never go on the command line. On standard input, one per line, in this order: the disk
passphrase (with --passphrase-stdin), then the account password (with --password-stdin).
Without them the engine asks on the terminal.";

const MIN_BYTES: u64 = 48 * 1024 * 1024 * 1024; // two versions side by side plus an update
const MIN_PASSPHRASE: usize = 8;
const MIN_PASSWORD: usize = 4;

#[derive(Default)]
struct Opts {
    target: String,
    yes: bool,
    pass_stdin: bool,
    pw_stdin: bool,
    dry_run: bool,
    encrypt: bool,
    locale: String,
    keymap: String,
    timezone: String,
    user: String,
    fullname: String,
    hostname: String,
    source: PathBuf,
}

/// The console keymap (before the desktop starts) for an XKB layout.
pub fn console_keymap(layout: &str) -> String {
    match layout {
        "gb" => "uk",
        "de" => "de-latin1",
        "fr" => "fr-latin1",
        "es" | "it" | "ru" | "ua" | "fi" | "hu" | "nl" => layout,
        "pt" => "pt-latin1",
        "br" => "br-abnt2",
        "pl" => "pl2",
        "se" => "sv-latin1",
        "no" => "no-latin1",
        "dk" => "dk-latin1",
        "tr" => "trq",
        "cz" => "cz-qwertz",
        "ch" => "de_CH-latin1",
        "jp" => "jp106",
        _ => "us",
    }
    .to_string()
}

fn all(s: &str, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(ok)
}

pub fn valid_locale(s: &str) -> bool {
    all(s, |c| c.is_ascii_alphanumeric() || "_.@-".contains(c))
}

pub fn valid_keymap(s: &str) -> bool {
    let (layout, variant) = s.split_once(':').unwrap_or((s, ""));
    all(layout, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-".contains(c))
        && (!s.contains(':') || all(variant, |c| c.is_ascii_alphanumeric() || "_-".contains(c)))
}

pub fn valid_timezone(s: &str) -> bool {
    !s.is_empty() && s.split('/').all(|p| all(p, |c| c.is_ascii_alphanumeric() || "_+-".contains(c)))
}

pub fn valid_hostname(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=63).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The first free user id from 1000 in a passwd file.
pub fn free_uid(passwd: &str) -> u32 {
    let used: Vec<u32> = passwd.lines().filter_map(|l| l.split(':').nth(2)?.parse().ok()).collect();
    (1000..).find(|u| !used.contains(u)).unwrap()
}

/// A group file with `user` added to the members of `group`.
pub fn add_member(groups: &str, group: &str, user: &str) -> String {
    groups
        .lines()
        .map(|l| {
            let mut f: Vec<String> = l.split(':').map(String::from).collect();
            if f.len() >= 4 && f[0] == group {
                f[3] = if f[3].is_empty() { user.to_string() } else { format!("{},{user}", f[3]) };
            }
            f.join(":")
        })
        .map(|l| l + "\n")
        .collect()
}

fn parse(args: &[String], out: &mut Out) -> Result<Option<Opts>> {
    let mut o = Opts {
        encrypt: true,
        locale: "C.UTF-8".into(),
        keymap: "us".into(),
        timezone: "UTC".into(),
        source: PathBuf::from(
            std::env::var("ASIMAOS_INSTALL_SOURCE").unwrap_or_else(|_| "/run/initramfs/live/asimaos".into()),
        ),
        ..Default::default()
    };
    let mut it = args.iter();
    fn value(name: &str, it: &mut std::slice::Iter<String>) -> Result<String> {
        it.next().cloned().ok_or_else(|| common::Stop(format!("{name} needs a value")).into())
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yes" => o.yes = true,
            "--json" => out.json = true,
            "--passphrase-stdin" => o.pass_stdin = true,
            "--password-stdin" => o.pw_stdin = true,
            "--encrypt" => {
                o.encrypt = match value("--encrypt", &mut it)?.as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return stop("--encrypt must be yes or no"),
                }
            }
            "--user" => o.user = value("--user", &mut it)?,
            "--fullname" => o.fullname = value("--fullname", &mut it)?,
            "--hostname" => o.hostname = value("--hostname", &mut it)?,
            "--dry-run" => o.dry_run = true,
            "--locale" => o.locale = value("--locale", &mut it)?,
            "--keymap" => o.keymap = value("--keymap", &mut it)?,
            "--timezone" => o.timezone = value("--timezone", &mut it)?,
            "--source" => o.source = PathBuf::from(value("--source", &mut it)?),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            s if s.starts_with('-') => return stop(format!("unknown option: {s}")),
            s => o.target = s.to_string(),
        }
    }
    Ok(Some(o))
}

pub fn main(args: &[String], out: &mut Out) -> Result<()> {
    let Some(o) = parse(args, out)? else { return Ok(()) };
    if o.target.is_empty() {
        println!("{USAGE}\n");
        let _ = std::process::Command::new("lsblk")
            .args(["-dpo", "NAME,SIZE,MODEL,SERIAL,TRAN,TYPE", "-e", "7,11"])
            .status();
        std::process::exit(2);
    }
    // Values end up in the new system's configuration: only well-formed ones.
    if !valid_locale(&o.locale) {
        return stop(format!("invalid --locale: {}", o.locale));
    }
    if !valid_keymap(&o.keymap) {
        return stop(format!("invalid --keymap: {}", o.keymap));
    }
    if !valid_timezone(&o.timezone) || !Path::new("/usr/share/zoneinfo").join(&o.timezone).exists() {
        return stop(format!("unknown --timezone: {}", o.timezone));
    }
    if !o.hostname.is_empty() && !valid_hostname(&o.hostname) {
        return stop("invalid --hostname: lowercase letters, digits and -, not at the start or end");
    }
    let mut factory =
        PathBuf::from(std::env::var("ASIMAOS_INSTALL_FACTORY_ETC").unwrap_or_else(|_| "/usr/share/factory/etc".into()));
    // A dry run on a computer that isn't running Asima OS works with that computer's /etc.
    if o.dry_run && !factory.join("passwd").is_file() {
        factory = PathBuf::from("/etc");
    }
    if o.pass_stdin && !o.encrypt {
        return stop("--passphrase-stdin needs --encrypt yes");
    }
    if !o.user.is_empty() {
        if !common::valid_username(&o.user) {
            return stop("invalid --user: lowercase letters, digits, - and _, starting with a letter (at most 32)");
        }
        if o.fullname.contains(['\n', '\r', ':']) || o.fullname.chars().count() > 64 {
            return stop("invalid --fullname");
        }
        let taken = |f: &str| read_file(&factory.join(f)).lines().any(|l| l.split(':').next() == Some(o.user.as_str()));
        if taken("passwd") || taken("group") {
            return stop(format!("the name {} is used by the system; choose another", o.user));
        }
    } else if o.pw_stdin {
        return stop("--password-stdin needs --user");
    }
    // SAFETY: geteuid has no preconditions.
    let root = unsafe { libc::geteuid() } == 0;
    if !root && !o.dry_run {
        return stop("must run as root");
    }
    if !o.source.join("slots").is_dir() || !o.source.join("VERSION").is_file() {
        return stop(format!("Asima OS installer files not found in {}", o.source.display()));
    }
    let version = read_file(&o.source.join("VERSION")).trim().to_string();
    let work = if o.dry_run {
        let w = std::env::temp_dir().join(format!("asimaos-install.{}", std::process::id()));
        let _ = fs::remove_dir_all(&w);
        w
    } else {
        PathBuf::from("/run/asimaos-install")
    };

    // ── 1. check the disk ──
    out.step("check", "active", "Check the disk");
    let size = if o.dry_run && Path::new(&o.target).is_file() {
        fs::metadata(&o.target)?.len() // a plain file stands in for a disk in tests
    } else {
        check_disk(&o.target)?
    };
    if size < MIN_BYTES {
        return stop(format!("{} is too small: Asima OS needs at least 48 GB", o.target));
    }
    out.step("check", "done", "");
    out.progress(0.03, "");

    if !o.yes {
        println!("Asima OS {version} will be installed on:");
        let _ = std::process::Command::new("lsblk").args(["-dpo", "NAME,SIZE,MODEL,SERIAL", &o.target]).status();
        println!("\nALL DATA ON THIS DISK WILL BE DESTROYED.");
        if ask("Type ERASE to continue: ") != "ERASE" {
            return stop("aborted");
        }
    }

    let mut stdin = std::io::stdin().lock();
    let mut pass = String::new();
    if o.encrypt {
        pass = if o.pass_stdin {
            read_line(&mut stdin).ok_or_else(|| common::Stop("no passphrase on standard input".into()))?
        } else if let Ok(p) = std::env::var("ASIMAOS_INSTALL_PASSPHRASE") {
            p
        } else {
            let a = ask_secret("Disk encryption passphrase: ")?;
            if a != ask_secret("Repeat passphrase: ")? {
                return stop("the passphrases do not match");
            }
            a
        };
        if pass.chars().count() < MIN_PASSPHRASE {
            return stop(format!("the passphrase is too short (at least {MIN_PASSPHRASE} characters)"));
        }
    }
    let mut password = String::new();
    if !o.user.is_empty() {
        password = if o.pw_stdin {
            read_line(&mut stdin).ok_or_else(|| common::Stop("no account password on standard input".into()))?
        } else if let Ok(p) = std::env::var("ASIMAOS_INSTALL_PASSWORD") {
            p
        } else {
            let a = ask_secret(&format!("Password for {}: ", o.user))?;
            if a != ask_secret("Repeat password: ")? {
                return stop("the passwords do not match");
            }
            a
        };
        if password.chars().count() < MIN_PASSWORD {
            return stop(format!("the password is too short (at least {MIN_PASSWORD} characters)"));
        }
    }
    drop(stdin);

    // ── 2. prepare ──
    out.step("prepare", "active", "Prepare Asima OS");
    let _cleanup =
        Cleanup { work: work.clone(), keep: std::env::var("ASIMAOS_INSTALL_KEEP_WORK").is_ok(), json: out.json };
    let _ = fs::remove_dir_all(&work);
    for d in [
        "esp/EFI/BOOT",
        "esp/EFI/systemd",
        "esp/EFI/Asima",
        "esp/loader/entries",
        "pool/@slots",
        "pool/@data/overlay/etc/work",
        "pool/@data/flags",
        "pool/@home",
    ] {
        fs::create_dir_all(work.join(d))?;
    }
    stage_esp(&o.source, &work.join("esp"), &version)?;
    let upper = work.join("pool/@data/overlay/etc/upper");
    write_settings(&o, &upper, &version, o.dry_run)?;
    if o.user.is_empty() {
        out.say("no account requested (--user): only the factory accounts exist");
    } else {
        let uid = add_account(&o, &factory, &upper, &work.join("pool/@home"), &password, root)?;
        out.say(&format!("account {} (uid {uid}) prepared", o.user));
    }
    password.clear();

    // Without encryption the pool is plain Btrfs under its own label (D9); otherwise the same
    // definitions the image builder uses.
    let mut definitions = o.source.join("repart.d");
    if !o.encrypt {
        definitions = work.join("repart-plain.d");
        fs::create_dir_all(&definitions)?;
        for e in fs::read_dir(o.source.join("repart.d"))?.flatten() {
            if e.path().extension().is_some_and(|x| x == "conf") {
                fs::copy(e.path(), definitions.join(e.file_name()))?;
            }
        }
        let pool = definitions.join("10-pool.conf");
        let text = plain_pool(&read_file(&pool));
        if !text.lines().any(|l| l == "Label=asimaos_pool") {
            return stop("unexpected pool definition");
        }
        fs::write(&pool, text)?;
    }
    if !o.dry_run {
        let (s, d) = (o.source.join("slots"), work.join("pool/@slots"));
        run("mount", &["--bind", &s.to_string_lossy(), &d.to_string_lossy()])
            .map_err(|e| common::Stop(e.to_string()))?;
    }
    let keyfile = work.join("luks.key");
    if o.encrypt {
        fs::write(&keyfile, pass.as_bytes())?;
        fs::set_permissions(&keyfile, fs::Permissions::from_mode(0o600))?;
    }
    pass.clear();
    out.step("prepare", "done", "");
    out.progress(0.08, "Ready to write");

    // ── 3. write ──
    let title =
        if o.encrypt { "Partition, encrypt and copy Asima OS" } else { "Partition and copy Asima OS" };
    out.step("write", "active", title);
    let payload = du(&o.source.join("slots"))
        + fs::metadata(o.source.join(format!("uki/asimaos_{version}.efi"))).map(|m| m.len()).unwrap_or(0);
    let expected = payload + payload / 20 + 64 * 1024 * 1024;
    let report = |written: u64| {
        let f = (written as f64 / expected as f64).min(1.0);
        out.progress(
            0.08 + f * 0.82,
            &format!("Copying Asima OS · {:.1} of {:.1} GB", written as f64 / 1e9, expected as f64 / 1e9),
        );
    };
    if o.dry_run {
        for i in 1..=24u64 {
            std::thread::sleep(Duration::from_millis(100));
            report(expected * i / 24);
        }
    } else {
        write_disk(&o, &work, &definitions, &keyfile, out, report)?;
        keep_boot_image(&o, &work, &keyfile, &version, out);
    }
    if !out.json {
        println!();
    }
    out.step("write", "done", "");
    out.progress(0.90, "Asima OS is on the disk");

    // ── 4. recovery key ──
    if o.encrypt {
        out.step("recovery", "active", "Create your recovery key");
        let key = if o.dry_run {
            "dddddddd-rrrrrrrr-uuuuuuuu-nnnnnnnn-kkkkkkkk-eeeeeeee-tttttttt-ccbbccbb".to_string()
        // not a real key
        } else {
            let _ = ok("udevadm", &["settle"]);
            let luks = common::part_of(&o.target, "crypt_pool")
                .ok_or_else(|| common::Stop("the encrypted partition was not found after writing".into()))?;
            let k = keyfile.to_string_lossy().into_owned();
            let text = run("systemd-cryptenroll", &[&format!("--unlock-key-file={k}"), "--recovery-key", &luks])
                .map_err(|_| common::Stop("the recovery key could not be created".into()))?;
            common::recovery_key(&text).ok_or_else(|| common::Stop("the recovery key could not be read".into()))?
        };
        if out.json {
            out.event(json!({ "event": "recovery-key", "key": key }));
        } else {
            println!(
                "\nYour recovery key (write it down; it unlocks the disk if you forget the passphrase):\n\n    {key}\n"
            );
        }
        out.step("recovery", "done", "");
    }
    out.progress(0.97, "");

    // ── 5. finish ──
    out.step("finish", "active", "Finish");
    // SAFETY: sync has no preconditions.
    unsafe { libc::sync() };
    out.step("finish", "done", "");
    out.progress(1.0, "");
    if out.json {
        out.event(json!({ "event": "done", "version": version }));
    } else {
        println!(
            "Done. Asima OS {version} is installed on {}{}.",
            o.target,
            if o.dry_run { " (dry run: nothing was written)" } else { "" }
        );
        println!("Power off, remove the USB stick, and start the computer.");
    }
    Ok(())
}

/// The staged files go when the engine ends, however it ends.
struct Cleanup {
    work: PathBuf,
    keep: bool,
    json: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = ok("umount", &["-q", &self.work.join("pool/@slots").to_string_lossy()]);
        let _ = fs::remove_file(self.work.join("luks.key"));
        if self.keep {
            let out = Out { json: self.json, fancy: true };
            out.say(&format!("staged files kept in {}", self.work.display()));
        } else {
            let _ = fs::remove_dir_all(&self.work);
        }
    }
}

/// A real disk: whole, not the medium Asima OS runs from, nothing mounted. Its size.
fn check_disk(target: &str) -> Result<u64> {
    let meta = fs::metadata(target).map_err(|_| common::Stop(format!("{target} is not a disk")))?;
    use std::os::unix::fs::FileTypeExt;
    if !meta.file_type().is_block_device() {
        return stop(format!("{target} is not a disk"));
    }
    if common::lsblk_value(target, "TYPE") != "disk" {
        return stop(format!("{target} is not a whole disk"));
    }
    let live = run("findmnt", &["-no", "SOURCE", "/run/initramfs/live"]).unwrap_or_default().trim().to_string();
    if !live.is_empty() {
        let disk = common::disk_of(&live).unwrap_or_else(|| live.clone());
        let real = |p: &str| fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
        if [disk, live].iter().any(|m| real(m) == real(target)) {
            return stop(format!("{target} is the USB stick Asima OS is running from"));
        }
    }
    if run("lsblk", &["-nro", "MOUNTPOINTS", target]).unwrap_or_default().lines().any(|l| !l.trim().is_empty()) {
        return stop(format!("{target} is in use: something on it is mounted"));
    }
    run("lsblk", &["-dbno", "SIZE", target])?.trim().parse().context("the disk's size could not be read")
}

/// The boot partition's files: shim (when the ISO carries it) or systemd-boot, the boot image,
/// Asima Rescue and the start menu (the same format as Asima Updater's).
fn stage_esp(src: &Path, esp: &Path, version: &str) -> Result<()> {
    let shim = src.join("shim");
    if shim.join("shimx64.efi").is_file()
        && shim.join("mmx64.efi").is_file()
        && shim.join("asimaos.der").is_file()
    {
        // Secure Boot with the PC's Microsoft keys: Microsoft-signed shim, then systemd-boot signed
        // with the Asima OS key, whose certificate stays on the ESP for the blue screen.
        fs::copy(shim.join("shimx64.efi"), esp.join("EFI/BOOT/BOOTX64.EFI"))?;
        fs::copy(shim.join("mmx64.efi"), esp.join("EFI/BOOT/mmx64.efi"))?;
        fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/BOOT/grubx64.efi"))?;
        fs::copy(shim.join("asimaos.der"), esp.join("EFI/BOOT/asimaos.der"))?;
    } else {
        fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/BOOT/BOOTX64.EFI"))?;
    }
    fs::copy(src.join("systemd-bootx64.efi"), esp.join("EFI/systemd/systemd-bootx64.efi"))?;
    fs::copy(src.join(format!("uki/asimaos_{version}.efi")), esp.join(format!("EFI/Asima/asimaos_{version}.efi")))?;
    fs::write(
        esp.join("loader/entries/asimaos-current+3-0.conf"),
        format!("# Asima OS start menu entry, kept up to date by Asima Updater.\ntitle    Asima OS {version} (current)\nversion  {version}\nsort-key asimaos-1\nuki      /EFI/Asima/asimaos_{version}.efi\n"),
    )?;
    if src.join("uki/asimaos-rescue.efi").is_file() {
        fs::copy(src.join("uki/asimaos-rescue.efi"), esp.join("EFI/Asima/asimaos-rescue.efi"))?;
        fs::write(
            esp.join("loader/entries/asimaos-rescue.conf"),
            "# Asima OS start menu entry.\ntitle    Asima Rescue\nsort-key asimaos-8\nuki      /EFI/Asima/asimaos-rescue.efi\n",
        )?;
    }
    fs::write(
        esp.join("loader/loader.conf"),
        "# Asima OS: the current version on top starts by itself; a version that keeps failing\n# falls back to the backup (entries without tries left sort last).\ndefault asimaos-*\ntimeout 3\neditor no\nconsole-mode keep\n",
    )?;
    Ok(())
}

/// Language, keyboard, time zone, the computer's name and install.conf, in the new /etc overlay.
fn write_settings(o: &Opts, upper: &Path, version: &str, dry_run: bool) -> Result<()> {
    fs::create_dir_all(upper.join("asimaos"))?;
    fs::write(upper.join("locale.conf"), format!("LANG={}\n", o.locale))?;
    let (layout, variant) = o.keymap.split_once(':').unwrap_or((&o.keymap, ""));
    fs::write(
        upper.join("vconsole.conf"),
        format!("KEYMAP={}\nXKBLAYOUT={layout}\nXKBVARIANT={variant}\n", console_keymap(layout)),
    )?;
    // The same layout for the start-up passphrase, from the very first start (EFI variable).
    if !dry_run && Path::new("/usr/lib/asimaos/asimaos-keyboard").is_file() {
        let _ = std::process::Command::new("/usr/lib/asimaos/asimaos-keyboard")
            .arg("--sync")
            .env("ASIMAOS_KEYBOARD_CONF", upper.join("vconsole.conf"))
            .status();
    }
    std::os::unix::fs::symlink(format!("../usr/share/zoneinfo/{}", o.timezone), upper.join("localtime"))?;
    if !o.hostname.is_empty() {
        fs::write(upper.join("hostname"), format!("{}\n", o.hostname))?;
    }
    fs::write(
        upper.join("asimaos/install.conf"),
        format!(
            "# Written by the Asima OS Installer.\nINSTALLED_VERSION={version}\nENCRYPTED={}\n",
            if o.encrypt { "yes" } else { "no" }
        ),
    )?;
    Ok(())
}

/// The account: the factory user databases plus this user, an administrator (group wheel); root
/// stays locked. Its home starts from /etc/skel. Returns the user id.
fn add_account(o: &Opts, factory: &Path, upper: &Path, homes: &Path, password: &str, root: bool) -> Result<u32> {
    let passwd = read_file(&factory.join("passwd"));
    let uid = free_uid(&passwd);
    let groups = read_file(&factory.join("group"));
    if !groups.lines().any(|l| l.starts_with("wheel:")) {
        return stop("the system has no administrators group (wheel)");
    }
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() / 86400;
    let hash = run_input("openssl", &["passwd", "-6", "-stdin"], format!("{password}\n").as_bytes(), &[])
        .map_err(|_| common::Stop("the password could not be protected".into()))?;
    let hash = hash.trim();
    let user = &o.user;
    let line = |l: &str| format!("{l}\n");
    fs::write(
        upper.join("passwd"),
        format!(
            "{}{}",
            ensure_nl(&passwd),
            line(&format!("{user}:x:{uid}:{uid}:{}:/home/{user}:/bin/bash", o.fullname))
        ),
    )?;
    fs::write(
        upper.join("group"),
        format!("{}{}", add_member(&groups, "wheel", user), line(&format!("{user}:x:{uid}:"))),
    )?;
    // Root-only files; a dry run as a normal user simulates them from nothing.
    let shadow = read_file(&factory.join("shadow"))
        .lines()
        .map(|l| {
            let mut f: Vec<&str> = l.split(':').collect();
            if f.first() == Some(&"root") && f.len() > 1 {
                f[1] = "!*";
            }
            f.join(":") + "\n"
        })
        .collect::<String>();
    write_private(&upper.join("shadow"), &format!("{shadow}{user}:{hash}:{days}:0:99999:7:::\n"))?;
    let gshadow = add_member(&read_file(&factory.join("gshadow")), "wheel", user);
    write_private(&upper.join("gshadow"), &format!("{gshadow}{user}:!::\n"))?;
    for ids in ["subuid", "subgid"] {
        // rootless containers (Podman, Distrobox)
        fs::write(upper.join(ids), format!("{}{user}:100000:65536\n", ensure_nl(&read_file(&factory.join(ids)))))?;
    }
    let home = homes.join(user);
    fs::create_dir_all(&home)?;
    if factory.join("skel").is_dir() {
        let _ = run("cp", &["-a", &format!("{}/.", factory.join("skel").display()), &home.to_string_lossy()]);
    }
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    if root {
        run("chown", &["-R", &format!("{uid}:{uid}"), &home.to_string_lossy()])?;
    }
    Ok(uid)
}

fn ensure_nl(s: &str) -> String {
    if s.is_empty() || s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{s}\n")
    }
}

fn write_private(p: &Path, text: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?;
    std::io::Write::write_all(&mut f, text.as_bytes())?;
    Ok(())
}

/// The pool definition without encryption: plain Btrfs labelled asimaos_pool.
pub fn plain_pool(conf: &str) -> String {
    conf.lines()
        .filter(|l| !l.starts_with("Encrypt="))
        .map(|l| {
            if l == "Label=crypt_pool" {
                "Label=asimaos_pool".to_string()
            } else {
                l.replace("LUKS2 (default) -> ", "")
            }
        })
        .map(|l| l + "\n")
        .collect()
}

fn du(dir: &Path) -> u64 {
    run("du", &["-bcL", &dir.to_string_lossy()])
        .ok()
        .and_then(|s| s.lines().last()?.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// systemd-repart partitions, encrypts and copies; progress from the disk's sectors written.
fn write_disk(
    o: &Opts,
    work: &Path,
    definitions: &Path,
    keyfile: &Path,
    out: &Out,
    report: impl Fn(u64),
) -> Result<()> {
    let dev = fs::canonicalize(&o.target)?;
    let stat = PathBuf::from(format!("/sys/class/block/{}/stat", dev.file_name().unwrap().to_string_lossy()));
    let sectors = || read_file(&stat).split_whitespace().nth(6).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let base = sectors();
    let log = fs::File::create(work.join("repart.log"))?;
    let mut cmd = std::process::Command::new("systemd-repart");
    cmd.args(["--dry-run=no", "--empty=force"])
        .arg(format!("--definitions={}", definitions.display()))
        .arg(format!("--copy-source={}", work.display()));
    if o.encrypt {
        cmd.arg(format!("--key-file={}", keyfile.display()));
    }
    let mut child = cmd
        .arg(&o.target)
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .context("systemd-repart could not be started")?;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        report(sectors().saturating_sub(base) * 512);
        std::thread::sleep(Duration::from_secs(1));
    };
    if !status.success() {
        let text = read_file(&work.join("repart.log"));
        let lines: Vec<&str> = text.lines().collect();
        out.say(&lines[lines.len().saturating_sub(25)..].join("\n"));
        return stop("writing the disk failed");
    }
    Ok(())
}

/// A copy of the boot image in the pool (@slots/boot), so "Repair this computer" can rebuild the ESP.
fn keep_boot_image(o: &Opts, work: &Path, keyfile: &Path, version: &str, out: &Out) {
    let _ = ok("udevadm", &["settle"]);
    let Some(part) = common::part_of(&o.target, "crypt_pool").or_else(|| common::part_of(&o.target, "asimaos_pool"))
    else {
        return;
    };
    let mut dev = part.clone();
    if o.encrypt && ok("cryptsetup", &["open", "--key-file", &keyfile.to_string_lossy(), &part, "asimaos-install-pool"])
    {
        dev = "/dev/mapper/asimaos-install-pool".into();
    }
    let top = work.join("top");
    let _ = fs::create_dir_all(&top);
    if ok("mount", &["-t", "btrfs", "-o", "subvolid=5", &dev, &top.to_string_lossy()]) {
        let boot = top.join("@slots/boot");
        let _ = fs::create_dir_all(&boot);
        let _ = ok("chattr", &["+m", &top.join("@slots").to_string_lossy(), &boot.to_string_lossy()]);
        if fs::copy(o.source.join(format!("uki/asimaos_{version}.efi")), boot.join(format!("asimaos_{version}.efi")))
            .is_err()
        {
            out.say("the boot image copy could not be kept");
        }
        let _ = ok("umount", &[&top.to_string_lossy()]);
    } else {
        out.say("the new pool could not be opened for the boot image copy");
    }
    if dev == "/dev/mapper/asimaos-install-pool" {
        let _ = ok("cryptsetup", &["close", "asimaos-install-pool"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_and_accounts() {
        assert!(valid_locale("en_IN.UTF-8") && valid_locale("sr_RS@latin") && !valid_locale("en IN"));
        assert!(
            valid_keymap("us")
                && valid_keymap("bd:probhat")
                && valid_keymap("in:eng")
                && !valid_keymap("US")
                && !valid_keymap("bd:")
        );
        assert!(
            valid_timezone("Asia/Kolkata")
                && valid_timezone("UTC")
                && !valid_timezone("../etc")
                && !valid_timezone("Asia//X")
        );
        assert!(
            valid_hostname("asha-laptop") && !valid_hostname("-x") && !valid_hostname("Asha") && !valid_hostname("")
        );
        assert_eq!(console_keymap("de"), "de-latin1");
        assert_eq!(console_keymap("bd"), "us");
        assert_eq!(free_uid("root:x:0:0::/root:/bin/bash\nasha:x:1000:1000::/home/asha:/bin/bash\n"), 1001);
        assert_eq!(add_member("wheel:x:998:\nusers:x:100:\n", "wheel", "asha"), "wheel:x:998:asha\nusers:x:100:\n");
        assert_eq!(add_member("wheel:x:998:ravi\n", "wheel", "asha"), "wheel:x:998:ravi,asha\n");
        let pool = "[Partition]\nType=linux-generic\nLabel=crypt_pool\nEncrypt=key-file\n# LUKS2 (default) -> Btrfs\n";
        assert_eq!(plain_pool(pool), "[Partition]\nType=linux-generic\nLabel=asimaos_pool\n# Btrfs\n");
    }
}
