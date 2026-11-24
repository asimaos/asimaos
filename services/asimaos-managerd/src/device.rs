//! Device settings through the kernel's own interfaces (sysfs), so they work the same on every
//! machine that has the hardware: the battery's charge limit. Fan and power modes stay with
//! power-profiles-daemon (one owner per setting); the handheld power limit and RGB lighting belong
//! to Asima Gaming (asimaos-gamingd).

use anyhow::{ensure, Context, Result};
use asimaos_svc::Root;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn read(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

// ───────────────────────────────────────────────────────────────────────── battery charge limit

/// The chosen limit, put back at every start (`asimaos-managerd --apply-battery`): some laptops forget
/// it when they restart.
pub const BATTERY_CONF: &str = "/etc/asimaos/battery.conf";

/// How this laptop limits its charge: the first of the kernel's interfaces that exists.
enum Method {
    /// A percentage: `charge_control_end_threshold` on the battery (ThinkPad, ASUS, Dell, …).
    Percent(Vec<PathBuf>),
    /// Fixed battery care, on or off, at a level the laptop decides (about 60–80 %): the battery's
    /// `charge_types` (or `charge_type`) offering "Long Life" (newer kernels: Lenovo IdeaPad and
    /// Legion, and others).
    LongLife(Vec<PathBuf>),
    /// The same through the older Lenovo ideapad driver: `conservation_mode` (0 or 1).
    Conservation(PathBuf),
}

fn all_batteries(root: &Root) -> Vec<PathBuf> {
    entries(&root.path("/sys/class/power_supply"))
        .into_iter()
        .filter(|p| read(&p.join("type")).as_deref() == Some("Battery"))
        .collect()
}

/// The battery's charge-type file that offers "Long Life", if any.
fn long_life_file(bat: &Path) -> Option<PathBuf> {
    ["charge_types", "charge_type"].iter().map(|f| bat.join(f)).find(|f| {
        read(f).is_some_and(|s| {
            let s = s.to_lowercase();
            s.contains("long life") || s.contains("long_life")
        })
    })
}

fn method(root: &Root) -> Option<Method> {
    let bats = all_batteries(root);
    let percent: Vec<PathBuf> =
        bats.iter().filter(|b| b.join("charge_control_end_threshold").exists()).cloned().collect();
    if !percent.is_empty() {
        return Some(Method::Percent(percent));
    }
    let long: Vec<PathBuf> = bats.iter().filter_map(|b| long_life_file(b)).collect();
    if !long.is_empty() {
        return Some(Method::LongLife(long));
    }
    entries(&root.path("/sys/bus/platform/drivers/ideapad_acpi"))
        .into_iter()
        .map(|d| d.join("conservation_mode"))
        .find(|f| f.exists())
        .map(Method::Conservation)
}

/// `percent` below 100 turns battery care on (at that level where the laptop allows a level), 100
/// turns it off. The choice is saved so the next start puts it back.
pub fn set_charge_limit(root: &Root, percent: i64) -> Result<()> {
    apply_charge_limit(root, percent)?;
    let conf = root.path(BATTERY_CONF);
    fs::create_dir_all(conf.parent().unwrap())?;
    fs::write(
        &conf,
        format!("# Asima Manager: the battery's charge limit, put back at every start.\nLIMIT={percent}\n"),
    )?;
    Ok(())
}

fn apply_charge_limit(root: &Root, percent: i64) -> Result<()> {
    ensure!((50..=100).contains(&percent), "The charge limit must be between 50 and 100 %");
    let on = percent < 100;
    match method(root) {
        Some(Method::Percent(bats)) => {
            for b in bats {
                fs::write(b.join("charge_control_end_threshold"), percent.to_string())
                    .with_context(|| format!("{} refused the limit", b.display()))?;
            }
        }
        Some(Method::LongLife(files)) => {
            for f in files {
                let wanted = if on { ["Long Life", "Long_Life"] } else { ["Standard", "Standard"] };
                ensure!(wanted.iter().any(|w| fs::write(&f, w).is_ok()), "{} refused battery care", f.display());
            }
        }
        Some(Method::Conservation(f)) => {
            fs::write(&f, if on { "1" } else { "0" })
                .with_context(|| format!("{} refused battery care", f.display()))?;
        }
        None => anyhow::bail!("This battery can't limit its charge"),
    }
    Ok(())
}

/// At start-up: the saved limit back on the battery (asimaos-battery.service). Nothing saved, nothing
/// done.
pub fn apply_saved(root: &Root) -> Result<()> {
    let Some(text) = read(&root.path(BATTERY_CONF)) else { return Ok(()) };
    let limit = text
        .lines()
        .find_map(|l| l.strip_prefix("LIMIT="))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .context("the saved charge limit can't be read")?;
    apply_charge_limit(root, limit)
}

pub fn status(root: &Root) -> Value {
    // value: the limit in %, 100 when battery care is off. fixed: the laptop decides the level (a
    // switch, no slider).
    let (supported, value, fixed) = match method(root) {
        Some(Method::Percent(bats)) => (
            true,
            bats.first()
                .and_then(|b| read(&b.join("charge_control_end_threshold")))
                .and_then(|s| s.parse::<i64>().ok()),
            false,
        ),
        Some(Method::LongLife(files)) => {
            let on = files.first().and_then(|f| read(f)).is_some_and(|s| {
                let s = s.to_lowercase();
                // "charge_types" brackets the active one; "charge_type" holds only it.
                s.contains("[long life]") || s.contains("[long_life]") || s == "long life" || s == "long_life"
            });
            (true, Some(if on { 80 } else { 100 }), true)
        }
        Some(Method::Conservation(f)) => (true, Some(if read(&f).as_deref() == Some("1") { 80 } else { 100 }), true),
        None => (false, None, false),
    };
    json!({ "chargeLimit": { "supported": supported, "value": value, "fixed": fixed } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Root, PathBuf) {
        let dir = std::env::temp_dir().join(format!("md-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        let bat = dir.join("sys/class/power_supply/BAT0");
        fs::create_dir_all(&bat).unwrap();
        fs::write(bat.join("type"), "Battery\n").unwrap();
        fs::write(bat.join("charge_control_end_threshold"), "100\n").unwrap();
        (Root(dir.clone()), dir)
    }

    #[test]
    fn charge_limit() {
        let (root, dir) = tree();
        set_charge_limit(&root, 80).unwrap();
        assert!(set_charge_limit(&root, 20).is_err());
        assert_eq!(status(&root)["chargeLimit"]["value"], 80);
        assert_eq!(status(&root)["chargeLimit"]["fixed"], false);
        assert!(fs::read_to_string(root.path(BATTERY_CONF)).unwrap().contains("LIMIT=80"));
        // Forgotten at a restart: the saved limit goes back on.
        fs::write(dir.join("sys/class/power_supply/BAT0/charge_control_end_threshold"), "100\n").unwrap();
        apply_saved(&root).unwrap();
        assert_eq!(status(&root)["chargeLimit"]["value"], 80);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lenovo_battery_care() {
        // Newer kernels: "Long Life" among the battery's charge types.
        let dir = std::env::temp_dir().join(format!("md-ll-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let bat = dir.join("sys/class/power_supply/BAT0");
        fs::create_dir_all(&bat).unwrap();
        fs::write(bat.join("type"), "Battery\n").unwrap();
        fs::write(bat.join("charge_types"), "[Standard] Long Life\n").unwrap();
        let root = Root(dir.clone());
        assert_eq!(status(&root)["chargeLimit"]["fixed"], true);
        assert_eq!(status(&root)["chargeLimit"]["value"], 100);
        set_charge_limit(&root, 80).unwrap();
        assert_eq!(fs::read_to_string(bat.join("charge_types")).unwrap(), "Long Life");
        // Older kernels: the ideapad driver's conservation mode.
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&bat).unwrap();
        fs::write(bat.join("type"), "Battery\n").unwrap();
        let vpc = dir.join("sys/bus/platform/drivers/ideapad_acpi/VPC2004:00");
        fs::create_dir_all(&vpc).unwrap();
        fs::write(vpc.join("conservation_mode"), "0\n").unwrap();
        assert_eq!(status(&root)["chargeLimit"]["supported"], true);
        set_charge_limit(&root, 80).unwrap();
        assert_eq!(fs::read_to_string(vpc.join("conservation_mode")).unwrap(), "1");
        assert_eq!(status(&root)["chargeLimit"]["value"], 80);
        set_charge_limit(&root, 100).unwrap();
        assert_eq!(status(&root)["chargeLimit"]["value"], 100);
        let _ = fs::remove_dir_all(dir);
    }
}
