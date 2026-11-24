//! Snapshots: read-only copies on the same disk that undo mistakes (apps/DESIGN.md: Asima Maintenance).
//! Shared by asimaos-updaterd (a set before every update) and asimaos-maintenanced (/home with the
//! settings and accounts, on the schedule chosen in Asima Maintenance: daily by default).
//!
//! ```text
//! @snapshots/home/<YYYY-MM-DD-HHMM>            /home on schedule (and "Snapshot now"): the newest
//!                                              14, only the newest 2 while free space is under 15 %
//! @snapshots/data/<YYYY-MM-DD-HHMM>            the settings and accounts (/etc), taken together
//!                                              with each /home snapshot, kept the same way; Asima OS
//!                                              Rescue can bring an account back from them
//! @snapshots/update/<version>/{home,data,var,root}   before every update: the last 2 updates
//! ```
//!
//! People browse the home snapshots read-only at `/home/.snapshots` (set up by asimaos-early-mount);
//! each person can open only their own folder there, because the snapshot keeps its permissions.
//! A service reaches the subvolumes through the pool's top level, mounted in its private mount
//! namespace at a folder of its own (`top`). In test mode (`--root`) the pool is the folder `<root>/pool` and snapshots are copies.

use crate::{cmd, Root};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const KEEP: usize = 14;
const KEEP_UPDATES: usize = 2;

pub struct Pool {
    top: PathBuf,
    test: bool,
}

/// Date and time (UTC) as `YYYY-MM-DD-HHMM`.
pub fn stamp_now() -> String {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}-{:02}{:02}", rest / 3600, (rest % 3600) / 60)
}

/// Days since 1970-01-01 → (year, month, day) (H. Hinnant's algorithm).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `YYYY-MM-DD-HHMM` → days since 1970-01-01.
fn stamp_days(s: &str) -> Option<i64> {
    crate::parse_date(s.get(..10)?)
}

impl Pool {
    /// Opens the pool's top level: mounted at `top` (a folder of the calling service under /run,
    /// inside its private mount namespace), or `<root>/pool` in test mode.
    pub async fn open(root: &Root, top: &str) -> Result<Pool> {
        if !root.is_system() {
            let top = root.path("/pool");
            fs::create_dir_all(top.join("@snapshots"))?;
            return Ok(Pool { top, test: true });
        }
        let mount = top;
        let top = PathBuf::from(mount);
        if cmd::probe("mountpoint", &["-q", mount]).await.is_none() {
            fs::create_dir_all(&top)?;
            let src = cmd::run("findmnt", &["-n", "-o", "SOURCE", "--target", "/var"]).await?.stdout;
            let dev = src.trim().split('[').next().unwrap_or("").to_string();
            anyhow::ensure!(dev.starts_with("/dev/"), "the Btrfs pool was not found");
            cmd::run("mount", &["-t", "btrfs", "-o", "subvolid=5,noatime", &dev, mount])
                .await
                .context("the pool could not be opened")?;
        }
        Ok(Pool { top, test: false })
    }

    fn snapshots(&self) -> PathBuf {
        self.top.join("@snapshots")
    }

    async fn snapshot(&self, subvol: &str, dst: &Path) -> Result<()> {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        let src = self.top.join(subvol);
        let (s, d) = (src.to_string_lossy().into_owned(), dst.to_string_lossy().into_owned());
        if self.test {
            cmd::run("cp", &["-a", "--reflink=auto", &s, &d]).await?;
        } else {
            cmd::run("btrfs", &["subvolume", "snapshot", "-r", &s, &d])
                .await
                .with_context(|| format!("snapshot of {subvol} failed"))?;
        }
        Ok(())
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let p = path.to_string_lossy().into_owned();
        if self.test {
            fs::remove_dir_all(path)?;
        } else {
            cmd::run("btrfs", &["subvolume", "delete", &p]).await?;
        }
        Ok(())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Before writing `version`: read-only snapshots of the user files and the system state.
    pub async fn before_update(&self, version: &str) -> Result<()> {
        let dir = self.snapshots().join("update").join(version);
        if dir.exists() {
            // A retried update: the snapshot taken before the first attempt is the right one.
            return Ok(());
        }
        for (sub, name) in [("@home", "home"), ("@data", "data"), ("@var", "var"), ("@root", "root")] {
            if self.top.join(sub).is_dir() {
                self.snapshot(sub, &dir.join(name)).await?;
            }
        }
        let all = Self::names(&self.snapshots().join("update"));
        let mut sets: Vec<String> = all;
        sets.sort_by(|a, b| crate::version::compare(a, b).unwrap_or(std::cmp::Ordering::Equal));
        while sets.len() > KEEP_UPDATES {
            let old = self.snapshots().join("update").join(sets.remove(0));
            for part in Self::names(&old) {
                self.delete(&old.join(part)).await?;
            }
            let _ = fs::remove_dir(&old);
        }
        Ok(())
    }

    /// A snapshot of /home and of the settings and accounts now; then the old ones go (the
    /// newest 14 stay, only 2 while space is short).
    pub async fn snapshot_home(&self) -> Result<String> {
        let name = stamp_now();
        for (sub, kind) in [("@home", "home"), ("@data", "data")] {
            let dir = self.snapshots().join(kind);
            if self.top.join(sub).is_dir() && !dir.join(&name).exists() {
                self.snapshot(sub, &dir.join(&name)).await?;
            }
        }
        let (free, total) = (crate::free_bytes(&self.top).unwrap_or(0), crate::total_bytes(&self.top).unwrap_or(1));
        let keep = if (free as f64) < 0.15 * total as f64 { 2 } else { KEEP };
        for kind in ["home", "data"] {
            let dir = self.snapshots().join(kind);
            let all = Self::names(&dir);
            for n in &all[..all.len().saturating_sub(keep)] {
                self.delete(&dir.join(n)).await?;
            }
        }
        Ok(name)
    }

    /// Days since the newest /home snapshot (None: there is none yet).
    pub fn days_since_newest(&self) -> Option<i64> {
        Self::names(&self.snapshots().join("home")).last().and_then(|n| stamp_days(n)).map(|d| crate::today() - d)
    }

    /// What Asima Maintenance lists, newest first.
    pub fn list(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for n in Self::names(&self.snapshots().join("home")).into_iter().rev() {
            let settings = self.snapshots().join("data").join(&n).is_dir();
            out.push(json!({ "id": format!("home/{n}"), "kind": "daily", "label": n, "when": n, "size": -1, "settings": settings }));
        }
        for v in Self::names(&self.snapshots().join("update")).into_iter().rev() {
            out.push(json!({ "id": format!("update/{v}"), "kind": "update", "label": v, "when": v, "size": -1 }));
        }
        out
    }
}
