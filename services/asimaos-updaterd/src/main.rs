//! asimaos-updaterd: the Asima Updater service (io.github.asimaos.Updater1 on the system bus).
//!
//! Update files and the versions in the start menu, nothing else, so it can stay unchanged while
//! the other Asima OS apps grow. Reading status never needs a password; every change is authorised
//! with polkit for the caller. Installing runs as a job that reports JobProgress / JobFinished.
//!
//!   asimaos-updaterd                      system bus, polkit (started by D-Bus activation)
//!   asimaos-updaterd --session --root DIR test mode: session bus, no polkit, a scratch system tree
//!   asimaos-updaterd --finish-update      after a start marked good (asimaos-finish-update.service)
//!   asimaos-updaterd --run-migrations     a new version's first start (asimaos-migrate.service)

mod status;
mod update;

use anyhow::Context;
use asimaos_svc::{polkit, snapshots, system, Root};
use asimaos_update::{boot, Mode};
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use zbus::{connection, fdo, interface, message::Header, object_server::SignalEmitter, zvariant, Connection};

const NAME: &str = "io.github.asimaos.Updater1";
const PATH: &str = "/io/github/asimaos/Updater1";
/// The pool's top level, for the snapshot before an update (private mount namespace).
const POOL: &str = "/run/asimaos-updaterd/pool";

struct Updater {
    root: Root,
    polkit: bool,
    test_mode: bool,
    busy: Arc<Mutex<()>>,
    jobs: AtomicU64,
    /// The newest job, for Status(): { id, progress, detail, done, ok, message } (the `asimaos`
    /// command follows it there; the apps use the signals).
    job: Arc<std::sync::Mutex<serde_json::Value>>,
}

fn note(job: &std::sync::Mutex<serde_json::Value>, update: serde_json::Value) {
    if let (Ok(mut j), Some(new)) = (job.lock(), update.as_object()) {
        if let Some(obj) = j.as_object_mut() {
            for (k, v) in new {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
}

/// Errors carry a sentence meant for people (the outermost context), shown as is by the GUI.
/// TPM unlock with systemd-pcrlock follows the start menu: a new version's boot image (and a new boot
/// loader on trial) is allowed before the restart, removed versions are taken out
/// (`/usr/lib/asimaos/asimaos-pcrlock`). Nothing happens when TPM unlock uses no pcrlock policy here.
/// A failure only means the passphrase is asked at the next start, so it never stops an update.
async fn refresh_tpm_policy(root: &Root) {
    if !root.is_system() {
        return;
    }
    if let Err(e) = asimaos_svc::cmd::run("/usr/lib/asimaos/asimaos-pcrlock", &["refresh"]).await {
        eprintln!("Asima OS: the TPM unlock policy was not updated ({e}); the passphrase may be asked at the next start");
    }
}

fn failed(e: anyhow::Error) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

fn file_from(fd: zvariant::OwnedFd) -> File {
    File::from(std::os::fd::OwnedFd::from(fd))
}

impl Updater {
    async fn auth(&self, conn: &Connection, header: &Header<'_>, action: &str) -> fdo::Result<()> {
        polkit::authorize(conn, header, action, self.polkit).await
    }

    /// Opens the slots read-write in this service's private mount namespace only (the unit has
    /// PrivateMounts=yes); everyone else keeps seeing them read-only.
    async fn slots_writable(&self, writable: bool) -> anyhow::Result<()> {
        if !self.root.is_system() {
            return Ok(());
        }
        let opts = if writable { "remount,rw,bind" } else { "remount,ro,bind" };
        asimaos_svc::cmd::run("mount", &["-o", opts, update::SLOTS]).await.map(|_| ())
    }

    /// Opens and checks an update file completely: signature, manifest, facts of this computer,
    /// and every part's checksum. Nothing is written. `reinstall`: the file must hold the running
    /// version itself ("Reinstall this version", D21).
    async fn verify(&self, file: File, reinstall: bool) -> anyhow::Result<(asimaos_update::Update, serde_json::Value)> {
        let root = self.root.clone();
        let test_mode = self.test_mode;
        tokio::task::spawn_blocking(move || -> anyhow::Result<(asimaos_update::Update, serde_json::Value)> {
            let mut u = update::open(&root, file)?;
            let facts = update::Facts {
                running: system::running_version(&root),
                free: asimaos_svc::free_bytes(&root.path(update::SLOTS)).unwrap_or(0),
            };
            let (mut problems, need) = update::assess(&u.manifest, &facts, test_mode, reinstall);
            // Every part is hashed before anything is written; skipped when it can't be
            // installed anyway (an expired file needn't be read twice).
            let parts_checked = problems.is_empty();
            let mut refused = None;
            if parts_checked {
                if let Err(e) = u.check_parts(&mut |_, _| {}) {
                    refused = Some(asimaos_update::refused(&e).map_or("invalid", |r| r.code()));
                    problems.push(update::Problem { code: "damaged", message: e.to_string() });
                }
            }
            let notes = u.notes().unwrap_or_default();
            let m = &u.manifest;
            let report = serde_json::json!({
                "ok": problems.is_empty(),
                "signature": true,
                "partsChecked": parts_checked,
                "refused": refused,
                "failed": problems.iter().map(|p| p.code).collect::<Vec<_>>(),
                "problems": problems.iter().map(|p| p.message.as_str()).collect::<Vec<_>>(),
                "version": m.version,
                "current": facts.running,
                "expires": m.expires,
                "unpackedSize": m.unpacked,
                "freeBytes": facts.free,
                "needBytes": need,
                "notes": notes,
                "reinstall": reinstall,
                // The running version itself: Asima Updater offers "Reinstall this version".
                "sameVersion": m.version == facts.running,
            });
            Ok((u, report))
        })
        .await?
    }

    /// The job behind ApplyUpdate and ReinstallVersion.
    async fn install_job(
        &self,
        header: &Header<'_>,
        conn: &Connection,
        file: zvariant::OwnedFd,
        reinstall: bool,
    ) -> fdo::Result<String> {
        self.auth(conn, header, "io.github.asimaos.updater.update").await?;
        let guard = self
            .busy
            .clone()
            .try_lock_owned()
            .map_err(|_| fdo::Error::Failed("Another change is in progress".into()))?;
        let (mut u, report) = self.verify(file_from(file), reinstall).await.map_err(failed)?;
        if report["ok"] != true {
            return Err(fdo::Error::Failed(
                report["problems"][0].as_str().unwrap_or("The update can't be installed").into(),
            ));
        }
        let version = u.manifest.version.clone();
        let job =
            format!("{}-{}", if reinstall { "reinstall" } else { "update" }, self.jobs.fetch_add(1, Ordering::Relaxed));
        let (root, conn, job_id) = (self.root.clone(), conn.clone(), job.clone());
        let state = self.job.clone();
        if let Ok(mut j) = state.lock() {
            *j = serde_json::json!({ "id": job, "progress": 0.0, "detail": "", "done": false, "ok": false, "message": "" });
        }
        let writable = self.slots_writable(true).await;
        tokio::spawn(async move {
            let _guard = guard;
            let emitter = SignalEmitter::new(&conn, PATH).unwrap().into_owned();
            let result = async {
                writable?;
                // Snapshots of the files and settings first (before every update).
                let _ =
                    Updater::job_progress(&emitter, &job_id, 0.01, "snapshot", "Snapshot of your files and settings")
                        .await;
                match snapshots::Pool::open(&root, POOL).await {
                    Ok(pool) => {
                        if let Err(e) = pool.before_update(&version).await {
                            eprintln!("asimaos-updaterd: snapshot before the update failed: {e:#}");
                        }
                    }
                    Err(e) => eprintln!("asimaos-updaterd: no snapshot before the update: {e:#}"),
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();
                let r2 = root.clone();
                let mode = if reinstall { Mode::Rewrite } else { Mode::Resume };
                let work = tokio::task::spawn_blocking(move || {
                    let mut last = 0.0;
                    asimaos_update::install(&mut u, &update::targets(&r2), mode, &mut |p, d| {
                        let p = 0.02 + 0.9 * p;
                        if p - last >= 0.005 {
                            last = p;
                            let _ = tx.send((p, d));
                        }
                    })
                });
                while let Some((p, d)) = rx.recv().await {
                    note(&state, serde_json::json!({ "progress": p, "detail": d }));
                    let _ = Updater::job_progress(&emitter, &job_id, p, "write", &d).await;
                }
                let installed = work.await??;
                update::keep_boot_copy(&root, &version)?;
                let running = system::running_version(&root);
                let _ = Updater::job_progress(&emitter, &job_id, 0.95, "finish", "Keeping two versions").await;
                boot::promote(&update::esp(&root), &version)?;
                update::prune(&root, &running)?;
                refresh_tpm_policy(&root).await;
                if root.is_system() {
                    let _ = asimaos_svc::cmd::run("mount", &["-o", "remount,ro,bind", update::SLOTS]).await;
                }
                anyhow::Ok(installed.kept.len())
            }
            .await;
            let (ok, message) = match result {
                Ok(_) if reinstall => {
                    (true, format!("A fresh copy of Asima OS {version} is written. Restart to use it."))
                }
                Ok(kept) if kept > 0 => (
                    true,
                    format!(
                        "Asima OS {version} is installed (the interrupted update was continued). Restart to start it."
                    ),
                ),
                Ok(_) => (true, format!("Asima OS {version} is installed. Restart to start it.")),
                Err(e) => (false, e.to_string()),
            };
            note(&state, serde_json::json!({ "progress": 1.0, "done": true, "ok": ok, "message": message }));
            let _ = Updater::job_finished(&emitter, &job_id, ok, &message).await;
        });
        Ok(job)
    }
}

#[interface(name = "io.github.asimaos.Updater1")]
impl Updater {
    /// The running version and every version in the start menu, as JSON. No password needed.
    async fn status(&self) -> String {
        let mut s = status::status(&self.root);
        if let (Some(obj), Ok(job)) = (s.as_object_mut(), self.job.lock()) {
            obj.insert("job".into(), job.clone());
        }
        s.to_string()
    }

    /// Checks an update file passed as a file descriptor: signature, newer, not expired, space.
    /// Returns a JSON report; never changes anything. No password needed.
    async fn verify_update(&self, file: zvariant::OwnedFd) -> fdo::Result<String> {
        match self.verify(file_from(file), false).await {
            Ok((_, report)) => Ok(report.to_string()),
            // The file couldn't be opened as a signed update: say why, so the user knows whether
            // to download it again or to get it from Asima OS.
            Err(e) => Ok(serde_json::json!({
                "ok": false,
                "signature": false,
                "refused": asimaos_update::refused(&e).map_or("invalid", |r| r.code()),
                "problems": [e.to_string()],
            })
            .to_string()),
        }
    }

    /// Installs a verified update as a job: the new version becomes "current" in the start menu,
    /// the running one its backup. Returns the job id; progress follows as signals.
    async fn apply_update(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        file: zvariant::OwnedFd,
    ) -> fdo::Result<String> {
        self.install_job(&header, conn, file, false).await
    }

    /// Writes a fresh copy of the running version from its own update file (D21): repairs
    /// damaged system files. Older versions stay refused.
    async fn reinstall_version(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        file: zvariant::OwnedFd,
    ) -> fdo::Result<String> {
        self.install_job(&header, conn, file, true).await
    }

    /// `once`: start `version` at the next start only. Otherwise make it "current" in the start
    /// menu (going back to the backup swaps the two; a failed update gets a fresh trial).
    async fn set_default_version(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        version: &str,
        once: bool,
    ) -> fdo::Result<()> {
        self.auth(conn, &header, "io.github.asimaos.updater.rollback").await?;
        let esp = update::esp(&self.root);
        let Some(entry) = boot::entries(&esp).into_iter().find(|e| e.version == version) else {
            return Err(fdo::Error::InvalidArgs("That version isn't in the start menu".into()));
        };
        if once {
            return asimaos_svc::cmd::run("bootctl", &["set-oneshot", &entry.id()]).await.map(|_| ()).map_err(failed);
        }
        let _guard = self
            .busy
            .clone()
            .try_lock_owned()
            .map_err(|_| fdo::Error::Failed("Another change is in progress".into()))?;
        boot::make_current(&esp, version).map_err(failed)?;
        // A failed update that gets another try pushes out the older backup.
        let writable = self.slots_writable(true).await;
        if writable.is_ok() {
            let _ = update::prune(&self.root, &system::running_version(&self.root));
            let _ = self.slots_writable(false).await;
        }
        refresh_tpm_policy(&self.root).await;
        Ok(())
    }

    #[zbus(signal)]
    async fn job_progress(
        emitter: &SignalEmitter<'_>,
        job: &str,
        progress: f64,
        step: &str,
        detail: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn job_finished(emitter: &SignalEmitter<'_>, job: &str, ok: bool, message: &str) -> zbus::Result<()>;
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
                println!("asimaos-updaterd {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            // After a start that systemd-bless-boot marked good (asimaos-finish-update.service).
            "--finish-update" => {
                if root.is_system() {
                    asimaos_svc::cmd::run("mount", &["-o", "remount,rw,bind", update::SLOTS]).await?;
                }
                system::open_esp(&root).await?;
                let done = update::finish(&root, &system::running_version(&root))?;
                for d in &done {
                    println!("Asima OS: updated {d}");
                }
                refresh_tpm_policy(&root).await;
                match update::note_new_defaults(&root) {
                    Ok(0) => {}
                    Ok(n) => {
                        println!(
                            "Asima OS: {n} settings file(s) you changed have a new default (Asima Maintenance)"
                        )
                    }
                    Err(e) => eprintln!("Asima OS: settings files not compared: {e}"),
                }
                return Ok(());
            }
            // Early in a new version's first start (asimaos-migrate.service).
            "--run-migrations" => {
                let n = update::run_migrations(&root, &system::running_version(&root))?;
                if n > 0 {
                    println!("Asima OS: {n} migration step(s) done");
                }
                return Ok(());
            }
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    // Polkit can only be skipped on a private test bus with a scratch root.
    anyhow::ensure!(!session || !root.is_system(), "--session needs --root (a scratch system tree)");
    // The start menu and the boot images live on the ESP, opened only in this service's namespace.
    system::open_esp(&root).await?;
    let updater = Updater {
        root,
        polkit: !session,
        test_mode: session,
        busy: Arc::new(Mutex::new(())),
        jobs: AtomicU64::new(1),
        job: Arc::new(std::sync::Mutex::new(serde_json::Value::Null)),
    };
    let busy = updater.busy.clone();
    let builder = if session { connection::Builder::session()? } else { connection::Builder::system()? };
    let conn = builder.name(NAME)?.serve_at(PATH, updater)?.build().await?;
    // The ESP is mounted only while the service runs: it exits after 5 idle minutes and
    // D-Bus starts it again on the next call. Off on a test bus unless ASIMAOS_IDLE_EXIT is set.
    let idle = if session { Duration::ZERO } else { Duration::from_secs(300) };
    asimaos_svc::idle::exit_when_idle(&conn, asimaos_svc::idle::after(idle), move || busy.try_lock().is_err());
    std::future::pending::<()>().await;
    Ok(())
}
