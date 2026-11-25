//! Disk use and limits (D55): how much each area of the pool takes, and a limit for those that can
//! grow without end: containers, virtual machines and Nix programs (the system's, and each
//! person's), Flatpak apps, /home and /var. Btrfs simple quotas count what each subvolume wrote
//! since they were switched on (the installer does that); a limit stops writes past it, and the
//! program writing sees "disk quota exceeded". Each person's containers and virtual machine disks
//! are subvolumes of their own (the user service asimaos-dev-storage makes them), so they can be
//! limited, and they stay out of the snapshots of /home.

use anyhow::{ensure, Context, Result};
use asimaos_svc::{cmd, Root};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// The system's areas: (id, mount point).
const SYSTEM: [(&str, &str); 6] = [
    ("containers", "/var/lib/containers"),
    ("vm", "/var/lib/libvirt"),
    ("nix", "/nix"),
    ("flatpak", "/var/lib/flatpak"),
    ("home", "/home"),
    ("var", "/var"),
];
/// Each person's, in their home folder: (id, folder).
const PERSONAL: [(&str, &str); 2] = [("containers", ".local/share/containers"), ("vm", ".local/share/libvirt/images")];
const GIB: u64 = 1 << 30;

/// One area: "nix", or "containers:asha" for a person's.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub id: String,
    pub path: PathBuf,
    pub person: Option<String>,
}

/// A subvolume's own top folder is inode 256 on Btrfs.
fn is_subvolume(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.ino() == 256)
}

/// The people with a home folder (accounts from 1000 on), by name.
fn people(root: &Root) -> Vec<(String, PathBuf)> {
    std::fs::read_to_string(root.path("/etc/passwd"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            (f.len() >= 7 && (1000..60000).contains(&uid) && f[5].starts_with("/home/"))
                .then(|| (f[0].to_string(), PathBuf::from(f[5])))
        })
        .collect()
}

pub fn areas(root: &Root) -> Vec<Area> {
    let mut out: Vec<Area> =
        SYSTEM.iter().map(|(id, p)| Area { id: id.to_string(), path: root.path(p), person: None }).collect();
    for (name, home) in people(root) {
        for (id, dir) in PERSONAL {
            out.push(Area {
                id: format!("{id}:{name}"),
                path: root.path(&home.to_string_lossy()).join(dir),
                person: Some(name.clone()),
            });
        }
    }
    out
}

/// `btrfs qgroup show --raw -r`: (used, limit) by subvolume ID.
fn parse_qgroups(text: &str) -> HashMap<u64, (u64, Option<u64>)> {
    text.lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let id: u64 = t.first()?.strip_prefix("0/")?.parse().ok()?;
            let used: u64 = t.get(1)?.parse().ok()?;
            let limit = t.get(3).and_then(|v| v.parse().ok());
            Some((id, (used, limit)))
        })
        .collect()
}

async fn qgroups(pool: &Path) -> Result<HashMap<u64, (u64, Option<u64>)>> {
    let out = cmd::run("btrfs", &["qgroup", "show", "--raw", "-r", &pool.to_string_lossy()]).await?;
    Ok(parse_qgroups(&out.stdout))
}

async fn subvolume_id(p: &Path) -> Option<u64> {
    cmd::probe("btrfs", &["inspect-internal", "rootid", &p.to_string_lossy()]).await?.trim().parse().ok()
}

/// Every area's use and limit, for Asima Dev (and Asima Maintenance).
pub async fn status(root: &Root) -> Value {
    let counted = qgroups(&root.path("/var")).await.ok();
    let mut list = Vec::new();
    for a in areas(root) {
        let separate = is_subvolume(&a.path);
        let (used, limit) = match (&counted, separate) {
            (Some(q), true) => match subvolume_id(&a.path).await.and_then(|id| q.get(&id).copied()) {
                Some((u, l)) => (Some(u), l),
                None => (None, None),
            },
            _ => (None, None),
        };
        list.push(json!({
            "id": a.id, "person": a.person, "path": a.path, "separate": separate, "used": used, "limit": limit,
        }));
    }
    json!({ "counted": counted.is_some(), "areas": list })
}

/// A limit for one area, in bytes (0: none). Simple quotas are switched on first when they are off.
pub async fn set_limit(root: &Root, id: &str, bytes: u64) -> Result<()> {
    let area = areas(root).into_iter().find(|a| a.id == id).context("Unknown storage area")?;
    ensure!(bytes == 0 || bytes >= GIB, "A limit must be at least 1 GB");
    ensure!(is_subvolume(&area.path), "This area isn't separate yet: sign in once more, then try again");
    let path = area.path.to_string_lossy().into_owned();
    if qgroups(&area.path).await.is_err() {
        cmd::run("btrfs", &["quota", "enable", "--simple", &path])
            .await
            .context("Disk use can't be counted on this computer (Btrfs simple quotas)")?;
    }
    let limit = if bytes == 0 { "none".to_string() } else { bytes.to_string() };
    cmd::run("btrfs", &["qgroup", "limit", &limit, &path]).await.context("The limit could not be set")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qgroup_lines() {
        let text = "Qgroupid    Referenced    Exclusive   Max referenced   Path\n\
                    --------    ----------    ---------   --------------   ----\n\
                    0/5               16384        16384             none   <toplevel>\n\
                    0/257        8053063680   8053063680      21474836480   @vm\n\
                    1/100              4096         4096             none   <squota space holder>\n";
        let q = parse_qgroups(text);
        assert_eq!(q.get(&257), Some(&(8053063680, Some(21474836480))));
        assert_eq!(q.get(&5), Some(&(16384, None)));
        assert!(!q.contains_key(&100));
    }

    #[test]
    fn areas_for_each_person() {
        let t = std::env::temp_dir().join(format!("asimaos-storage-{}", std::process::id()));
        std::fs::create_dir_all(t.join("etc")).unwrap();
        std::fs::write(
            t.join("etc/passwd"),
            "root:x:0:0::/root:/bin/bash\nasha:x:1000:1000:Asha:/home/asha:/bin/bash\nnobody:x:65534:65534::/:/usr/bin/nologin\n",
        )
        .unwrap();
        let a = areas(&Root(t.clone()));
        let ids: Vec<&str> = a.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["containers", "vm", "nix", "flatpak", "home", "var", "containers:asha", "vm:asha"]);
        assert!(a[7].path.ends_with("home/asha/.local/share/libvirt/images") && a[7].person.as_deref() == Some("asha"));
        let _ = std::fs::remove_dir_all(t);
    }
}
