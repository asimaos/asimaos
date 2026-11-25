//! asimaos-gamingd: the Asima Gaming service (io.github.asimaos.Gaming1 on the system bus).
//!
//! Gaming settings that need root: the handheld's power limit, the CPU scheduler (sched-ext,
//! through scx_loader) and RGB lighting. Status needs no password; changes are allowed for the
//! person at the computer (io.github.asimaos.gaming.device).
//!
//!   asimaos-gamingd                        system bus, polkit (D-Bus activation)
//!   asimaos-gamingd --apply-lighting [LED] put the saved colour back on a light (udev) or on all
//!   asimaos-gamingd --session --root DIR   test mode: session bus, no polkit, a scratch system tree

mod device;
mod dongle;
mod inputplumber;
mod lighting;
mod mouse;

use anyhow::Context;
use asimaos_svc::{polkit, Root};
use serde_json::json;
use std::path::PathBuf;
use tokio::sync::Mutex;
use zbus::{connection, fdo, interface, message::Header, object_server::SignalEmitter, zvariant, Connection};

const NAME: &str = "io.github.asimaos.Gaming1";
const PATH: &str = "/io/github/asimaos/Gaming1";

struct Gaming {
    root: Root,
    polkit: bool,
    lock: Mutex<()>,
}

fn failed(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

/// The gaming scheduler through scx_loader (org.scx.Loader), which owns sched-ext.
async fn scheduler(conn: &Connection, name: &str) -> anyhow::Result<()> {
    let (method, body): (&str, Option<(String, u32)>) = match name {
        "default" => ("StopScheduler", None),
        "lavd" | "bpfland" => ("SwitchScheduler", Some((format!("scx_{name}"), if name == "lavd" { 1 } else { 0 }))),
        _ => anyhow::bail!("Unknown scheduler"),
    };
    let dest = Some("org.scx.Loader");
    let r = match body {
        Some(b) => conn.call_method(dest, "/org/scx/Loader", Some("org.scx.Loader"), method, &b).await,
        None => conn.call_method(dest, "/org/scx/Loader", Some("org.scx.Loader"), method, &()).await,
    };
    r.map(|_| ()).context("The gaming scheduler service (scx_loader) is not available")
}

/// The chosen scheduler is also the one scx_loader starts at the next start-up (D44: the gaming
/// scheduler is the factory default; a choice made in Asima Gaming stays).
fn save_scheduler(root: &Root, name: &str) -> anyhow::Result<()> {
    let body = match name {
        "lavd" => "default_sched = \"scx_lavd\"\ndefault_mode = \"Gaming\"\n",
        "bpfland" => "default_sched = \"scx_bpfland\"\ndefault_mode = \"Auto\"\n",
        _ => "# The standard Linux scheduler: none of the sched_ext ones is started.\n",
    };
    let path = root.path("/etc/scx_loader/config.toml");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, format!("# Asima OS: the CPU scheduler chosen in Asima Gaming (D44).\n{body}"))
        .context("The scheduler choice could not be saved")
}

#[interface(name = "io.github.asimaos.Gaming1")]
impl Gaming {
    /// What this device can do and its current values, as JSON. No password needed.
    async fn status(&self, #[zbus(connection)] conn: &Connection) -> String {
        json!({
            "contract": asimaos_svc::CONTRACT,
            "managed": true,
            "device": device::status(&self.root),
            "lighting": lighting::status(&self.root),
            // The handheld's built-in controller through InputPlumber (D55), or null.
            "controller": inputplumber::status(&self.root, conn).await,
            // Xbox Wireless Adapters plugged in, and whether their firmware is here (D55).
            "dongles": dongle::status(&self.root),
        })
        .to_string()
    }

    /// area/key = handheld/tdp (int W), handheld/fan ("auto" | "max"), handheld/controller (the
    /// built-in controller as "deck" | "xbox-elite" | "ds5-edge"), controllers/dongleFirmware (an
    /// Xbox Wireless Adapter's product ID: its firmware from Microsoft), controllers/rumble (0–100 %),
    /// gaming/scheduler ("lavd" | "bpfland" | "default") or lighting/color ("#RRGGBB:percent", 0 %
    /// turns the lights off).
    async fn set(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        area: &str,
        key: &str,
        value: zvariant::Value<'_>,
    ) -> fdo::Result<()> {
        polkit::authorize(conn, &header, "io.github.asimaos.gaming.device", self.polkit).await?;
        let _g = self.lock.lock().await;
        match (area, key, &value) {
            ("handheld", "tdp", v) => {
                let watts = match v {
                    zvariant::Value::I32(w) => *w as i64,
                    zvariant::Value::I64(w) => *w,
                    zvariant::Value::U32(w) => *w as i64,
                    zvariant::Value::F64(w) => w.round() as i64,
                    _ => return Err(fdo::Error::InvalidArgs("Expected a number".into())),
                };
                device::set_tdp(&self.root, watts).map_err(failed)?;
            }
            ("handheld", "fan", zvariant::Value::Str(mode)) => {
                device::set_fan(&self.root, mode.as_str()).map_err(failed)?
            }
            ("handheld", "controller", zvariant::Value::Str(kind)) => {
                inputplumber::set_controller(&self.root, conn, kind.as_str()).await.map_err(failed)?
            }
            // The Xbox Wireless Adapter's firmware, from Microsoft (D55): the value is its product ID.
            ("controllers", "dongleFirmware", zvariant::Value::Str(pid)) => {
                dongle::check_pid(pid.as_str()).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
                dongle::get_firmware(&self.root, pid.as_str()).await.map_err(failed)?
            }
            ("controllers", "rumble", v) => {
                let pct = match v {
                    zvariant::Value::I32(n) => *n as i64,
                    zvariant::Value::I64(n) => *n,
                    zvariant::Value::U32(n) => *n as i64,
                    zvariant::Value::F64(n) => n.round() as i64,
                    _ => return Err(fdo::Error::InvalidArgs("Expected a number".into())),
                };
                device::set_rumble(&self.root, pct).map_err(failed)?;
            }
            // Signs the caller in by itself, into game mode (or stops that).
            ("gaming", "gameMode", zvariant::Value::Bool(on)) => {
                // Signing in by itself concerns the whole computer: the administrator's password.
                polkit::authorize(conn, &header, "io.github.asimaos.gaming.gamemode", self.polkit).await?;
                let user = if *on {
                    let sender =
                        header.sender().ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?.to_owned();
                    let uid = fdo::DBusProxy::new(conn).await?.get_connection_unix_user(sender.into()).await?;
                    let passwd = std::fs::read_to_string(self.root.path("/etc/passwd")).unwrap_or_default();
                    let name = passwd
                        .lines()
                        .map(|l| l.split(':').collect::<Vec<_>>())
                        .find(|f| f.len() >= 7 && f[2].parse::<u32>().ok() == Some(uid) && f[5].starts_with("/home/"))
                        .map(|f| f[0].to_string())
                        .ok_or_else(|| fdo::Error::Failed("Game mode is for a person's account".into()))?;
                    Some(name)
                } else {
                    None
                };
                device::set_game_mode(&self.root, user.as_deref()).map_err(failed)?;
            }
            ("gaming", "scheduler", zvariant::Value::Str(name)) => {
                scheduler(conn, name.as_str()).await.map_err(failed)?;
                save_scheduler(&self.root, name.as_str()).map_err(failed)?
            }
            ("lighting", "color", zvariant::Value::Str(t)) => {
                let bad = || fdo::Error::InvalidArgs("Expected #RRGGBB:percent".into());
                let (color, pct) = t.as_str().split_once(':').ok_or_else(bad)?;
                let pct: i64 = pct.parse().map_err(|_| bad())?;
                lighting::set(&self.root, color, pct).map_err(failed)?;
            }
            _ => return Err(fdo::Error::NotSupported(format!("{area}/{key} is not a setting of this service"))),
        }
        let _ = Self::changed(&emitter, area).await;
        Ok(())
    }

    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, area: &str) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    let (mut session, mut root, mut apply_lighting) = (false, Root::system(), None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--session" => session = true,
            // A game controller as mouse and keyboard (the user service asimaos-controller-mouse).
            "--controller-mouse" => return mouse::run(),
            // udev, as a game controller connects: the saved rumble strength.
            "--rumble-gain" => {
                let dev = args.next().context("--rumble-gain needs a device")?;
                anyhow::ensure!(dev.starts_with("/dev/input/event"), "not an input event device");
                return device::apply_rumble(&root, std::path::Path::new(&dev));
            }
            "--apply-lighting" => {
                apply_lighting = Some(args.next_if(|a| !a.starts_with("--")).map(PathBuf::from));
            }
            "--root" => root = Root(PathBuf::from(args.next().context("--root needs a directory")?)),
            "--version" => {
                println!("asimaos-gamingd {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    if let Some(led) = apply_lighting {
        let n = lighting::apply_saved(&root, led.as_deref())?;
        if n > 0 {
            println!("Asima Gaming: the saved colour is on {n} light(s)");
        }
        return Ok(());
    }
    anyhow::ensure!(!session || !root.is_system(), "--session needs --root (a scratch system tree)");
    let service = Gaming { root, polkit: !session, lock: Mutex::new(()) };
    let builder = if session { connection::Builder::session()? } else { connection::Builder::system()? };
    let _conn = builder.name(NAME)?.serve_at(PATH, service)?.build().await?;
    std::future::pending::<()>().await;
    Ok(())
}
