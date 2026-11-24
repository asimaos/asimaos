//! RGB lighting through the kernel's LED class (sysfs), so it works the same on every device the
//! kernel drives: a DualSense light bar, a handheld's joystick rings, RGB keyboards with a kernel
//! driver. Most PCs have none; then Asima Gaming shows no Lighting page at all.
//!
//! The chosen colour is kept in /etc/asimaos/lighting.conf and put back by
//! `asimaos-gamingd --apply-lighting` when a light appears (udev: at start-up and when a
//! controller connects).

use anyhow::{ensure, Context, Result};
use asimaos_svc::Root;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const CONF: &str = "/etc/asimaos/lighting.conf";

fn read(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

/// A multicolour LED: `multi_index` names its red, green and blue channels.
fn is_rgb(led: &Path) -> bool {
    read(&led.join("multi_index"))
        .is_some_and(|i| ["red", "green", "blue"].iter().all(|c| i.split_whitespace().any(|x| x == *c)))
}

fn rgb_leds(root: &Root) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(root.path("/sys/class/leds"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| is_rgb(p)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn parse_color(c: &str) -> Result<(u32, u32, u32)> {
    let hex = c.strip_prefix('#').unwrap_or(c);
    ensure!(hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()), "Colours look like #2F80ED");
    let v = u32::from_str_radix(hex, 16).context("Colours look like #2F80ED")?;
    Ok((v >> 16 & 0xff, v >> 8 & 0xff, v & 0xff))
}

fn max_brightness(led: &Path) -> u32 {
    read(&led.join("max_brightness")).and_then(|s| s.parse().ok()).unwrap_or(255).max(1)
}

fn apply(led: &Path, (r, g, b): (u32, u32, u32), brightness: u32) -> Result<()> {
    let max = max_brightness(led);
    let intensity: Vec<String> = read(&led.join("multi_index"))
        .unwrap_or_default()
        .split_whitespace()
        .map(|ch| match ch {
            "red" => r,
            "green" => g,
            "blue" => b,
            _ => 0,
        })
        .map(|v| (v * max / 255).to_string())
        .collect();
    fs::write(led.join("multi_intensity"), intensity.join(" "))
        .with_context(|| format!("{} refused the colour", led.display()))?;
    fs::write(led.join("brightness"), (brightness * max / 100).to_string())
        .with_context(|| format!("{} refused the brightness", led.display()))?;
    Ok(())
}

/// The saved choice: (colour, brightness %).
fn saved(root: &Root) -> Option<(String, u32)> {
    let text = fs::read_to_string(root.path(CONF)).ok()?;
    let get = |k: &str| text.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()));
    let color = get("COLOR=")?;
    parse_color(&color).ok()?;
    let brightness = get("BRIGHTNESS=")?.parse::<u32>().ok().filter(|b| *b <= 100)?;
    Some((color.to_uppercase(), brightness))
}

/// Sets every RGB light to `color` at `brightness` percent (0 turns them off) and keeps the
/// choice for lights that appear later.
pub fn set(root: &Root, color: &str, brightness: i64) -> Result<usize> {
    let rgb = parse_color(color)?;
    ensure!((0..=100).contains(&brightness), "Brightness is 0–100 %");
    let leds = rgb_leds(root);
    ensure!(!leds.is_empty(), "No lights that Asima OS can control were found");
    for led in &leds {
        apply(led, rgb, brightness as u32)?;
    }
    let path = root.path(CONF);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(
        &path,
        format!(
            "# RGB lighting, written by Asima Gaming.\nCOLOR=#{}\nBRIGHTNESS={brightness}\n",
            color.trim_start_matches('#').to_uppercase()
        ),
    )?;
    Ok(leds.len())
}

/// Puts the saved colour back on one light (`led`, a /sys/class/leds path from udev) or on all.
/// Without a saved choice the lights keep the firmware's own colour.
pub fn apply_saved(root: &Root, led: Option<&Path>) -> Result<usize> {
    let Some((color, brightness)) = saved(root) else { return Ok(0) };
    let rgb = parse_color(&color)?;
    let leds = match led {
        Some(l) => {
            ensure!(is_rgb(l), "{} is not an RGB light", l.display());
            vec![l.to_path_buf()]
        }
        None => rgb_leds(root),
    };
    for l in &leds {
        apply(l, rgb, brightness)?;
    }
    Ok(leds.len())
}

/// "input5:rgb:indicator" → the device's own name if the kernel gives one, and what the light is.
fn describe(led: &Path) -> (String, String) {
    let id = led.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let function = id.rsplit(':').next().unwrap_or("").replace('_', " ");
    let function = match function.as_str() {
        "indicator" => "Light bar".to_string(),
        "" => "Light".to_string(),
        f => {
            let mut c = f.chars();
            c.next().map(|x| x.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    };
    let device = read(&led.join("device/name"))
        .or_else(|| read(&led.join("device/device/name")))
        .unwrap_or_else(|| id.split(':').next().unwrap_or("").to_string());
    (function, device)
}

pub fn status(root: &Root) -> Value {
    let leds = rgb_leds(root);
    let lights: Vec<Value> = leds
        .iter()
        .map(|l| {
            let (name, device) = describe(l);
            json!({ "id": l.file_name().map(|n| n.to_string_lossy().into_owned()), "name": name, "device": device })
        })
        .collect();
    // What the lights show: the saved choice, else what the first light shows now.
    let (color, brightness) = saved(root).unwrap_or_else(|| {
        leds.first()
            .map(|l| {
                let max = max_brightness(l);
                let index = read(&l.join("multi_index")).unwrap_or_default();
                let values = read(&l.join("multi_intensity")).unwrap_or_default();
                let mut rgb = [0u32; 3];
                for (ch, v) in index.split_whitespace().zip(values.split_whitespace()) {
                    let v = v.parse::<u32>().unwrap_or(0) * 255 / max;
                    match ch {
                        "red" => rgb[0] = v.min(255),
                        "green" => rgb[1] = v.min(255),
                        "blue" => rgb[2] = v.min(255),
                        _ => {}
                    }
                }
                let b = read(&l.join("brightness")).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                (format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]), (b * 100 / max).min(100))
            })
            .unwrap_or_else(|| ("#2F80ED".into(), 0))
    });
    json!({ "lights": lights, "color": color, "brightness": brightness })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Root, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gl-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&dir);
        let led = dir.join("sys/class/leds/input5:rgb:indicator");
        fs::create_dir_all(led.join("device")).unwrap();
        fs::write(led.join("device/name"), "DualSense Wireless Controller\n").unwrap();
        fs::write(led.join("multi_index"), "red green blue\n").unwrap();
        fs::write(led.join("max_brightness"), "255\n").unwrap();
        fs::write(led.join("multi_intensity"), "0 0 255\n").unwrap();
        fs::write(led.join("brightness"), "255\n").unwrap();
        // A single-colour LED is not ours.
        let caps = dir.join("sys/class/leds/input3::capslock");
        fs::create_dir_all(&caps).unwrap();
        fs::write(caps.join("brightness"), "0\n").unwrap();
        (Root(dir.clone()), dir, led)
    }

    #[test]
    fn set_and_keep() {
        let (root, dir, led) = tree();
        let s = status(&root);
        assert_eq!(s["lights"].as_array().unwrap().len(), 1);
        assert_eq!(s["lights"][0]["name"], "Light bar");
        assert_eq!(s["lights"][0]["device"], "DualSense Wireless Controller");
        assert_eq!(s["color"], "#0000FF"); // what the light shows now
        assert_eq!(set(&root, "#2f80ed", 50).unwrap(), 1);
        assert_eq!(read(&led.join("multi_intensity")).unwrap(), "47 128 237");
        assert_eq!(read(&led.join("brightness")).unwrap(), "127");
        assert_eq!(status(&root)["color"], "#2F80ED");
        assert_eq!(status(&root)["brightness"], 50);
        assert!(set(&root, "blue", 50).is_err());
        assert!(set(&root, "#2F80ED", 101).is_err());
        // The controller reconnects with its own colour: the saved one comes back.
        fs::write(led.join("multi_intensity"), "0 0 255\n").unwrap();
        assert_eq!(apply_saved(&root, Some(&led)).unwrap(), 1);
        assert_eq!(read(&led.join("multi_intensity")).unwrap(), "47 128 237");
        assert!(apply_saved(&root, Some(&dir.join("sys/class/leds/input3::capslock"))).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_saved_changes_nothing() {
        let (root, dir, led) = tree();
        assert_eq!(apply_saved(&root, None).unwrap(), 0);
        assert_eq!(read(&led.join("multi_intensity")).unwrap(), "0 0 255");
        let _ = fs::remove_dir_all(dir);
    }
}
