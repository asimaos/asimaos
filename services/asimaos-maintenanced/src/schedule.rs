//! How often the automatic snapshots of /home and the settings are taken: every day
//! (the default), every week, every month or never. The nightly timer always runs; it only takes a
//! snapshot when one is due. "Snapshot now" ignores the schedule.

use anyhow::{bail, Result};
use asimaos_svc::Root;
use std::fs;

pub const CONF: &str = "/etc/asimaos/snapshots.conf";
pub const CHOICES: [&str; 4] = ["daily", "weekly", "monthly", "off"];

pub fn get(root: &Root) -> String {
    fs::read_to_string(root.path(CONF))
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("SCHEDULE=").map(|v| v.trim().to_string())))
        .filter(|v| CHOICES.contains(&v.as_str()))
        .unwrap_or_else(|| "daily".into())
}

pub fn set(root: &Root, schedule: &str) -> Result<()> {
    if !CHOICES.contains(&schedule) {
        bail!("Choose daily, weekly, monthly or off");
    }
    let path = root.path(CONF);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        format!("# Asima OS: automatic snapshots (Asima Maintenance → Backups).\nSCHEDULE={schedule}\n"),
    )?;
    Ok(())
}

/// Is a snapshot due, given the days since the newest one?
pub fn due(schedule: &str, days_since_newest: Option<i64>) -> bool {
    let Some(days) = days_since_newest else { return schedule != "off" };
    match schedule {
        "off" => false,
        "weekly" => days >= 7,
        "monthly" => days >= 30,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules() {
        assert!(due("daily", Some(0)) && due("weekly", None) && !due("off", None));
        assert!(
            !due("weekly", Some(6)) && due("weekly", Some(7)) && !due("monthly", Some(29)) && due("monthly", Some(31))
        );
        let t = std::env::temp_dir().join(format!("asimaos-schedule-{}", std::process::id()));
        let root = Root(t.clone());
        assert_eq!(get(&root), "daily");
        set(&root, "weekly").unwrap();
        assert_eq!(get(&root), "weekly");
        assert!(set(&root, "hourly").is_err());
        let _ = fs::remove_dir_all(t);
    }
}
