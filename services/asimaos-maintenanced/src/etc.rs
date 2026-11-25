//! Restoring /etc files to their factory version (Asima Maintenance → System changes): one file,
//! or every setting at once ("Reset settings").
//!
//! The overlay's upper directory must never be changed while /etc is mounted, so a
//! restore is only *scheduled* here: the path goes into @data/flags/restore-etc (one file) or
//! @data/flags/reset-settings-etc (the reset) and the initramfs removes the upper copies at the next
//! start, before /etc is mounted. For the reset, @data/flags/reset-settings names the person whose
//! own settings follow before the login screen (asimaos-reset-settings.service).

use anyhow::{bail, ensure, Context, Result};
use asimaos_svc::{cmd, Root};
use std::io::Write;
use std::path::Path;

pub const UPPER: &str = "/run/initramfs/data/overlay/etc/upper";
pub const LIST: &str = "/run/initramfs/data/flags/restore-etc";
pub const RESET_LIST: &str = "/run/initramfs/data/flags/reset-settings-etc";
/// Written by asimaos-updaterd when the settings came from a version older than an update's
/// migration steps reach (D52): Asima Maintenance then suggests Reset settings.
pub const SETTINGS_BEHIND: &str = "/var/lib/asimaos/settings-behind";
pub const RESET_PEOPLE: &str = "/run/initramfs/data/flags/reset-settings";
/// A copy of /etc's changes from before each reset (root only); the newest three are kept.
pub const OLD_SETTINGS: &str = "/run/initramfs/data/old-settings";

/// What "Reset settings" leaves as it is in /etc. A `*` matches within one part of the path; a
/// folder keeps everything in it.
const RESET_KEEP: [&str; 32] = [
    // The accounts (and their backup copies) and this installation.
    "passwd",
    "passwd-",
    "shadow",
    "shadow-",
    "group",
    "group-",
    "gshadow",
    "gshadow-",
    "subuid",
    "subgid",
    "machine-id",
    "crypttab",
    "fstab",
    "asimaos/install.conf",
    // The computer's name, the language, the keyboard, the time zone and the clock.
    "hostname",
    "machine-info",
    "locale.conf",
    "vconsole.conf",
    "X11/xorg.conf.d/00-keyboard.conf",
    "localtime",
    "adjtime",
    // Wi-Fi, VPN and the other networks; this computer's SSH identity; certificates added by hand.
    "NetworkManager/system-connections",
    "ssh/ssh_host_*",
    "ca-certificates/trust-source",
    // Backups.
    "asimaos/backup.conf",
    "asimaos/restic.pw",
    // Virtual machines and the graphics card kept for them.
    "libvirt",
    "asimaos/passthrough.conf",
    "systemd/system/user@.service.d/90-asimaos-passthrough.conf",
    "systemd/user.conf.d/90-asimaos-passthrough.conf",
    // The installed snaps' mounts and services.
    "systemd/system/snap*",
    "systemd/system/*/snap*",
];

/// Files whose loss would lock people out or break the system: never restored this way.
const PROTECTED: [&str; 10] = [
    "passwd",
    "shadow",
    "group",
    "gshadow",
    "subuid",
    "subgid",
    "machine-id",
    "crypttab",
    "fstab",
    "asimaos/install.conf",
];

/// "/etc/hostname" → "hostname", refusing anything outside /etc or with "..".
pub fn relative(path: &str) -> Result<String> {
    let rel = path.strip_prefix("/etc/").unwrap_or("");
    ensure!(
        !rel.is_empty() && !rel.split('/').any(|p| p.is_empty() || p == "." || p == ".."),
        "That isn't a file in /etc"
    );
    ensure!(
        !PROTECTED.contains(&rel),
        "This file holds accounts or keeps the system starting, so it can't be restored here"
    );
    Ok(rel.to_string())
}

pub fn schedule(root: &Root, path: &str) -> Result<()> {
    let rel = relative(path)?;
    let upper = root.path(UPPER).join(&rel);
    ensure!(upper.symlink_metadata().is_ok(), "{path} already is the factory version");
    let list = root.path(LIST);
    let existing = std::fs::read_to_string(&list).unwrap_or_default();
    if existing.lines().any(|l| l == rel) {
        return Ok(());
    }
    std::fs::create_dir_all(list.parent().unwrap())?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&list)?;
    writeln!(f, "{rel}")?;
    f.sync_all()?;
    Ok(())
}

pub fn scheduled(root: &Root) -> Vec<String> {
    std::fs::read_to_string(root.path(LIST)).unwrap_or_default().lines().map(|l| format!("/etc/{l}")).collect()
}

/// `*` matches any run of characters within one path part.
fn glob(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len()
                && name.starts_with(head)
                && (0..=name.len() - head.len())
                    .any(|i| name.is_char_boundary(head.len() + i) && glob(tail, &name[head.len() + i..]))
        }
    }
}

/// Whether `rel` (relative to /etc) is kept by the reset: it is a kept path or inside one.
fn kept(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    RESET_KEEP.iter().any(|k| {
        let keep: Vec<&str> = k.split('/').collect();
        parts.len() >= keep.len() && keep.iter().zip(&parts).all(|(k, p)| glob(k, p))
    })
}

/// Whether the folder `rel` holds a kept path (so it is walked, not reset as a whole).
fn holds_kept(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    RESET_KEEP.iter().any(|k| {
        let keep: Vec<&str> = k.split('/').collect();
        keep.len() > parts.len() && keep.iter().zip(&parts).all(|(k, p)| glob(k, p))
    })
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("{} could not be read", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    for name in names {
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if kept(&r) {
            continue;
        }
        let path = dir.join(&name);
        let is_dir = path.symlink_metadata().is_ok_and(|m| m.is_dir());
        if is_dir && holds_kept(&r) {
            walk(&path, &r, out)?;
        } else {
            out.push(r);
        }
    }
    Ok(())
}

/// The /etc changes "Reset settings" undoes: every entry of the overlay's upper directory but
/// the kept ones, as paths relative to /etc.
pub fn reset_plan(root: &Root) -> Result<Vec<String>> {
    let upper = root.path(UPPER);
    let mut out = Vec::new();
    if upper.is_dir() {
        walk(&upper, "", &mut out)?;
    }
    Ok(out)
}

/// The account name of `uid` when it is a person's (a home folder in /home).
pub fn person(root: &Root, uid: u32) -> Result<String> {
    let passwd = std::fs::read_to_string(root.path("/etc/passwd")).context("The accounts could not be read")?;
    for line in passwd.lines() {
        let f: Vec<&str> = line.split(':').collect();
        if f.len() >= 7 && f[2].parse::<u32>().ok() == Some(uid) {
            ensure!(
                f[5].starts_with("/home/") && asimaos_svc::valid_username(f[0]),
                "Settings can only be reset for a person's account"
            );
            return Ok(f[0].to_string());
        }
    }
    bail!("This account isn't on this computer")
}

/// Schedules "Reset settings" for the next start: /etc's changes (but the kept ones) go back to
/// the factory version, and `person`'s own desktop and app settings follow. A copy of /etc's
/// changes is kept first. Returns how many /etc entries are reset.
pub async fn schedule_reset(root: &Root, person: &str) -> Result<usize> {
    ensure!(asimaos_svc::valid_username(person), "That isn't an account name");
    let plan = reset_plan(root)?;
    let old = root.path(OLD_SETTINGS);
    std::fs::create_dir_all(&old)?;
    let copy = old.join(asimaos_svc::snapshots::stamp_now());
    let _ = std::fs::remove_dir_all(&copy);
    let (upper, target) = (root.path(UPPER), copy.join("etc"));
    if upper.is_dir() {
        std::fs::create_dir_all(&copy)?;
        cmd::run("cp", &["-a", "--reflink=auto", &upper.to_string_lossy(), &target.to_string_lossy()])
            .await
            .context("The current settings couldn't be saved first, so nothing was reset")?;
    }
    let mut copies: Vec<_> = std::fs::read_dir(&old)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    copies.sort();
    for c in copies.iter().take(copies.len().saturating_sub(3)) {
        let _ = std::fs::remove_dir_all(c);
    }

    let write = |path: &str, text: String| -> Result<()> {
        let p = root.path(path);
        std::fs::create_dir_all(p.parent().unwrap())?;
        let mut f = std::fs::File::create(&p)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        Ok(())
    };
    write(RESET_LIST, plan.iter().map(|l| format!("{l}\n")).collect())?;
    // Settings from a version more than a year older (D52) are this version's again afterwards.
    let _ = std::fs::remove_file(root.path(SETTINGS_BEHIND));
    let mut people: Vec<String> =
        std::fs::read_to_string(root.path(RESET_PEOPLE)).unwrap_or_default().lines().map(String::from).collect();
    if !people.iter().any(|p| p == person) {
        people.push(person.to_string());
    }
    write(RESET_PEOPLE, people.iter().map(|l| format!("{l}\n")).collect())?;
    Ok(plan.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_keeps_accounts_and_choices() {
        for p in PROTECTED {
            assert!(kept(p), "{p}");
        }
        assert!(glob("ssh_host_*", "ssh_host_ed25519_key") && !glob("ssh_host_*", "sshd_config"));
        assert!(glob("snap*", "snap-hello-42.mount") && glob("a*b*c", "aXbYc") && !glob("a*b", "ab_"));
        let d = std::env::temp_dir().join(format!("asimaos-etc-reset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let root = Root(d.clone());
        let up = root.path(UPPER);
        for f in [
            "passwd",
            "shadow",
            "hostname",
            "locale.conf",
            "kernel/cmdline",
            "asimaos/install.conf",
            "asimaos/shield.conf",
            "asimaos/restic.pw",
            "NetworkManager/NetworkManager.conf",
            "NetworkManager/system-connections/home.nmconnection",
            "ssh/sshd_config.d/50-asimaos.conf",
            "ssh/ssh_host_ed25519_key",
            "systemd/system/multi-user.target.wants/sshd.service",
            "systemd/system/multi-user.target.wants/snap-hello-42.mount",
            "systemd/system/snap-hello-42.mount",
            "libvirt/qemu/win11.xml",
            "scx_loader/config.toml",
        ] {
            let p = up.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        assert_eq!(
            reset_plan(&root).unwrap(),
            [
                "NetworkManager/NetworkManager.conf",
                "asimaos/shield.conf",
                "kernel",
                "scx_loader",
                "ssh/sshd_config.d",
                "systemd/system/multi-user.target.wants/sshd.service",
            ]
        );
        std::fs::create_dir_all(root.path("/etc")).unwrap();
        std::fs::write(
            root.path("/etc/passwd"),
            "root:x:0:0::/root:/bin/bash\nasha:x:1000:1000:Asha:/home/asha:/bin/bash\n",
        )
        .unwrap();
        assert_eq!(person(&root, 1000).unwrap(), "asha");
        assert!(person(&root, 0).is_err() && person(&root, 4242).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn paths() {
        assert_eq!(relative("/etc/hostname").unwrap(), "hostname");
        assert_eq!(relative("/etc/ssh/sshd_config.d/50-asimaos.conf").unwrap(), "ssh/sshd_config.d/50-asimaos.conf");
        assert!(relative("/etc/../root/x").is_err());
        assert!(relative("/home/asha/x").is_err());
        assert!(relative("/etc/shadow").is_err());
        assert!(relative("/etc/").is_err());
    }
}
