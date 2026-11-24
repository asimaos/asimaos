//! Reset to a fresh installation of the current version (D7, D12, D13).
//!
//! There is no first-start setup (D10), so the fresh account is staged first. On encrypted disks
//! the new passphrase and a new recovery key are enrolled right away (the old secrets still work
//! until the reset runs). `ScheduleFactoryReset` marks the staged reset; the initramfs carries it
//! out at the next start (`asimaos-reset`): it removes every other key slot, deletes and recreates
//! the data subvolumes, keeps only the running version, creates the account, and TRIMs.

use crate::security;
use anyhow::{ensure, Context, Result};
use asimaos_svc::{cmd, system::DATA, Root};
use serde::Deserialize;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fresh {
    pub full_name: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub passphrase: String,
    #[serde(default)]
    pub current: String,
}

fn plan_path(root: &Root) -> std::path::PathBuf {
    root.path(DATA).join("flags/reset.json")
}

pub async fn prepare(root: &Root, fresh: Fresh) -> Result<Value> {
    ensure!(asimaos_svc::valid_username(&fresh.username), "The username isn't valid");
    ensure!(asimaos_svc::valid_fullname(&fresh.full_name) && !fresh.full_name.trim().is_empty(), "The name isn't valid");
    ensure!(fresh.password.chars().count() >= 4, "The password needs at least 4 characters");
    let hash =
        cmd::run_with("openssl", &["passwd", "-6", "-stdin"], Some(format!("{}\n", fresh.password).as_bytes()), &[])
            .await
            .context("The password could not be protected")?
            .stdout
            .trim()
            .to_string();

    let encrypted = root.path(&format!("/dev/mapper/{}", security::POOL_MAPPING)).exists();
    let (mut keep, mut recovery_key) = (Vec::<u32>::new(), String::new());
    if encrypted {
        ensure!(fresh.passphrase.chars().count() >= 8, "The disk passphrase needs at least 8 characters");
        let dev = security::luks_device().await?;
        let before = security::keyslots(&dev).await?;
        cmd::run_with(
            "systemd-cryptenroll",
            &["--password", &dev],
            None,
            &[("PASSWORD", fresh.current.as_str()), ("NEWPASSWORD", fresh.passphrase.as_str())],
        )
        .await
        .context("That passphrase or recovery key doesn't unlock this disk")?;
        let out = cmd::run_with(
            "systemd-cryptenroll",
            &["--recovery-key", &dev],
            None,
            &[("PASSWORD", fresh.current.as_str())],
        )
        .await?;
        recovery_key = security::find_recovery_key(&out.stdout).context("The new recovery key could not be read")?;
        let after = security::keyslots(&dev).await?;
        keep = after.iter().map(|s| s.0).filter(|s| !before.iter().any(|b| b.0 == *s)).collect();
        ensure!(keep.len() == 2, "The new disk keys could not be identified");
    }

    let plan = json!({
        "version": 1,
        "username": fresh.username,
        "fullName": fresh.full_name.trim(),
        "passwordHash": hash,
        "encrypted": encrypted,
        "keepSlots": keep,
        "scheduled": false,
    });
    write_plan(root, &plan)?;
    Ok(json!({ "recoveryKey": recovery_key, "username": fresh.username }))
}

fn write_plan(root: &Root, plan: &Value) -> Result<()> {
    let path = plan_path(root);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(plan)?)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Forgets a staged reset; on encrypted disks the newly added (never used) key slots go too.
pub async fn cancel(root: &Root) -> Result<()> {
    let path = plan_path(root);
    if let Ok(text) = std::fs::read_to_string(&path) {
        let plan: Value = serde_json::from_str(&text).unwrap_or_default();
        let slots: Vec<String> = plan["keepSlots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_u64())
            .map(|s| s.to_string())
            .collect();
        if plan["encrypted"].as_bool() == Some(true) && !slots.is_empty() {
            let dev = security::luks_device().await?;
            cmd::run("systemd-cryptenroll", &[&format!("--wipe-slot={}", slots.join(",")), &dev]).await?;
        }
        std::fs::remove_file(&path)?;
    }
    let _ = std::fs::remove_file(path.with_file_name("reset.plan"));
    Ok(())
}

/// Marks the staged reset to run at the next start, and writes `reset.plan` for the initramfs: the
/// ready-made account lines (from this version's factory /etc, as the installer does) and the key
/// slots to keep, as plain KEY=VALUE lines a shell can read without parsing JSON.
pub fn schedule(root: &Root) -> Result<()> {
    let path = plan_path(root);
    let text = std::fs::read_to_string(&path).context("Set up the fresh account first")?;
    let mut plan: Value = serde_json::from_str(&text)?;
    let user = plan["username"].as_str().context("broken reset plan")?.to_string();
    let name = plan["fullName"].as_str().unwrap_or("").to_string();
    let hash = plan["passwordHash"].as_str().context("broken reset plan")?.to_string();
    ensure!(asimaos_svc::valid_username(&user) && asimaos_svc::valid_fullname(&name), "broken reset plan");
    ensure!(hash.starts_with("$6$") && !hash.contains([':', '\n']), "broken reset plan");

    // First free uid from 1000 in this version's factory accounts (the fresh system's).
    let factory = std::fs::read_to_string(root.path("/usr/share/factory/etc/passwd")).unwrap_or_default();
    let used: Vec<u32> = factory.lines().filter_map(|l| l.split(':').nth(2)?.parse().ok()).collect();
    let uid = (1000..60000).find(|u| !used.contains(u)).context("no free user id")?;
    let keep: Vec<String> =
        plan["keepSlots"].as_array().into_iter().flatten().filter_map(|s| s.as_u64()).map(|s| s.to_string()).collect();

    let lines = [
        format!("USERNAME={user}"),
        format!("USER_ID={uid}"),
        format!("PASSWD_LINE={user}:x:{uid}:{uid}:{name}:/home/{user}:/bin/bash"),
        format!("SHADOW_LINE={user}:{hash}:{}:0:99999:7:::", asimaos_svc::today()),
        format!("ENCRYPTED={}", if plan["encrypted"] == true { "yes" } else { "no" }),
        format!("KEEP_SLOTS={}", keep.join(",")),
    ];
    let env_path = path.with_file_name("reset.plan");
    let tmp = env_path.with_extension("tmp");
    std::fs::write(&tmp, lines.join("\n") + "\n")?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, &env_path)?;

    plan["scheduled"] = Value::Bool(true);
    write_plan(root, &plan)
}
