//! asimaos-maintenanced: the Asima Maintenance service (io.github.asimaos.Maintenance1 on the system bus).
//!
//! Looking after the installed system: the health check, Secure Boot and the disk's secrets,
//! snapshots and backups, restoring /etc files and resetting the settings, and the factory reset.
//! Reading status never needs a password; every change is authorised with polkit for the caller.
//!
//!   asimaos-maintenanced                      system bus, polkit (started by D-Bus activation)
//!   asimaos-maintenanced --session --root DIR test mode: session bus, no polkit, a scratch system tree
//!   asimaos-maintenanced --snapshot-daily     every night (asimaos-snapshot.timer), when the schedule says so
//!   asimaos-maintenanced --backup-run         asimaos-backup.service (timer or "Back up now")
//!   asimaos-maintenanced --restore-run        asimaos-restore.service ("Restore" in Backups)
//!   asimaos-maintenanced --tpm-rotation       asimaos-tpm-rotation.service: TPM unlock set up again, second step

mod backup;
mod bootorder;
mod crashes;
mod etc;
mod health;
mod ownkeys;
mod report;
mod reset;
mod schedule;
mod security;
mod status;
mod storage;

use anyhow::Context;
use asimaos_svc::{polkit, snapshots, Root};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use zbus::{connection, fdo, interface, message::Header, Connection};

const NAME: &str = "io.github.asimaos.Maintenance1";
const PATH: &str = "/io/github/asimaos/Maintenance1";
const WORKDIR: &str = "/run/asimaos-maintenanced";
/// The pool's top level, for snapshots (private mount namespace).
const POOL: &str = "/run/asimaos-maintenanced/pool";

struct Maintenance {
    root: Root,
    polkit: bool,
    workdir: PathBuf,
    jobs: AtomicU64,
}

/// Errors carry a sentence meant for people (the outermost context), shown as is by the GUI.
fn failed(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

impl Maintenance {
    async fn auth(&self, conn: &Connection, header: &Header<'_>, action: &str) -> fdo::Result<()> {
        polkit::authorize(conn, header, action, self.polkit).await
    }

    fn scratch(&self) -> fdo::Result<PathBuf> {
        let dir = self.workdir.join(format!("w{}", self.jobs.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).map_err(|e| fdo::Error::Failed(e.to_string()))?;
        Ok(dir)
    }
}

#[interface(name = "io.github.asimaos.Maintenance1")]
impl Maintenance {
    /// Everything Asima Maintenance shows, as a JSON object. No password needed.
    async fn status(&self) -> String {
        status::status(&self.root, POOL).await.to_string()
    }

    async fn run_health_check(&self) -> String {
        health::run(&self.root).await.to_string()
    }

    // ── Disk use and limits (D55) ──────────────────────────────────────────────

    /// How much each area takes and its limit, as JSON: containers, virtual machines and Nix
    /// programs (the system's and each person's), Flatpak apps, /home and /var. No password needed.
    async fn storage(&self) -> String {
        storage::status(&self.root).await.to_string()
    }

    /// A limit for one area ("nix", "vm", "containers:asha" …), in bytes; 0 removes it.
    async fn set_storage_limit(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        area: &str,
        bytes: u64,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.storage").await?;
        storage::set_limit(&self.root, area, bytes).await.map_err(failed)
    }

    /// Flatpak repair (D55) of the system's installation: every installed app and runtime is
    /// checked, broken objects are removed and what is missing is downloaded again. Returns
    /// Flatpak's report. (Each person's own installation is repaired without root.)
    async fn repair_flatpak(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.flatpak").await?;
        let out = asimaos_svc::cmd::run("flatpak", &["repair", "--system"]).await.map_err(failed)?;
        Ok(out.stdout)
    }

    // ── Snapshots and backups ──────────────────────────────────────────────────

    /// A snapshot of /home now (also taken every night). Returns its name.
    async fn snapshot_now(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        let pool = snapshots::Pool::open(&self.root, POOL).await.map_err(failed)?;
        pool.snapshot_home().await.map_err(failed)
    }

    /// Sets up the backup (JSON: repository, password) and creates the repository if needed.
    async fn configure_backup(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        settings: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        let v: serde_json::Value =
            serde_json::from_str(settings).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        let (repo, pw) = (v["repository"].as_str().unwrap_or(""), v["password"].as_str().unwrap_or(""));
        backup::configure(&self.root, repo, pw).await.map_err(failed)
    }

    /// Starts a backup now; Status() shows its progress and result.
    async fn backup_now(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        if backup::repository(&self.root).is_none() {
            return Err(fdo::Error::Failed("No backup is set up yet".into()));
        }
        if self.root.is_system() {
            // asimaos-backup.service may read /home; this service may not.
            asimaos_svc::cmd::run("systemctl", &["start", "--no-block", "asimaos-backup.service"])
                .await
                .map(|_| ())
                .map_err(failed)
        } else {
            let root = self.root.clone();
            tokio::spawn(async move {
                let _ = backup::run(&root).await;
            });
            Ok(())
        }
    }

    /// How often snapshots are taken by themselves: "daily", "weekly", "monthly" or "off".
    async fn set_snapshot_schedule(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        schedule: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        schedule::set(&self.root, schedule).map_err(failed)
    }

    /// The backups that can be restored, as a JSON array (newest first).
    async fn list_backups(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        backup::list(&self.root).await.map(|v| v.to_string()).map_err(failed)
    }

    /// Brings one person's folder back from a backup, as a new folder in their home; Status()
    /// shows the progress and the folder's name.
    async fn restore_backup(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        id: &str,
        user: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.backup").await?;
        backup::request_restore(&self.root, id, user).map_err(failed)?;
        if self.root.is_system() {
            // asimaos-restore.service may write to /home; this service may not.
            asimaos_svc::cmd::run("systemctl", &["start", "--no-block", "asimaos-restore.service"])
                .await
                .map(|_| ())
                .map_err(failed)
        } else {
            let root = self.root.clone();
            tokio::spawn(async move {
                let _ = backup::restore_run(&root).await;
            });
            Ok(())
        }
    }

    // ── Problem report ───────────────────────────────────────────────

    /// The problem report as plain text, for the person to read and edit before saving it.
    async fn problem_report(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.report").await?;
        Ok(report::make(&self.root).await)
    }

    // ── Secure Boot (D16) ──────────────────────────────────────────────────────

    /// Queues the Asima OS key for approval; returns the one-time code for the blue MOK screen.
    async fn approve_asimaos_key(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        let dir = self.scratch()?;
        let r = security::approve_key(&self.root, &dir).await.map_err(failed);
        let _ = std::fs::remove_dir_all(&dir);
        r
    }

    async fn cancel_key_approval(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        security::cancel_key().await.map_err(failed)
    }

    // ── Disk secrets (D10) ─────────────────────────────────────────────────────

    async fn enroll_tpm(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        secret: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secrets").await?;
        security::require_encrypted(&self.root).map_err(failed)?;
        security::enroll_tpm(&self.root, secret, None).await.map_err(failed)
    }

    /// The optional way (D49): the TPM opens the disk only together with a PIN typed at every start.
    async fn enroll_tpm_pin(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        secret: &str,
        pin: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secrets").await?;
        security::check_pin(pin).map_err(failed)?;
        security::require_encrypted(&self.root).map_err(failed)?;
        security::enroll_tpm(&self.root, secret, Some(pin)).await.map_err(failed)
    }

    async fn remove_tpm(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secrets").await?;
        security::require_encrypted(&self.root).map_err(failed)?;
        security::remove_tpm(&self.root).await.map_err(failed)
    }

    async fn change_passphrase(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        current: &str,
        new: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secrets").await?;
        security::require_encrypted(&self.root).map_err(failed)?;
        security::change_passphrase(current, new).await.map_err(failed)
    }

    /// Enrolls a new recovery key, removes the old one and returns the new key once.
    async fn new_recovery_key(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        current: &str,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secrets").await?;
        security::require_encrypted(&self.root).map_err(failed)?;
        security::new_recovery_key(current).await.map_err(failed)
    }

    // ── System changes ─────────────────────────────────────────────────────────

    /// Schedules restoring the factory version of one /etc file at the next start.
    async fn restore_etc(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        path: &str,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.etc").await?;
        etc::schedule(&self.root, path).map_err(failed)
    }

    /// Own Secure Boot keys, step 1: saves the firmware's db, KEK and dbx and makes this
    /// computer's Platform Key and KEK. Then the person puts the firmware into Setup Mode.
    async fn prepare_own_keys(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        ownkeys::prepare(&self.root).await.map_err(failed)
    }

    /// Own Secure Boot keys, in Setup Mode: writes dbx, db (the kept certificates plus the
    /// Asima OS key), KEK and the Platform Key. `drop`: fingerprints to leave out.
    async fn enroll_own_keys(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        drop: Vec<String>,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        ownkeys::enroll(&self.root, &drop).await.map_err(failed)
    }

    /// Retires approved Secure Boot keys (SHA-256 fingerprints from Status): removed from the MOK
    /// list and blocked in MOKX at the next start, confirmed with the returned one-time code.
    async fn retire_keys(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        ids: Vec<String>,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        let dir = self.scratch()?;
        let r = security::retire_keys(&self.root, &dir, &ids).await.map_err(failed);
        let _ = std::fs::remove_dir_all(&dir);
        r
    }

    async fn cancel_retire_keys(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.secureboot").await?;
        security::cancel_retire().await.map_err(failed)
    }

    /// Moves the entry this start came from to the front of the firmware's start order (after
    /// Windows put itself first).
    async fn start_asimaos_first(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.boot").await?;
        bootorder::start_asimaos_first(&self.root).map_err(failed)
    }

    /// "Reset settings": the system's settings and the caller's desktop and app settings go back
    /// to this version's defaults at the next start (the accounts, files, networks, language,
    /// keyboard and time zone stay), then the computer restarts. Returns how many /etc entries
    /// are reset.
    async fn reset_settings(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<u32> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.etc").await?;
        let sender = header.sender().ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?.to_owned();
        let uid = fdo::DBusProxy::new(conn).await?.get_connection_unix_user(sender.into()).await?;
        let person = etc::person(&self.root, uid).map_err(failed)?;
        let n = etc::schedule_reset(&self.root, &person).await.map_err(failed)?;
        if self.root.is_system() {
            asimaos_svc::cmd::run("systemctl", &["reboot"]).await.map_err(failed)?;
        }
        Ok(n as u32)
    }

    // ── Reset (D7, D12, D13) ───────────────────────────────────────────────────

    /// Stages the fresh account (JSON: fullName, username, password, passphrase, current).
    /// Returns JSON { recoveryKey, username }; the key is empty without encryption.
    async fn prepare_factory_reset(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        fresh: &str,
    ) -> fdo::Result<String> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.reset").await?;
        let fresh: reset::Fresh = serde_json::from_str(fresh).map_err(|e| fdo::Error::InvalidArgs(e.to_string()))?;
        reset::prepare(&self.root, fresh).await.map(|v| v.to_string()).map_err(failed)
    }

    async fn cancel_factory_reset(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.reset").await?;
        reset::cancel(&self.root).await.map_err(failed)
    }

    /// Marks the staged reset and restarts; the initramfs carries it out.
    async fn schedule_factory_reset(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.maintenance.reset").await?;
        reset::schedule(&self.root).map_err(failed)?;
        if self.root.is_system() {
            asimaos_svc::cmd::run("systemctl", &["reboot"]).await.map_err(failed)?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut session, mut root) = (false, Root::system());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--session" => session = true,
            "--root" => root = Root(PathBuf::from(args.next().context("--root needs a directory")?)),
            "--version" => {
                println!("asimaos-maintenanced {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            // Every night (asimaos-snapshot.timer): a snapshot when the schedule says one is due.
            "--snapshot-daily" => {
                let pool = snapshots::Pool::open(&root, POOL).await?;
                let when = schedule::get(&root);
                if !schedule::due(&when, pool.days_since_newest()) {
                    println!("Asima OS: no snapshot due (schedule: {when})");
                    return Ok(());
                }
                let name = pool.snapshot_home().await?;
                println!("Asima OS: snapshot {name} of /home and the settings");
                return Ok(());
            }
            // asimaos-restore.service ("Restore" in Asima Maintenance → Backups): writes to /home.
            "--restore-run" => {
                backup::restore_run(&root).await?;
                return Ok(());
            }
            // asimaos-tpm-rotation.service, after a start: the second step of setting TPM unlock up again.
            "--tpm-rotation" => {
                security::finish_rotation(&root).await?;
                return Ok(());
            }
            // asimaos-backup.service (timer or "Back up now"): needs to read /home.
            "--backup-run" => {
                backup::run(&root).await?;
                return Ok(());
            }
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    // Polkit can only be skipped on a private test bus with a scratch root.
    anyhow::ensure!(!session || !root.is_system(), "--session needs --root (a scratch system tree)");
    // The health check reads the start menu, Secure Boot and Rescue need the ESP: opened only in
    // this service's namespace. Not fatal: everything else works without it.
    if let Err(e) = asimaos_svc::system::open_esp(&root).await {
        eprintln!("asimaos-maintenanced: {e}");
    }
    let workdir = if session { root.path(WORKDIR) } else { PathBuf::from(WORKDIR) };
    std::fs::create_dir_all(&workdir)?;
    let service = Maintenance { root, polkit: !session, workdir, jobs: AtomicU64::new(1) };
    let builder = if session { connection::Builder::session()? } else { connection::Builder::system()? };
    let conn = builder.name(NAME)?.serve_at(PATH, service)?.build().await?;
    // The ESP is mounted only while the service runs: it exits after 5 idle minutes and
    // D-Bus starts it again on the next call. Backups and restores run in their own units.
    let idle = if session { Duration::ZERO } else { Duration::from_secs(300) };
    asimaos_svc::idle::exit_when_idle(&conn, asimaos_svc::idle::after(idle), || false);
    std::future::pending::<()>().await;
    Ok(())
}
