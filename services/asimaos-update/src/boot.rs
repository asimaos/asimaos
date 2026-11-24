//! The boot menu: which installed version is "current" and which is the "backup".
//!
//! UKIs live in `EFI/Asima/asimaos_<version>.efi` (not `EFI/Linux`, so systemd-boot does not list
//! them a second time on its own). The menu is made of Boot Loader Specification type #1 entries
//! that point at them, so their titles can say what they are, and the current version is always
//! on top:
//!
//! ```text
//! loader/entries/asimaos-current[+L-D].conf  "Asima OS 2026.11.18.1 (current)"        sort-key asimaos-1
//! loader/entries/asimaos-backup[+L-D].conf   "Asima OS 2026.11.16.2 (backup)"         sort-key asimaos-2
//! loader/entries/asimaos-rescue.conf         "Asima Rescue"                           sort-key asimaos-8
//! loader/entries/asimaos-failed.conf         "Asima OS 2026.11.20.1 (failed update)"  sort-key asimaos-9
//! ```
//!
//! The version number is written the way the Asima OS apps show it (`IMAGE_VERSION`), never
//! "alpha" or "beta".
//!
//! `+L-D` is systemd's boot counter (L tries left, D done) for a version on trial;
//! `systemd-bless-boot` removes it after a good start. `loader.conf` says `default asimaos-*`, and
//! systemd-boot sorts entries without tries left to the end of the menu: a new version that keeps
//! failing falls back to the backup by itself (and to Rescue if there is no backup). After such a
//! fallback, [`settle`] makes the working version current again: seamless switching.

use anyhow::{bail, ensure, Context, Result};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const UKI_DIR: &str = "EFI/Asima";
pub const ENTRIES_DIR: &str = "loader/entries";
pub const RESCUE_UKI: &str = "asimaos-rescue.efi";
const TRIES: (u32, u32) = (3, 0);

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Role {
    Current,
    Backup,
    Failed,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Current => "current",
            Role::Backup => "backup",
            Role::Failed => "failed",
        }
    }
    fn title(self, version: &str) -> String {
        let role = match self {
            Role::Current => "current",
            Role::Backup => "backup",
            Role::Failed => "failed update",
        };
        format!("Asima OS {version} ({role})")
    }
    fn sort_key(self) -> &'static str {
        match self {
            Role::Current => "asimaos-1",
            Role::Backup => "asimaos-2",
            Role::Failed => "asimaos-9",
        }
    }
    fn parse(s: &str) -> Option<Role> {
        match s {
            "current" => Some(Role::Current),
            "backup" => Some(Role::Backup),
            "failed" => Some(Role::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BootEntry {
    pub role: Role,
    pub version: String,
    /// (tries left, tries done) while the version is on trial; `None` once it started fine.
    pub tries: Option<(u32, u32)>,
    pub file: PathBuf,
}

impl BootEntry {
    /// The entry's identifier as systemd-boot reports it (`LoaderEntrySelected`): no counter.
    pub fn id(&self) -> String {
        format!("asimaos-{}.conf", self.role.name())
    }
    pub fn blessed(&self) -> bool {
        self.tries.is_none()
    }
    /// No tries left: systemd-boot no longer starts it by itself.
    pub fn bad(&self) -> bool {
        matches!(self.tries, Some((0, _)))
    }
}

pub fn uki_name(version: &str) -> String {
    format!("asimaos_{version}.efi")
}

fn valid_version(v: &str) -> bool {
    let p: Vec<&str> = v.split('.').collect();
    p.len() == 4 && p.iter().all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit()))
}

/// `asimaos-current+2-1` → (Current, Some((2, 1))).
fn parse_name(stem: &str) -> Option<(Role, Option<(u32, u32)>)> {
    let rest = stem.strip_prefix("asimaos-")?;
    let (role, counter) = match rest.split_once('+') {
        Some((r, c)) => (r, Some(c)),
        None => (rest, None),
    };
    let role = Role::parse(role)?;
    let tries = match counter {
        None => None,
        Some(c) => {
            let (l, d) = c.split_once('-').unwrap_or((c, "0"));
            Some((l.parse().ok()?, d.parse().ok()?))
        }
    };
    Some((role, tries))
}

/// The Asima OS entries on the ESP mounted at `esp`, current first.
pub fn entries(esp: &Path) -> Vec<BootEntry> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(esp.join(ENTRIES_DIR)) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".conf") else { continue };
        let Some((role, tries)) = parse_name(stem) else { continue };
        let Ok(text) = fs::read_to_string(e.path()) else { continue };
        let Some(version) = text.lines().find_map(|l| l.strip_prefix("version").map(|v| v.trim().to_string())) else {
            continue;
        };
        if valid_version(&version) {
            out.push(BootEntry { role, version, tries, file: e.path() });
        }
    }
    out.sort_by_key(|e| e.role);
    out
}

pub fn find(esp: &Path, role: Role) -> Option<BootEntry> {
    entries(esp).into_iter().find(|e| e.role == role)
}

fn sync_dir(dir: &Path) {
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

/// Writes the entry for `role` (replacing any older one of the same role).
pub fn write_entry(esp: &Path, role: Role, version: &str, tries: Option<(u32, u32)>) -> Result<PathBuf> {
    ensure!(valid_version(version), "invalid version {version}");
    let dir = esp.join(ENTRIES_DIR);
    fs::create_dir_all(&dir)?;
    for old in entries(esp).into_iter().filter(|e| e.role == role) {
        fs::remove_file(&old.file)?;
    }
    let name = match tries {
        Some((l, d)) => format!("asimaos-{}+{l}-{d}.conf", role.name()),
        None => format!("asimaos-{}.conf", role.name()),
    };
    let tmp = dir.join(format!(".{name}.tmp"));
    let path = dir.join(&name);
    let mut f = File::create(&tmp).with_context(|| format!("can't write {}", tmp.display()))?;
    write!(
        f,
        "# Asima OS start menu entry, kept up to date by Asima Updater.\n\
         title    {}\nversion  {version}\nsort-key {}\nuki      /{UKI_DIR}/{}\n",
        role.title(version),
        role.sort_key(),
        uki_name(version)
    )?;
    f.sync_all()?;
    fs::rename(&tmp, &path)?;
    sync_dir(&dir);
    Ok(path)
}

fn referenced(esp: &Path) -> BTreeSet<String> {
    entries(esp).into_iter().map(|e| e.version).collect()
}

/// A newly written `version` becomes current with three tries; the previous current becomes the
/// backup (an older backup and a failed version leave the menu). Writing the same version again
/// (reinstall) gives it a fresh trial and leaves the backup alone. Returns the versions that no
/// longer have an entry, so their files can be removed.
pub fn promote(esp: &Path, version: &str) -> Result<Vec<String>> {
    ensure!(esp.join(UKI_DIR).join(uki_name(version)).is_file(), "the boot image of {version} is missing");
    let before = referenced(esp);
    let current = find(esp, Role::Current);
    match &current {
        Some(c) if c.version == version => {}
        Some(c) => {
            write_entry(esp, Role::Backup, &c.version, c.tries)?;
        }
        None => {}
    }
    write_entry(esp, Role::Current, version, Some(TRIES))?;
    if let Some(f) = find(esp, Role::Failed) {
        fs::remove_file(&f.file)?;
    }
    if find(esp, Role::Backup).is_some_and(|b| b.version == version) {
        // The backup was this very version (e.g. reinstalled from Rescue): no duplicate entry.
        if let Some(b) = find(esp, Role::Backup) {
            fs::remove_file(&b.file)?;
        }
    }
    let after = referenced(esp);
    Ok(before.difference(&after).cloned().collect())
}

/// Makes `version` current. The backup and the current swap places ("go back to the backup"); a
/// failed version gets a fresh trial, with the current becoming the backup. Returns the versions
/// that left the menu.
pub fn make_current(esp: &Path, version: &str) -> Result<Vec<String>> {
    let before = referenced(esp);
    let es = entries(esp);
    let Some(target) = es.iter().find(|e| e.version == version).cloned() else {
        bail!("Asima OS {version} has no start menu entry")
    };
    let current = es.iter().find(|e| e.role == Role::Current).cloned();
    match target.role {
        Role::Current => return Ok(Vec::new()),
        Role::Backup => {
            write_entry(esp, Role::Current, &target.version, target.tries)?;
            match current {
                Some(c) => {
                    write_entry(esp, Role::Backup, &c.version, c.tries)?;
                }
                None => {
                    let _ = fs::remove_file(&target.file);
                }
            }
        }
        Role::Failed => {
            fs::remove_file(&target.file)?;
            if let Some(c) = &current {
                write_entry(esp, Role::Backup, &c.version, c.tries)?;
            }
            write_entry(esp, Role::Current, version, Some(TRIES))?;
        }
    }
    let after = referenced(esp);
    Ok(before.difference(&after).cloned().collect())
}

/// After a good start of `running`: when the start menu fell back to the backup because the
/// current version kept failing, the working version becomes current again and the one that
/// failed is kept as "failed update". Returns true when it changed something.
pub fn settle(esp: &Path, running: &str) -> Result<bool> {
    let es = entries(esp);
    let current = es.iter().find(|e| e.role == Role::Current).cloned();
    let me = es.iter().find(|e| e.version == running).cloned();
    match (current, me) {
        (Some(c), Some(m)) if m.role == Role::Backup && c.bad() && c.version != running => {
            write_entry(esp, Role::Failed, &c.version, None)?;
            write_entry(esp, Role::Current, running, m.tries)?;
            fs::remove_file(&m.file)
                .or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Removes every entry of `version`.
pub fn remove(esp: &Path, version: &str) -> Result<()> {
    for e in entries(esp).into_iter().filter(|e| e.version == version) {
        fs::remove_file(&e.file)?;
    }
    Ok(())
}

/// The Rescue entry exists exactly when its image does.
pub fn sync_rescue(esp: &Path) -> Result<()> {
    let dir = esp.join(ENTRIES_DIR);
    let path = dir.join("asimaos-rescue.conf");
    if esp.join(UKI_DIR).join(RESCUE_UKI).is_file() {
        fs::create_dir_all(&dir)?;
        fs::write(&path, format!("# Asima OS start menu entry.\ntitle    Asima Rescue\nsort-key asimaos-8\nuki      /{UKI_DIR}/{RESCUE_UKI}\n"))?;
    } else if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// The `loader.conf` every Asima OS ESP gets.
pub fn loader_conf() -> &'static str {
    "# Asima OS: the current version on top starts by itself; a version that keeps failing\n\
     # falls back to the backup (entries without tries left sort last).\n\
     default asimaos-*\ntimeout 3\neditor no\nconsole-mode keep\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("asimaos-boot-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join(UKI_DIR)).unwrap();
        d
    }
    fn uki(d: &Path, v: &str) {
        fs::write(d.join(UKI_DIR).join(uki_name(v)), v).unwrap();
    }
    fn bless(d: &Path, r: Role) {
        let e = find(d, r).unwrap();
        fs::rename(&e.file, d.join(ENTRIES_DIR).join(e.id())).unwrap();
    }
    fn exhaust(d: &Path, r: Role) {
        let e = find(d, r).unwrap();
        fs::rename(&e.file, d.join(ENTRIES_DIR).join(format!("asimaos-{}+0-3.conf", r.name()))).unwrap();
    }
    fn roles(d: &Path) -> Vec<(String, String, bool)> {
        entries(d).into_iter().map(|e| (e.role.name().to_string(), e.version.clone(), e.blessed())).collect()
    }

    #[test]
    fn update_rollback_fallback() {
        let d = esp("flow");
        uki(&d, "2026.01.01.1");
        assert!(promote(&d, "2026.01.01.1").unwrap().is_empty());
        let e = find(&d, Role::Current).unwrap();
        assert_eq!((e.tries, e.id()), (Some((3, 0)), "asimaos-current.conf".to_string()));
        assert!(fs::read_to_string(&e.file).unwrap().contains("title    Asima OS 2026.01.01.1 (current)"));
        bless(&d, Role::Current);
        // An update: the new version is current on trial, the old one the backup.
        uki(&d, "2026.02.01.1");
        assert!(promote(&d, "2026.02.01.1").unwrap().is_empty());
        assert_eq!(
            roles(&d),
            [("current".into(), "2026.02.01.1".into(), false), ("backup".into(), "2026.01.01.1".into(), true)]
        );
        // It keeps failing; the menu falls back; after the good start the roles settle.
        exhaust(&d, Role::Current);
        assert!(!settle(&d, "2026.02.01.1").unwrap(), "nothing to do when the current one runs");
        assert!(settle(&d, "2026.01.01.1").unwrap());
        assert_eq!(
            roles(&d),
            [("current".into(), "2026.01.01.1".into(), true), ("failed".into(), "2026.02.01.1".into(), true)]
        );
        // Trying the failed update again: fresh trial, the working version is the backup.
        assert!(make_current(&d, "2026.02.01.1").unwrap().is_empty());
        assert_eq!(
            roles(&d),
            [("current".into(), "2026.02.01.1".into(), false), ("backup".into(), "2026.01.01.1".into(), true)]
        );
        bless(&d, Role::Current);
        // Going back by hand swaps the two; nothing leaves the menu.
        assert!(make_current(&d, "2026.01.01.1").unwrap().is_empty());
        assert_eq!(
            roles(&d),
            [("current".into(), "2026.01.01.1".into(), true), ("backup".into(), "2026.02.01.1".into(), true)]
        );
        // The next update pushes out the older backup.
        uki(&d, "2026.03.01.1");
        assert_eq!(promote(&d, "2026.03.01.1").unwrap(), ["2026.02.01.1"]);
        assert_eq!(
            roles(&d),
            [("current".into(), "2026.03.01.1".into(), false), ("backup".into(), "2026.01.01.1".into(), true)]
        );
        // Reinstalling the current version: a fresh trial, the backup stays.
        assert!(promote(&d, "2026.03.01.1").unwrap().is_empty());
        assert_eq!(find(&d, Role::Current).unwrap().tries, Some((3, 0)));
        assert_eq!(find(&d, Role::Backup).unwrap().version, "2026.01.01.1");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn rescue_and_odd_files() {
        let d = esp("odd");
        fs::create_dir_all(d.join(ENTRIES_DIR)).unwrap();
        fs::write(d.join(ENTRIES_DIR).join("other-os.conf"), "title Other\nversion 1\n").unwrap();
        fs::write(d.join(ENTRIES_DIR).join("asimaos-current+x.conf"), "version 2026.01.01.1\n").unwrap();
        assert!(entries(&d).is_empty(), "foreign or broken files are ignored");
        assert!(promote(&d, "2026.01.01.1").is_err(), "no entry without its boot image");
        sync_rescue(&d).unwrap();
        assert!(!d.join(ENTRIES_DIR).join("asimaos-rescue.conf").exists());
        fs::write(d.join(UKI_DIR).join(RESCUE_UKI), "r").unwrap();
        sync_rescue(&d).unwrap();
        assert!(fs::read_to_string(d.join(ENTRIES_DIR).join("asimaos-rescue.conf")).unwrap().contains("Asima Rescue"));
        let _ = fs::remove_dir_all(d);
    }
}
