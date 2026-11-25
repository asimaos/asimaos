//! The handheld's built-in controller through InputPlumber (D55). InputPlumber combines the
//! handheld's buttons, sticks, back buttons, quick-access button and motion sensors into one
//! virtual controller; this module chooses which controller it shows to Steam and the games. As a
//! Steam Deck controller, Steam Input offers the back buttons, the quick-access button (Steam's
//! quick-access menu) and aiming with motion (gyro) in every game.
//!
//! InputPlumber manages only the handhelds its files list with `auto_manage` (those named
//! `50-*.yaml`); other controllers are left alone. The choice stays: a copy of InputPlumber's
//! file for this handheld in `/etc/inputplumber/devices.d` (read before its own files) with the
//! chosen controller, applied at once through InputPlumber's D-Bus interface. Choosing
//! InputPlumber's own default again removes the copy.

use anyhow::{Context, Result};
use asimaos_svc::Root;
use serde_json::{json, Value};
use std::path::PathBuf;
use zbus::{zvariant, Connection};

const DEST: &str = "org.shadowblip.InputPlumber";
const IFACE: &str = "org.shadowblip.Input.CompositeDevice";
const SHIPPED: &str = "/usr/share/inputplumber/devices";
const CHOSEN: &str = "/etc/inputplumber/devices.d";
/// The controllers Asima Gaming offers: a Steam Deck controller (everything in Steam Input), an
/// Xbox Elite controller (every game knows it) and a DualSense Edge (motion and touchpad for
/// games and emulators that use PlayStation controls).
pub const KINDS: [&str; 3] = ["deck", "xbox-elite", "ds5-edge"];

/// Whether an InputPlumber target is the game controller (not its mouse, keyboard or touch parts).
fn is_gamepad(target: &str) -> bool {
    !matches!(target, "mouse" | "keyboard" | "touchpad" | "touchscreen" | "dbus" | "debug" | "null")
}

/// A top-level `key: value` of an InputPlumber file, without quotes.
fn top_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
}

/// The file's `target_devices` list.
fn targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        if !inside {
            inside = l.trim_end() == "target_devices:";
            continue;
        }
        if l.starts_with(|c: char| !c.is_whitespace() && c != '#') {
            break;
        }
        if let Some(v) = l.trim().strip_prefix("- ") {
            out.push(v.trim().trim_matches('"').to_string());
        }
    }
    out
}

/// The file with its `target_devices` list replaced (or added).
fn with_targets(text: &str, list: &[String]) -> String {
    let block = |out: &mut String| {
        out.push_str("target_devices:\n");
        for t in list {
            out.push_str(&format!("  - {t}\n"));
        }
    };
    let mut out = String::from(
        "# Asima OS: the controller chosen in Asima Gaming → Handheld (a copy of InputPlumber's file for\n\
         # this handheld, which InputPlumber reads instead of its own).\n",
    );
    let (mut skipping, mut done) = (false, false);
    for l in text.lines() {
        if skipping {
            if l.trim().is_empty() || l.starts_with(char::is_whitespace) || l.starts_with('#') {
                continue;
            }
            skipping = false;
        }
        if !done && l.trim_end() == "target_devices:" {
            block(&mut out);
            (skipping, done) = (true, true);
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    if !done {
        out.push('\n');
        block(&mut out);
    }
    out
}

/// InputPlumber's file for the handheld with this name.
fn handheld_file(root: &Root, name: &str) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.path(SHIPPED)).ok()?.flatten().map(|e| e.path()).collect();
    files.sort();
    files.into_iter().find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("50-") && n.ends_with(".yaml"))
            && std::fs::read_to_string(p).is_ok_and(|t| top_value(&t, "name").as_deref() == Some(name))
    })
}

async fn name_of(conn: &Connection, path: &str) -> zbus::Result<String> {
    let reply =
        conn.call_method(Some(DEST), path, Some("org.freedesktop.DBus.Properties"), "Get", &(IFACE, "Name")).await?;
    let v: zvariant::OwnedValue = reply.body().deserialize()?;
    String::try_from(v).map_err(zbus::Error::from)
}

/// The built-in controller InputPlumber runs now: its D-Bus path, name and InputPlumber's file.
async fn builtin(root: &Root, conn: &Connection) -> Option<(String, String, PathBuf)> {
    for i in 0..16 {
        let path = format!("/org/shadowblip/InputPlumber/CompositeDevice{i}");
        match name_of(conn, &path).await {
            Ok(name) => {
                if let Some(file) = handheld_file(root, &name) {
                    return Some((path, name, file));
                }
            }
            // InputPlumber isn't running (or this isn't a handheld it knows).
            Err(zbus::Error::MethodError(e, _, _)) if e.as_str().ends_with("ServiceUnknown") => return None,
            Err(_) => {}
        }
    }
    None
}

/// The handheld's controller for Asima Gaming, or null when InputPlumber doesn't manage one here.
pub async fn status(root: &Root, conn: &Connection) -> Value {
    let Some((_, name, file)) = builtin(root, conn).await else { return Value::Null };
    let shipped = targets(&std::fs::read_to_string(&file).unwrap_or_default());
    let chosen =
        file.file_name().and_then(|n| std::fs::read_to_string(root.path(CHOSEN).join(n)).ok()).map(|t| targets(&t));
    let gamepad = |list: &[String]| list.iter().find(|t| is_gamepad(t)).cloned();
    json!({
        "name": name,
        "controller": gamepad(chosen.as_deref().unwrap_or(shipped.as_slice())),
        "default": gamepad(shipped.as_slice()),
        "kinds": KINDS,
    })
}

/// Shows the built-in controller as `kind` (one of KINDS), now and after every start.
pub async fn set_controller(root: &Root, conn: &Connection, kind: &str) -> Result<()> {
    anyhow::ensure!(KINDS.contains(&kind), "Unknown kind of controller");
    let (path, _, file) =
        builtin(root, conn).await.context("InputPlumber doesn't manage a built-in controller here")?;
    let text = std::fs::read_to_string(&file).context("InputPlumber's file for this handheld can't be read")?;
    let shipped = targets(&text);
    let mut list: Vec<String> = shipped.iter().filter(|t| !is_gamepad(t)).cloned().collect();
    list.insert(0, kind.to_string());
    let copy = root.path(CHOSEN).join(file.file_name().context("no file name")?);
    if shipped.iter().find(|t| is_gamepad(t)).is_some_and(|d| d == kind) {
        let _ = std::fs::remove_file(&copy);
        list = shipped;
    } else {
        std::fs::create_dir_all(copy.parent().unwrap())?;
        std::fs::write(&copy, with_targets(&text, &list)).context("The choice could not be saved")?;
    }
    conn.call_method(Some(DEST), path.as_str(), Some(IFACE), "SetTargetDevices", &(list,))
        .await
        .context("InputPlumber could not switch the controller")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Lenovo Legion Go\nversion: 1\nkind: CompositeDevice\nname: Lenovo Legion Go\n\
        source_devices:\n  - group: gamepad\noptions:\n  auto_manage: true\n\n\
        # The target input device(s) to emulate by default\ntarget_devices:\n  - xbox-elite\n  - mouse\n\
        \x20 - keyboard\n  #- touchscreen\n";

    #[test]
    fn reads_and_replaces_the_targets() {
        assert_eq!(top_value(FILE, "name").as_deref(), Some("Lenovo Legion Go"));
        assert_eq!(targets(FILE), ["xbox-elite", "mouse", "keyboard"]);
        let list: Vec<String> = ["deck", "mouse", "keyboard"].map(String::from).to_vec();
        let new = with_targets(FILE, &list);
        assert_eq!(targets(&new), list);
        assert!(new.contains("auto_manage: true") && new.starts_with("# Asima OS") && !new.contains("xbox-elite"));
        assert_eq!(targets(&with_targets("name: x\n", &list)), list);
    }

    #[test]
    fn finds_the_handhelds_file_only() {
        let t = std::env::temp_dir().join(format!("asimaos-ip-{}", std::process::id()));
        let dir = t.join("usr/share/inputplumber/devices");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("50-legion_go.yaml"), FILE).unwrap();
        std::fs::write(dir.join("60-ps5.yaml"), "name: \"Sony DualSense\"\n").unwrap();
        let root = Root(t.clone());
        assert!(handheld_file(&root, "Lenovo Legion Go").is_some_and(|p| p.ends_with("50-legion_go.yaml")));
        assert!(handheld_file(&root, "Sony DualSense").is_none());
        let _ = std::fs::remove_dir_all(t);
    }
}
