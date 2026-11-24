//! Backups with restic: encrypted copies of /home on another drive, a network share or any
//! restic repository (apps/DESIGN.md: Asima Maintenance).
//!
//! Settings: `/etc/asimaos/backup.conf` (`REPOSITORY=...`) and the password in
//! `/etc/asimaos/restic.pw` (root only). Results: `/var/lib/asimaos/backup/status.json`.
//! A run backs up /home, keeps 7 daily and 4 weekly versions, and then restores one file from the
//! newest backup into a temporary folder to prove the backup can be read back. On an encrypted
//! disk it also keeps a copy of the disk-encryption (LUKS2) header, tagged on its own (the newest 4):
//! a damaged header otherwise loses the whole disk.
//! The run itself happens in `asimaos-backup.service` (it needs to read /home, which the D-Bus
//! service is not allowed to), started by the timer or by "Back up now".
//!
//! Restoring never overwrites anything: one person's folder from a chosen backup comes
//! back as a new folder `Restored YYYY-MM-DD` in their home, written by `asimaos-restore.service`
//! (it may write to /home) from the request the D-Bus service leaves in `RESTORE_REQUEST`.

use anyhow::{bail, Context, Result};
use asimaos_svc::{cmd, Root};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub const CONF: &str = "/etc/asimaos/backup.conf";
pub const PASSWORD: &str = "/etc/asimaos/restic.pw";
pub const STATUS: &str = "/var/lib/asimaos/backup/status.json";
pub const RESTORE_REQUEST: &str = "/var/lib/asimaos/backup/restore-request.json";
/// Where the header copy is written for restic, and removed again right after.
pub const HEADER: &str = "/var/lib/asimaos/backup/disk-header.img";
/// Test mode only: a file naming the "disk" (a LUKS2 image) instead of the real pool's partition.
const TEST_DEVICE: &str = "/run/asimaos-test/luks-device";

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn repository(root: &Root) -> Option<String> {
    let text = fs::read_to_string(root.path(CONF)).ok()?;
    text.lines().find_map(|l| l.strip_prefix("REPOSITORY=")).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// A folder (USB drive, mounted share) or a restic URL (sftp:, rest:, s3:, b2:, ...).
fn valid_repository(r: &str) -> bool {
    let url = ["sftp:", "rest:", "s3:", "b2:", "azure:", "gs:", "swift:"].iter().any(|p| r.starts_with(p));
    (r.starts_with('/') || url) && !r.contains('\n') && r.len() < 1024
}

async fn restic(root: &Root, args: &[&str]) -> Result<cmd::Output> {
    let repo = repository(root).context("No backup is set up")?;
    let pw = root.path(PASSWORD).to_string_lossy().into_owned();
    let mut all = vec!["-r", repo.as_str(), "--password-file", pw.as_str(), "--no-cache", "-q"];
    all.extend_from_slice(args);
    // RESTIC_* variables from the environment never reach the tool.
    cmd::run_with("restic", &all, None, &[("RESTIC_PASSWORD", ""), ("RESTIC_REPOSITORY", "")]).await
}

/// Saves the settings and creates the repository there if it does not exist yet.
pub async fn configure(root: &Root, repo: &str, password: &str) -> Result<()> {
    if !valid_repository(repo) {
        bail!("Choose a folder (for example on a USB drive) or a restic address such as sftp:user@host:/backups");
    }
    if password.chars().count() < 8 {
        bail!("The backup password needs at least 8 characters");
    }
    let conf = root.path(CONF);
    fs::create_dir_all(conf.parent().unwrap())?;
    let pw = root.path(PASSWORD);
    fs::write(&pw, password)?;
    fs::set_permissions(&pw, fs::Permissions::from_mode(0o600))?;
    fs::write(&conf, format!("# Asima OS backup (restic). Set up in Asima Maintenance.\nREPOSITORY={repo}\n"))?;
    if repo.starts_with('/') {
        fs::create_dir_all(repo).context("The backup folder could not be created")?;
    }
    if restic(root, &["cat", "config"]).await.is_err() {
        restic(root, &["init"]).await.context("The backup could not be created there")?;
    }
    write_status(root, json!({ "configuredAt": now() }))?;
    Ok(())
}

fn read_status(root: &Root) -> Value {
    fs::read_to_string(root.path(STATUS)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

fn write_status(root: &Root, add: Value) -> Result<()> {
    let mut s = read_status(root);
    if let (Some(obj), Some(new)) = (s.as_object_mut(), add.as_object()) {
        for (k, v) in new {
            obj.insert(k.clone(), v.clone());
        }
    }
    let path = root.path(STATUS);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, serde_json::to_string_pretty(&s)?)?;
    Ok(())
}

/// One backup run: back up, thin out, prove a restore. The result is kept for Asima Maintenance.
pub async fn run(root: &Root) -> Result<()> {
    if repository(root).is_none() {
        return Ok(()); // not set up: the timer has nothing to do
    }
    write_status(root, json!({ "running": true, "startedAt": now() }))?;
    let home = root.path("/home").to_string_lossy().into_owned();
    let result: Result<Value> = async {
        restic(root, &["backup", &home, "--one-file-system", "--exclude-caches", "--exclude", &format!("{home}/.snapshots"), "--tag", "asimaos"]).await?;
        restic(root, &["forget", "--tag", "asimaos", "--keep-daily", "7", "--keep-weekly", "4", "--prune"]).await?;
        let header = header_backup(root).await?;
        let test = restore_test(root).await;
        let stats = restic(root, &["stats", "--mode", "raw-data", "--json"]).await.ok()
            .and_then(|o| serde_json::from_str::<Value>(&o.stdout).ok()).and_then(|v| v["total_size"].as_u64()).unwrap_or(0);
        let count = restic(root, &["snapshots", "--tag", "asimaos", "--json"]).await.ok()
            .and_then(|o| serde_json::from_str::<Value>(&o.stdout).ok()).and_then(|v| v.as_array().map(|a| a.len())).unwrap_or(0);
        Ok(json!({ "lastTestOk": test.is_ok(), "lastTestMessage": test.err().map(|e| e.to_string()), "repoSize": stats, "kept": count,
                   "headerSaved": header.then(now) }))
    }
    .await;
    match result {
        Ok(v) => {
            write_status(root, v)?;
            write_status(
                root,
                json!({ "running": false, "lastRun": now(), "lastOk": true, "lastMessage": null, "lastTest": now() }),
            )?;
            Ok(())
        }
        Err(e) => {
            write_status(
                root,
                json!({ "running": false, "lastRun": now(), "lastOk": false, "lastMessage": e.to_string() }),
            )?;
            Err(e)
        }
    }
}

/// The encrypted partition, when the pool is encrypted.
async fn encrypted_device(root: &Root) -> Option<String> {
    if root.is_system() {
        crate::security::luks_device().await.ok()
    } else {
        fs::read_to_string(root.path(TEST_DEVICE)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

/// Copies the LUKS2 header into the backup (tag `asimaos-disk-header`). False on a plain disk. The
/// copy holds the key slots as they are now, so it only ever opens the disk with a secret that was
/// valid at the time (the passphrase, the recovery key); after changing them the next backup has
/// the new header.
async fn header_backup(root: &Root) -> Result<bool> {
    let Some(dev) = encrypted_device(root).await else { return Ok(false) };
    let file = root.path(HEADER);
    fs::create_dir_all(file.parent().context("bad path")?)?;
    let _ = fs::remove_file(&file); // cryptsetup refuses to overwrite
    let f = file.to_string_lossy().into_owned();
    cmd::run("cryptsetup", &["luksHeaderBackup", &dev, "--header-backup-file", &f])
        .await
        .context("The disk's encryption header could not be copied")?;
    let saved = restic(root, &["backup", &f, "--tag", "asimaos-disk-header"]).await;
    let _ = fs::remove_file(&file);
    saved.context("The disk's encryption header could not be backed up")?;
    restic(root, &["forget", "--tag", "asimaos-disk-header", "--keep-last", "4", "--prune"]).await?;
    Ok(true)
}

/// Restores one regular file from the newest backup into a temporary folder and compares its size.
async fn restore_test(root: &Root) -> Result<()> {
    let ls = restic(root, &["ls", "--tag", "asimaos", "latest", "--json"]).await?;
    let file = ls
        .stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["type"] == "file" && v["size"].as_u64().is_some_and(|s| s > 0 && s < 50 << 20))
        .nth((now() as usize) % 7)
        .or_else(|| {
            ls.stdout.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|v| v["type"] == "file")
        })
        .context("The backup holds no file to test with")?;
    let path = file["path"].as_str().context("bad listing")?.to_string();
    let size = file["size"].as_u64().unwrap_or(0);
    let tmp = PathBuf::from(format!("/var/tmp/asimaos-restore-test-{}", std::process::id()));
    let tmp = if root.is_system() { tmp } else { root.path(&tmp.to_string_lossy()) };
    let _ = fs::remove_dir_all(&tmp);
    let t = tmp.to_string_lossy().into_owned();
    let r = restic(root, &["restore", "--tag", "asimaos", "latest", "--target", &t, "--include", &path]).await;
    let restored = tmp.join(path.trim_start_matches('/'));
    let ok = r.is_ok() && fs::metadata(&restored).is_ok_and(|m| m.len() == size);
    let _ = fs::remove_dir_all(&tmp);
    if ok {
        Ok(())
    } else {
        bail!("A test restore of {path} failed")
    }
}

/// The backups that can be restored, newest first: id, time, paths.
pub async fn list(root: &Root) -> Result<Value> {
    let out =
        restic(root, &["snapshots", "--tag", "asimaos", "--json"]).await.context("The backups could not be read")?;
    let all: Value = serde_json::from_str(&out.stdout).context("The list of backups could not be read")?;
    let mut v: Vec<Value> = all
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| json!({ "id": s["short_id"], "time": s["time"], "paths": s["paths"] }))
        .collect();
    v.reverse();
    Ok(Value::Array(v))
}

fn valid_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// Checks a restore and leaves the request for asimaos-restore.service.
pub fn request_restore(root: &Root, id: &str, user: &str) -> Result<()> {
    if repository(root).is_none() {
        bail!("No backup is set up");
    }
    if !valid_id(id) {
        bail!("That backup does not exist");
    }
    if !asimaos_svc::valid_username(user) || !root.path(&format!("/home/{user}")).is_dir() {
        bail!("There is no account {user} on this computer");
    }
    if read_status(root)["restore"]["running"] == true {
        bail!("A restore is already running");
    }
    let path = root.path(RESTORE_REQUEST);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, json!({ "id": id, "user": user }).to_string())?;
    write_status(root, json!({ "restore": { "running": true, "user": user, "backup": id, "startedAt": now() } }))?;
    Ok(())
}

/// asimaos-restore.service: `<home>/Restored YYYY-MM-DD[ n]` from the requested backup.
pub async fn restore_run(root: &Root) -> Result<()> {
    let req: Value = fs::read_to_string(root.path(RESTORE_REQUEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .context("No restore was asked for")?;
    let _ = fs::remove_file(root.path(RESTORE_REQUEST));
    let (id, user) = (req["id"].as_str().unwrap_or(""), req["user"].as_str().unwrap_or(""));
    let result: Result<String> = async {
        if !valid_id(id) || !asimaos_svc::valid_username(user) {
            bail!("The restore request is not valid");
        }
        let home = root.path(&format!("/home/{user}"));
        let base = format!("Restored {}", today_iso());
        let mut dest = home.join(&base);
        let mut n = 2;
        while dest.exists() {
            dest = home.join(format!("{base} {n}"));
            n += 1;
        }
        let d = dest.to_string_lossy().into_owned();
        restic(root, &["restore", &format!("{id}:/home/{user}"), "--target", &d])
            .await
            .context("The backup could not be restored")?;
        // The restored files keep their owners; the new folder belongs to the person too.
        let meta = fs::metadata(&home)?;
        std::os::unix::fs::chown(
            &dest,
            Some(std::os::unix::fs::MetadataExt::uid(&meta)),
            Some(std::os::unix::fs::MetadataExt::gid(&meta)),
        )
        .ok();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o700))?;
        Ok(dest.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
    }
    .await;
    let (ok, folder, message) = match &result {
        Ok(f) => (true, Some(f.clone()), None),
        Err(e) => (false, None, Some(e.to_string())),
    };
    write_status(
        root,
        json!({ "restore": { "running": false, "user": user, "backup": id, "at": now(),
                                             "ok": ok, "folder": folder, "message": message } }),
    )?;
    result.map(|_| ())
}

fn today_iso() -> String {
    let d = asimaos_svc::snapshots::stamp_now();
    d.get(..10).unwrap_or(&d).to_string()
}

/// What Asima Maintenance shows.
pub fn status(root: &Root) -> Value {
    let s = read_status(root);
    json!({
        "configured": repository(root).is_some(),
        "destination": repository(root),
        "running": s["running"].as_bool().unwrap_or(false),
        "lastRun": s["lastRun"], "lastOk": s["lastOk"], "lastMessage": s["lastMessage"],
        "lastTest": s["lastTest"], "lastTestOk": s["lastTestOk"],
        "repoSize": s["repoSize"], "kept": s["kept"],
        "headerSaved": s["headerSaved"],
        "restore": s["restore"],
    })
}
