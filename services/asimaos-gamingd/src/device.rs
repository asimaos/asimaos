//! The handheld power limit through the kernel's own interfaces (sysfs): ASUS firmware
//! attributes (ROG Ally and others) or the AMD graphics driver's power cap (Steam Deck, many AMD
//! handhelds). The fan at full speed or automatic where the laptop's driver allows it (ASUS,
//! ThinkPad: hwmon pwm1_enable); quieter or faster fans otherwise come with the power mode, which
//! stays with power-profiles-daemon (one owner per setting). The controllers' rumble strength
//! (FF_GAIN), kept in /etc/asimaos/gaming.conf and set on every controller as it connects (udev).

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

/// Where the power limit can be set. Values in watts.
enum Tdp {
    Asus(PathBuf),   // …/firmware-attributes/asus-armoury/attributes
    AmdCap(PathBuf), // …/hwmon/hwmonN (power1_cap in microwatts)
}

fn tdp(root: &Root) -> Option<Tdp> {
    let asus = root.path("/sys/class/firmware-attributes/asus-armoury/attributes");
    if asus.join("ppt_pl1_spl/current_value").exists() {
        return Some(Tdp::Asus(asus));
    }
    for card in entries(&root.path("/sys/class/drm")) {
        if read(&card.join("device/vendor")).as_deref() != Some("0x1002") {
            continue;
        }
        for hw in entries(&card.join("device/hwmon")) {
            if hw.join("power1_cap").exists() && hw.join("power1_cap_max").exists() {
                return Some(Tdp::AmdCap(hw));
            }
        }
    }
    None
}

fn tdp_range(t: &Tdp) -> (i64, i64, i64) {
    let num = |p: PathBuf| read(&p).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    match t {
        Tdp::Asus(a) => (
            num(a.join("ppt_pl1_spl/min_value")),
            num(a.join("ppt_pl1_spl/max_value")),
            num(a.join("ppt_pl1_spl/current_value")),
        ),
        Tdp::AmdCap(h) => (
            num(h.join("power1_cap_min")) / 1_000_000,
            num(h.join("power1_cap_max")) / 1_000_000,
            num(h.join("power1_cap")) / 1_000_000,
        ),
    }
}

pub fn set_tdp(root: &Root, watts: i64) -> Result<()> {
    let t = tdp(root).context("This device's power limit can't be changed")?;
    let (min, max, _) = tdp_range(&t);
    ensure!(watts >= min.max(3) && watts <= max, "Choose between {} and {max} W", min.max(3));
    match t {
        // Steady power: the sustained, short and fast limits all follow the chosen watts.
        Tdp::Asus(a) => {
            for attr in ["ppt_pl1_spl", "ppt_pl2_sppt", "ppt_fppt"] {
                let path = a.join(attr).join("current_value");
                if path.exists() {
                    fs::write(&path, watts.to_string()).with_context(|| format!("{attr} refused {watts} W"))?;
                }
            }
        }
        Tdp::AmdCap(h) => fs::write(h.join("power1_cap"), (watts * 1_000_000).to_string())
            .context("the graphics driver refused the limit")?,
    }
    Ok(())
}

pub fn status(root: &Root) -> Value {
    let tdp = tdp(root).map(|t| {
        let (min, max, cur) = tdp_range(&t);
        json!({ "min": min.max(3), "max": max, "current": cur, "method": match t { Tdp::Asus(_) => "asus", Tdp::AmdCap(_) => "amdgpu" } })
    });
    let fan = fan_control(root).map(|f| {
        json!({ "modes": ["auto", "max"], "current": if read(&f).as_deref() == Some("0") { "max" } else { "auto" } })
    });
    json!({ "tdp": tdp, "fan": fan, "rumble": saved_rumble(root).unwrap_or(100), "gameMode": game_mode_user(root) })
}

// ───────────────────────────────────────────────────────────────────────── game mode

/// The login manager's drop-in that signs one person in by itself, into the game-mode session.
pub const GAME_MODE_CONF: &str = "/etc/plasmalogin.conf.d/90-asimaos-game-mode.conf";

/// Whose game mode starts by itself, if anyone's.
pub fn game_mode_user(root: &Root) -> Option<String> {
    fs::read_to_string(root.path(GAME_MODE_CONF)).ok()?.lines().find_map(|l| l.strip_prefix("User=").map(String::from))
}

/// Start in game mode: `user` is signed in by itself at start-up, straight into Steam's
/// full-screen session; None turns that off. Leaving Steam brings the login screen (no relogin).
pub fn set_game_mode(root: &Root, user: Option<&str>) -> Result<()> {
    let path = root.path(GAME_MODE_CONF);
    match user {
        Some(name) => {
            ensure!(asimaos_svc::valid_username(name), "That isn't an account name");
            fs::create_dir_all(path.parent().context("bad path")?)?;
            fs::write(
                &path,
                format!(
                    "# Asima OS: Asima Gaming → Start in game mode. {name} is signed in by itself into\n\
                     # Steam's full-screen session; leaving Steam brings the login screen.\n\
                     [Autologin]\nUser={name}\nSession=asimaos-game-mode\nRelogin=false\n"
                ),
            )
            .context("Game mode could not be switched on")
        }
        None => match fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).context("Game mode could not be switched off"),
            _ => Ok(()),
        },
    }
}

// ───────────────────────────────────────────────────────────────────────── fan

/// The fan switch of a laptop driver that has one: pwm1_enable, 0 = full speed, 2 = automatic.
fn fan_control(root: &Root) -> Option<PathBuf> {
    entries(&root.path("/sys/class/hwmon")).into_iter().find_map(|hw| {
        let name = read(&hw.join("name"))?;
        let f = hw.join("pwm1_enable");
        (matches!(name.as_str(), "asus" | "thinkpad") && f.exists()).then_some(f)
    })
}

pub fn set_fan(root: &Root, mode: &str) -> Result<()> {
    let f = fan_control(root).context("This computer's fan can't be set from Asima OS")?;
    let value = match mode {
        "auto" => "2",
        "max" => "0",
        _ => anyhow::bail!("The fan is auto or max"),
    };
    fs::write(&f, value).context("The fan setting could not be changed")
}

// ───────────────────────────────────────────────────────────────────────── rumble

pub const GAMING_CONF: &str = "/etc/asimaos/gaming.conf";

fn saved_rumble(root: &Root) -> Option<i64> {
    fs::read_to_string(root.path(GAMING_CONF))
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("RUMBLE=")?.trim().parse().ok())
}

/// Sets the rumble strength of one controller (FF_GAIN, 0–0xFFFF) through its event device.
fn gain(dev: &Path, percent: i64) -> Result<()> {
    use std::io::Write;
    const EV_FF: u16 = 0x15;
    const FF_GAIN: u16 = 0x60;
    let value = (0xFFFF * percent.clamp(0, 100) / 100) as i32;
    let mut event = [0u8; 24]; // struct input_event: a timeval (zero), type, code, value
    event[16..18].copy_from_slice(&EV_FF.to_ne_bytes());
    event[18..20].copy_from_slice(&FF_GAIN.to_ne_bytes());
    event[20..24].copy_from_slice(&value.to_ne_bytes());
    let mut f =
        fs::OpenOptions::new().write(true).open(dev).with_context(|| format!("{} can't be opened", dev.display()))?;
    f.write_all(&event).context("This controller doesn't take a rumble strength")
}

/// Saves the rumble strength and sets it on every game controller connected now.
pub fn set_rumble(root: &Root, percent: i64) -> Result<()> {
    ensure!((0..=100).contains(&percent), "The rumble strength is 0–100 %");
    let path = root.path(GAMING_CONF);
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_else(|_| "# Asima OS: Asima Gaming's settings for every controller.\n".into())
        .lines()
        .filter(|l| !l.starts_with("RUMBLE="))
        .map(String::from)
        .collect();
    lines.push(format!("RUMBLE={percent}"));
    fs::create_dir_all(path.parent().context("bad path")?)?;
    fs::write(&path, lines.join("\n") + "\n")?;
    let devices = fs::read_to_string(root.path("/proc/bus/input/devices")).unwrap_or_default();
    for pad in crate::mouse::pads(&devices) {
        let _ = gain(&root.path(&format!("/dev/input/{}", pad.event)), percent);
    }
    Ok(())
}

/// udev, as a controller connects (--rumble-gain DEVICE): the saved strength, when one is saved.
pub fn apply_rumble(root: &Root, dev: &Path) -> Result<()> {
    match saved_rumble(root) {
        Some(p) => gain(dev, p),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_limit() {
        let dir = std::env::temp_dir().join(format!("gd-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        let asus = dir.join("sys/class/firmware-attributes/asus-armoury/attributes");
        for a in ["ppt_pl1_spl", "ppt_pl2_sppt", "ppt_fppt"] {
            fs::create_dir_all(asus.join(a)).unwrap();
            fs::write(asus.join(a).join("current_value"), "17\n").unwrap();
            fs::write(asus.join(a).join("min_value"), "7\n").unwrap();
            fs::write(asus.join(a).join("max_value"), "30\n").unwrap();
        }
        let root = Root(dir.clone());
        set_tdp(&root, 25).unwrap();
        assert_eq!(read(&asus.join("ppt_fppt/current_value")).unwrap(), "25");
        assert!(set_tdp(&root, 40).is_err());
        assert_eq!(status(&root)["tdp"]["max"], 30);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn fan_and_rumble() {
        let dir = std::env::temp_dir().join(format!("gd-fan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = Root(dir.clone());
        assert!(status(&root)["fan"].is_null() && set_fan(&root, "max").is_err());
        let hw = dir.join("sys/class/hwmon/hwmon3");
        fs::create_dir_all(&hw).unwrap();
        fs::write(hw.join("name"), "asus\n").unwrap();
        fs::write(hw.join("pwm1_enable"), "2\n").unwrap();
        assert_eq!(status(&root)["fan"]["current"], "auto");
        set_fan(&root, "max").unwrap();
        assert_eq!(status(&root)["fan"]["current"], "max");
        assert!(set_fan(&root, "loud").is_err());
        // Rumble: saved, set on each controller (here a plain file stands in for the device).
        fs::create_dir_all(dir.join("proc/bus/input")).unwrap();
        fs::write(dir.join("proc/bus/input/devices"),
            "I: Bus=0003 Vendor=045e Product=028e Version=0114\nN: Name=\"Microsoft X-Box 360 pad\"\nH: Handlers=event5 js0\nB: EV=20000b\nB: KEY=7cdb000000000000 0 0 0 0\nB: ABS=3003f\n\n").unwrap();
        fs::create_dir_all(dir.join("dev/input")).unwrap();
        fs::write(dir.join("dev/input/event5"), "").unwrap();
        set_rumble(&root, 50).unwrap();
        assert_eq!(status(&root)["rumble"], 50);
        let ev = fs::read(dir.join("dev/input/event5")).unwrap();
        assert_eq!(ev.len(), 24);
        assert_eq!(u16::from_ne_bytes([ev[16], ev[17]]), 0x15);
        assert_eq!(i32::from_ne_bytes([ev[20], ev[21], ev[22], ev[23]]), 0xFFFF * 50 / 100);
        assert!(set_rumble(&root, 120).is_err());
        // Game mode: one person signed in by itself into the Steam session, then off again.
        set_game_mode(&root, Some("asha")).unwrap();
        assert_eq!(status(&root)["gameMode"], "asha");
        assert!(fs::read_to_string(root.path(GAME_MODE_CONF))
            .unwrap()
            .contains("Session=asimaos-game-mode\nRelogin=false"));
        assert!(set_game_mode(&root, Some("Root!")).is_err());
        set_game_mode(&root, None).unwrap();
        assert!(status(&root)["gameMode"].is_null());
        set_game_mode(&root, None).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
