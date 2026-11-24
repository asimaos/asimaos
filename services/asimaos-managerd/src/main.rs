//! asimaos-managerd: the Asima Manager service (io.github.asimaos.Manager1 on the system bus).
//!
//! The settings of this computer that need root: the battery's charge limit, where face sign-in
//! may be used, and Asima Shield (the firewall). Status needs no password. Device settings are
//! allowed for the person at the computer (io.github.asimaos.manager.device); the firewall asks for the
//! administrator's password. RGB lighting is Asima Gaming's (asimaos-gamingd); Android apps are
//! set up by Asima Dev through two system units (D34).
//!
//!   asimaos-managerd                        system bus, polkit (D-Bus activation)
//!   asimaos-managerd --apply-shield         apply the saved firewall zone (asimaos-shield.service, at boot)
//!   asimaos-managerd --session --root DIR   test mode: session bus, no polkit, a scratch system tree

mod device;
mod shield;

use anyhow::Context;
use asimaos_svc::{cmd, polkit, Root};
use serde_json::json;
use std::path::PathBuf;
use tokio::sync::Mutex;
use zbus::{connection, fdo, interface, message::Header, object_server::SignalEmitter, zvariant, Connection};

const NAME: &str = "io.github.asimaos.Manager1";
const PATH: &str = "/io/github/asimaos/Manager1";
/// Where face sign-in may be used, and whether a normal webcam may; read by pam_asimaos_face and
/// asimaos-faced.
const FACE_CONF: &str = "/etc/asimaos/face.conf";

struct Manager {
    root: Root,
    polkit: bool,
    lock: Mutex<()>,
}

fn failed(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

/// LOGIN=no or UNLOCK=no in face.conf switches that use off; anything else leaves it on.
fn face_use(root: &Root, key: &str) -> bool {
    std::fs::read_to_string(root.path(FACE_CONF)).unwrap_or_default().lines().all(|l| l.trim() != format!("{key}=no"))
}

/// WEBCAM=yes switches a normal webcam on for face unlock; it is off otherwise (D37).
fn face_on(root: &Root, key: &str) -> bool {
    std::fs::read_to_string(root.path(FACE_CONF)).unwrap_or_default().lines().any(|l| l.trim() == format!("{key}=yes"))
}

#[interface(name = "io.github.asimaos.Manager1")]
impl Manager {
    /// What this device can do and its current values, as JSON. No password needed.
    async fn status(&self) -> String {
        let s = shield::load(&self.root);
        json!({
            "contract": asimaos_svc::CONTRACT,
            "managed": true,
            "shield": { "zone": s.zone, "killSwitch": s.kill_switch, "available": cmd::exists("nft") },
            "device": device::status(&self.root),
            "biometrics": {
                "useLogin": face_use(&self.root, "LOGIN"),
                "useUnlock": face_use(&self.root, "UNLOCK"),
                "useWebcam": face_on(&self.root, "WEBCAM"),
            },
        })
        .to_string()
    }

    /// Device settings: area/key = power/chargeLimit (int %), biometrics/useLogin, useUnlock or
    /// useWebcam (bool; a normal webcam for face unlock is off until switched on, D37).
    async fn set(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        area: &str,
        key: &str,
        value: zvariant::Value<'_>,
    ) -> fdo::Result<()> {
        polkit::authorize(conn, &header, "io.github.asimaos.manager.device", self.polkit).await?;
        let _g = self.lock.lock().await;
        let int = || -> fdo::Result<i64> {
            match &value {
                zvariant::Value::I32(v) => Ok(*v as i64),
                zvariant::Value::I64(v) => Ok(*v),
                zvariant::Value::U32(v) => Ok(*v as i64),
                zvariant::Value::F64(v) => Ok(v.round() as i64),
                _ => Err(fdo::Error::InvalidArgs("Expected a number".into())),
            }
        };
        match (area, key) {
            ("power", "chargeLimit") => device::set_charge_limit(&self.root, int()?).map_err(failed)?,
            ("biometrics", k @ ("useLogin" | "useUnlock" | "useWebcam")) => {
                let on = matches!(value, zvariant::Value::Bool(true));
                let path = self.root.path(FACE_CONF);
                let key = match k {
                    "useLogin" => "LOGIN",
                    "useUnlock" => "UNLOCK",
                    _ => "WEBCAM",
                };
                let old = std::fs::read_to_string(&path).unwrap_or_default();
                let mut lines: Vec<String> =
                    old.lines().filter(|l| !l.starts_with(&format!("{key}="))).map(String::from).collect();
                if lines.is_empty() {
                    lines.push(
                        "# Face sign-in, written by Asima Manager. WEBCAM=yes: a normal webcam may unlock the screen."
                            .into(),
                    );
                }
                lines.push(format!("{key}={}", if on { "yes" } else { "no" }));
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| failed(e.into()))?;
                std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| failed(e.into()))?;
            }
            _ => return Err(fdo::Error::NotSupported(format!("{area}/{key} is not a setting of this service"))),
        }
        let _ = Self::changed(&emitter, area).await;
        Ok(())
    }

    /// Firewall: key = "zone" (public | home | developer) or "killSwitch" (bool).
    async fn set_shield(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        key: &str,
        value: zvariant::Value<'_>,
    ) -> fdo::Result<()> {
        polkit::authorize(conn, &header, "io.github.asimaos.manager.shield", self.polkit).await?;
        let _g = self.lock.lock().await;
        let mut s = shield::load(&self.root);
        match (key, &value) {
            ("zone", zvariant::Value::Str(z)) if shield::ZONES.contains(&z.as_str()) => s.zone = z.to_string(),
            ("killSwitch", zvariant::Value::Bool(b)) => s.kill_switch = *b,
            _ => return Err(fdo::Error::InvalidArgs("Unknown firewall setting".into())),
        }
        // Apply first: the saved choice only changes when the firewall accepted it.
        if self.root.is_system() {
            shield::apply(&s).await.map_err(failed)?;
        }
        shield::save(&self.root, &s).map_err(failed)?;
        let _ = Self::changed(&emitter, "shield").await;
        Ok(())
    }

    #[zbus(signal)]
    async fn changed(emitter: &SignalEmitter<'_>, area: &str) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut session, mut apply_shield, mut apply_battery, mut root) = (false, false, false, Root::system());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--session" => session = true,
            "--apply-shield" => apply_shield = true,
            "--apply-battery" => apply_battery = true,
            "--root" => root = Root(PathBuf::from(args.next().context("--root needs a directory")?)),
            "--version" => {
                println!("asimaos-managerd {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    if apply_shield {
        let s = shield::load(&root);
        shield::apply(&s).await?;
        println!("Asima Shield: {} zone{}", s.zone, if s.kill_switch { ", VPN kill-switch on" } else { "" });
        return Ok(());
    }
    // At start-up (asimaos-battery.service): the saved charge limit back on the battery.
    if apply_battery {
        return device::apply_saved(&root);
    }
    anyhow::ensure!(!session || !root.is_system(), "--session needs --root (a scratch system tree)");
    let service = Manager { root, polkit: !session, lock: Mutex::new(()) };
    let builder = if session { connection::Builder::session()? } else { connection::Builder::system()? };
    let _conn = builder.name(NAME)?.serve_at(PATH, service)?.build().await?;
    std::future::pending::<()>().await;
    Ok(())
}
