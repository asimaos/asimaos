//! asimaos-faced: face sign-in, and fingerprint or face approval of administrator tasks, for Asima OS
//! OS (io.github.asimaos.Face1 on the system bus).
//!
//! The service owns the camera and the faceprints; apps only start enrolment and show its
//! progress, and the PAM module (pam_asimaos_face.so) asks VerifyFor. No camera frame ever leaves
//! the service. Infrared cameras are used as they are; a normal webcam only after the person
//! switched it on in Asima Manager, with a stricter match and a head turn, and only to unlock the
//! screen (D37).
//!
//! Administrator tasks (D37): the Asima OS password dialog (asimaos-polkit-agent) offers a fingerprint
//! and a face button. Tapping one asks AdminToken: this service checks the finger (through fprintd)
//! or the face itself, as root, and returns a one-time token that the dialog sends in place of the
//! password. pam_asimaos_face.so (use=admin, in the polkit-1 stack) hands it back to ConsumeToken.
//!
//!   asimaos-faced                                       system bus, polkit
//!   asimaos-faced --session --store DIR --models DIR --picture FILE [--webcam] [--conf FILE]
//!                                                      test mode: a still picture as the camera

mod camera;
mod face;
mod store;

use anyhow::Context;
use asimaos_svc::polkit;
use camera::{Kind, Source};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use zbus::{connection, fdo, interface, message::Header, object_server::SignalEmitter, Connection};

const NAME: &str = "io.github.asimaos.Face1";
const PATH: &str = "/io/github/asimaos/Face1";
const MODELS: &str = "/usr/share/asimaos/face";
const STORE: &str = "/var/lib/asimaos-face";
/// Where face sign-in may be used (Asima Manager writes it): LOGIN=no, UNLOCK=no, WEBCAM=yes.
const FACE_CONF: &str = "/etc/asimaos/face.conf";
/// The only program that may ask for administrator tokens: the Asima OS password dialog.
const AGENT: &str = "/usr/lib/asimaos/asimaos-polkit-agent";
const ENROLL_FRAMES: usize = 12; // faceprints kept per look from a still test picture
const ENROLL_MAX: usize = 24; // faceprints kept per look from a guided set-up (several poses)
const MATCH: f32 = 0.45; // cosine similarity; stricter than SFace's 0.363 for sign-in
const MATCH_WEBCAM: f32 = 0.55; // a normal webcam: stricter still (D37)
const MAX_LOOKS: usize = 2;
const MAX_FAILS: u32 = 3; // then a pause, and the password
const PAUSE: Duration = Duration::from_secs(300);
const TOKEN_LIFE: Duration = Duration::from_secs(60);
/// Plasma's lock screen offers a Face button when this PAM service exists (D47).
const LOCK_SCREEN_FACE: &str = "/etc/pam.d/kde-face";
const LOCK_SCREEN_FACE_TEMPLATE: &str = "/usr/share/asimaos/pam/kde-face";

/// The lock screen's Face button is there while someone has a face set up, and only then.
fn lock_screen_button(store: &store::Store, file: &Path, template: &Path) {
    if store.anyone() {
        if !file.exists() {
            if let Err(e) = std::fs::copy(template, file) {
                eprintln!("asimaos-faced: the lock screen's Face button could not be added: {e}");
            }
        }
    } else {
        let _ = std::fs::remove_file(file);
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.asimaos.Face1")]
enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// Face sign-in can't be used here (no face set up, a webcam at the login screen, a pause
    /// after failed tries): PAM moves on to the password.
    NoFace(String),
    NoCamera(String),
    AccessDenied(String),
    Busy(String),
    Failed(String),
}

impl From<fdo::Error> for Error {
    fn from(e: fdo::Error) -> Self {
        Error::AccessDenied(e.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Failed(e.to_string())
    }
}

struct FaceService {
    models: Arc<face::Models>,
    store: Arc<store::Store>,
    /// The lock screen's Face button: its PAM file, and the template it is copied from.
    pam: Arc<(PathBuf, PathBuf)>,
    picture: Option<face::Frame>,
    /// Tests: the picture stands in for a webcam instead of an infrared camera.
    picture_webcam: bool,
    conf: PathBuf,
    polkit: bool,
    busy: Arc<Mutex<()>>,
    reader: Arc<Mutex<()>>,
    cancel: Arc<AtomicBool>,
    /// Failed tries per account: (count, since).
    fails: std::sync::Mutex<HashMap<u32, (u32, Instant)>>,
    /// One-time administrator tokens: token → (account, the dialog's process, issued).
    tokens: std::sync::Mutex<HashMap<String, (u32, u32, Instant)>>,
}

impl FaceService {
    fn conf_says(&self, line: &str) -> bool {
        std::fs::read_to_string(&self.conf).unwrap_or_default().lines().any(|l| l.trim() == line)
    }

    /// The camera face sign-in uses now (looked up each time: cameras come and go).
    fn current_camera(&self) -> Option<camera::Camera> {
        if self.picture.is_some() {
            let kind = if self.picture_webcam { Kind::Webcam } else { Kind::Infrared };
            return Some(camera::Camera { dev: String::new(), name: "Test picture".into(), kind });
        }
        camera::find(self.conf_says("WEBCAM=yes"))
    }

    fn source(&self, cam: &camera::Camera) -> Result<Box<dyn Source>, Error> {
        if let Some(p) = &self.picture {
            let frame = face::Frame { width: p.width, height: p.height, channels: 1, data: p.data.clone() };
            return Ok(Box::new(camera::Still { frame, n: 0 }));
        }
        Ok(Box::new(camera::V4l2::open(cam)?))
    }

    async fn caller_uid(conn: &Connection, header: &Header<'_>) -> Result<u32, Error> {
        let sender = header.sender().ok_or_else(|| Error::AccessDenied("no sender".into()))?.to_owned();
        let dbus = fdo::DBusProxy::new(conn).await?;
        Ok(dbus.get_connection_unix_user(sender.into()).await?)
    }

    /// Administrator tokens go only to the Asima OS password dialog, never to any other program of
    /// the session (which could otherwise watch the camera unasked), and only to one nobody has
    /// tampered with: started without code injected through the environment (LD_PRELOAD, Qt plugin
    /// paths), not traced, and protected from being attached to (the dialog makes itself
    /// non-dumpable at its start, which hands its /proc files to root). Returns the dialog's
    /// process, which the token is tied to (0 in test mode).
    async fn require_agent(&self, conn: &Connection, header: &Header<'_>) -> Result<u32, Error> {
        if !self.polkit {
            return Ok(0);
        }
        let sender = header.sender().ok_or_else(|| Error::AccessDenied("no sender".into()))?.to_owned();
        let pid = fdo::DBusProxy::new(conn).await?.get_connection_unix_process_id(sender.into()).await?;
        let refuse =
            |why: &str| Error::AccessDenied(format!("Only the Asima OS password dialog may ask for this ({why})"));
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).unwrap_or_default();
        if exe != Path::new(AGENT) {
            return Err(refuse("another program"));
        }
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
        if tracer(&status) != Some(0) {
            return Err(refuse("it is being traced"));
        }
        use std::os::unix::fs::MetadataExt;
        if std::fs::metadata(format!("/proc/{pid}/environ")).map(|m| m.uid()).unwrap_or(u32::MAX) != 0 {
            return Err(refuse("it can be attached to"));
        }
        let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        if let Some(var) = injected(&environ) {
            return Err(refuse(&format!("started with {var}")));
        }
        Ok(pid)
    }

    fn paused(&self, uid: u32) -> bool {
        let mut fails = self.fails.lock().unwrap();
        match fails.get(&uid) {
            Some((n, since)) if *n >= MAX_FAILS && since.elapsed() < PAUSE => true,
            Some((n, _)) if *n >= MAX_FAILS => {
                fails.remove(&uid);
                false
            }
            _ => false,
        }
    }

    fn count(&self, uid: u32, ok: bool) {
        let mut fails = self.fails.lock().unwrap();
        if ok {
            fails.remove(&uid);
        } else {
            let e = fails.entry(uid).or_insert((0, Instant::now()));
            e.0 += 1;
            e.1 = Instant::now();
        }
    }

    /// Looks at the camera for `user`'s face; webcams need a straight look and a head turn.
    async fn look(&self, uid: u32) -> Result<bool, Error> {
        if self.paused(uid) {
            return Err(Error::NoFace("Too many tries: use the password".into()));
        }
        let known: Vec<Vec<f32>> = self.store.load(uid).faces.into_iter().flat_map(|f| f.embeddings).collect();
        if known.is_empty() {
            return Err(Error::NoFace("No face is set up for this account".into()));
        }
        let cam = self.current_camera().ok_or_else(|| Error::NoFace("No camera for face sign-in".into()))?;
        let _guard = self.busy.clone().try_lock_owned().map_err(|_| Error::Busy("The camera is in use".into()))?;
        let mut source = self.source(&cam)?;
        let models = self.models.clone();
        let kind = cam.kind;
        let ok = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
            let mut check = face::Liveness::new(kind == Kind::Webcam);
            // A webcam's two blinks take a few seconds more.
            let limit = if kind == Kind::Webcam { 8000 } else { 4000 };
            let started = Instant::now();
            while started.elapsed().as_millis() < limit {
                let frame = source.next()?;
                if let (Some(f), _) = best_face(&models, &frame)? {
                    let print = models.embed(&frame, &f)?;
                    let best = known.iter().map(|k| face::similarity(k, &print)).fold(f32::MIN, f32::max);
                    let strict = if kind == Kind::Webcam { MATCH_WEBCAM } else { MATCH };
                    if check.see(best, strict, face::eye_openness(&frame, &f)) {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })
        .await
        .map_err(|e| Error::Failed(e.to_string()))?
        .map_err(Error::from)?;
        self.count(uid, ok);
        Ok(ok)
    }

    fn new_token(&self, uid: u32, agent: u32) -> Result<String, Error> {
        let mut bytes = [0u8; 24];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
            .map_err(|e| Error::Failed(e.to_string()))?;
        let token = format!("asimaos-admin-token:{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let mut tokens = self.tokens.lock().unwrap();
        tokens.retain(|_, (_, _, at)| at.elapsed() < TOKEN_LIFE);
        tokens.insert(token.clone(), (uid, agent, Instant::now()));
        Ok(token)
    }
}

fn uid_of(user: &str) -> Option<u32> {
    let c = std::ffi::CString::new(user).ok()?;
    // SAFETY: getpwnam returns a pointer to static storage or NULL; we read one field at once.
    let pw = unsafe { libc::getpwnam(c.as_ptr()) };
    (!pw.is_null()).then(|| unsafe { (*pw).pw_uid })
}

/// The biggest face in the frame and what to tell the person.
fn best_face(models: &face::Models, frame: &face::Frame) -> anyhow::Result<(Option<face::Face>, &'static str)> {
    let faces = models.detect(frame)?;
    let Some(f) = faces.into_iter().max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h))) else {
        return Ok((None, "look"));
    };
    let share = f.w / frame.width as f32;
    if share < 0.15 {
        return Ok((None, "closer"));
    }
    if share > 0.75 {
        return Ok((None, "farther"));
    }
    Ok((Some(f), "turn"))
}

/// The fingerprint reader (fprintd), as root: does a finger of `user` match within three tries?
async fn fingerprint_matches(conn: &Connection, user: &str) -> anyhow::Result<bool> {
    const FPRINT: &str = "net.reactivated.Fprint";
    let manager =
        zbus::Proxy::new(conn, FPRINT, "/net/reactivated/Fprint/Manager", "net.reactivated.Fprint.Manager").await?;
    let dev: zbus::zvariant::OwnedObjectPath =
        manager.call("GetDefaultDevice", &()).await.context("There is no fingerprint reader")?;
    let device = zbus::Proxy::new(conn, FPRINT, dev, "net.reactivated.Fprint.Device").await?;
    let mut status = device.receive_signal("VerifyStatus").await?;
    device.call_method("Claim", &(user,)).await.context("The fingerprint reader is busy")?;
    let result = async {
        for _ in 0..3 {
            device.call_method("VerifyStart", &("any",)).await?;
            loop {
                let msg = tokio::time::timeout(Duration::from_secs(20), status.next())
                    .await
                    .context("No finger was placed")?
                    .context("The fingerprint reader went away")?;
                let (result, done): (String, bool) = msg.body().deserialize()?;
                if done {
                    let _ = device.call_method("VerifyStop", &()).await;
                    match result.as_str() {
                        "verify-match" => return Ok(true),
                        "verify-no-match" => break,
                        _ => return Ok(false),
                    }
                }
            }
        }
        Ok(false)
    }
    .await;
    let _ = device.call_method("Release", &()).await;
    result
}

/// Whether `user` has a finger enrolled on the default reader.
async fn has_fingerprint(conn: &Connection, user: &str) -> bool {
    async {
        let manager = zbus::Proxy::new(
            conn,
            "net.reactivated.Fprint",
            "/net/reactivated/Fprint/Manager",
            "net.reactivated.Fprint.Manager",
        )
        .await?;
        let dev: zbus::zvariant::OwnedObjectPath = manager.call("GetDefaultDevice", &()).await?;
        let device = zbus::Proxy::new(conn, "net.reactivated.Fprint", dev, "net.reactivated.Fprint.Device").await?;
        let fingers: Vec<String> = device.call("ListEnrolledFingers", &(user,)).await?;
        anyhow::Ok(!fingers.is_empty())
    }
    .await
    .unwrap_or(false)
}

#[interface(name = "io.github.asimaos.Face1")]
impl FaceService {
    /// The name of the camera face sign-in uses; empty when there is none.
    #[zbus(property)]
    async fn camera(&self) -> String {
        self.current_camera().map(|c| c.name).unwrap_or_default()
    }

    /// "infrared", "webcam" (switched on in Asima Manager) or "" (none).
    #[zbus(property)]
    async fn camera_kind(&self) -> String {
        self.current_camera().map(|c| c.kind.name().to_string()).unwrap_or_default()
    }

    /// A normal webcam's name, used or not: Asima Manager offers to switch it on.
    #[zbus(property)]
    async fn webcam(&self) -> String {
        if self.picture.is_some() {
            return if self.picture_webcam { "Test picture".into() } else { String::new() };
        }
        camera::webcam_name().unwrap_or_default()
    }

    /// The caller's face looks (labels).
    async fn list_faces(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<Vec<String>, Error> {
        let uid = Self::caller_uid(conn, &header).await?;
        Ok(self.store.load(uid).faces.into_iter().map(|f| f.label).collect())
    }

    /// Starts enrolling the caller's face; progress follows as EnrollProgress / EnrollDone.
    async fn enroll(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        label: &str,
    ) -> Result<(), Error> {
        polkit::authorize(conn, &header, "io.github.asimaos.face.enroll", self.polkit).await?;
        let uid = Self::caller_uid(conn, &header).await?;
        if self.store.load(uid).faces.len() >= MAX_LOOKS {
            return Err(Error::Failed("Two looks are already set up. Remove them to start again.".into()));
        }
        let cam = self.current_camera().ok_or_else(|| {
            Error::NoCamera(
                "This computer has no camera face sign-in may use. A webcam must be switched on first.".into(),
            )
        })?;
        let guard = self.busy.clone().try_lock_owned().map_err(|_| Error::Busy("The camera is in use".into()))?;
        let mut source = self.source(&cam)?;
        self.cancel.store(false, Ordering::SeqCst);
        // A camera guides the person through poses and blinks; a still test picture can't.
        let guided = self.picture.is_none();
        let (models, store, cancel, label) =
            (self.models.clone(), self.store.clone(), self.cancel.clone(), label.chars().take(40).collect::<String>());
        let pam = self.pam.clone();
        let emitter = SignalEmitter::new(conn, PATH)?.into_owned();
        tokio::spawn(async move {
            let _guard = guard;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, &'static str)>();
            let work = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Vec<f32>>> {
                let mut prints = Vec::new();
                let started = Instant::now();
                // Guided (a camera): straight, turn both ways, nod both ways, three blinks (D47). A
                // still test picture can't turn or blink: it gives straight looks only.
                let mut guide = face::EnrollGuide::default();
                let done = |g: &face::EnrollGuide, n: usize| if guided { g.done() } else { n >= ENROLL_FRAMES };
                while !done(&guide, prints.len()) {
                    anyhow::ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
                    anyhow::ensure!(
                        started.elapsed().as_secs() < if guided { 90 } else { 30 },
                        "No face was seen clearly. Face the camera, about an arm's length away, and try again."
                    );
                    let frame = source.next()?;
                    let (found, hint) = best_face(&models, &frame)?;
                    let mut hint = hint;
                    if let Some(f) = found {
                        let keep = !guided || guide.see(face::yaw(&f), face::pitch(&f), face::eye_openness(&frame, &f));
                        if keep && prints.len() < ENROLL_MAX {
                            prints.push(models.embed(&frame, &f)?);
                        }
                        if guided {
                            hint = guide.hint();
                        }
                    }
                    let progress = if guided { guide.progress() } else { prints.len() as f64 / ENROLL_FRAMES as f64 };
                    let hint = if !guided && prints.len() + 2 >= ENROLL_FRAMES { "hold" } else { hint };
                    let _ = tx.send((progress, hint));
                }
                anyhow::ensure!(!prints.is_empty(), "No face was seen clearly. Try again.");
                Ok(prints)
            });
            while let Some((p, hint)) = rx.recv().await {
                let _ = FaceService::enroll_progress(&emitter, p, hint).await;
            }
            let result = match work.await {
                Ok(Ok(prints)) => {
                    let mut faces = store.load(uid);
                    faces.version = 1;
                    faces.faces.push(store::StoredFace { label, embeddings: prints });
                    let saved = store.save(uid, &faces).map(|_| String::new());
                    lock_screen_button(&store, &pam.0, &pam.1);
                    saved
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!(e.to_string())),
            };
            let _ = match result {
                Ok(_) => FaceService::enroll_done(&emitter, true, "").await,
                Err(e) => FaceService::enroll_done(&emitter, false, &e.to_string()).await,
            };
        });
        Ok(())
    }

    async fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Deletes the caller's faceprints.
    async fn delete_all(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> Result<(), Error> {
        let uid = Self::caller_uid(conn, &header).await?;
        let r = self.store.delete(uid).map_err(Error::from);
        lock_screen_button(&self.store, &self.pam.0, &self.pam.1);
        r
    }

    /// The lock screen's question, kept for older callers: VerifyFor(user, "unlock").
    async fn verify_user(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        user: &str,
    ) -> Result<bool, Error> {
        self.verify_for(header, conn, user, "unlock").await
    }

    /// For the PAM module: does the person in front of the camera match `user`? `use` is "login"
    /// or "unlock"; a webcam only unlocks the screen (D37). Only root (the login manager) or that
    /// user (lock screen) may ask. Looks for up to four seconds (six with a webcam's head turn).
    async fn verify_for(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        user: &str,
        r#use: &str,
    ) -> Result<bool, Error> {
        let caller = Self::caller_uid(conn, &header).await?;
        let uid = uid_of(user).ok_or_else(|| Error::NoFace("Unknown user".into()))?;
        if caller != 0 && caller != uid {
            return Err(Error::AccessDenied("Only the person themself can sign in with their face".into()));
        }
        if r#use != "unlock" && self.current_camera().is_some_and(|c| c.kind == Kind::Webcam) {
            return Err(Error::NoFace("A webcam face only unlocks the screen".into()));
        }
        self.look(uid).await
    }

    /// What may approve administrator tasks for `user` besides the password: "fingerprint" (a
    /// reader with a finger enrolled), "face" (a face set up and a camera). For the dialog's buttons.
    async fn admin_methods(&self, #[zbus(connection)] conn: &Connection, user: &str) -> Vec<String> {
        let mut methods = Vec::new();
        if self.polkit && has_fingerprint(conn, user).await {
            methods.push("fingerprint".to_string());
        }
        let has_face = uid_of(user).is_some_and(|uid| !self.store.load(uid).faces.is_empty());
        if has_face && self.current_camera().is_some() {
            methods.push("face".to_string());
        }
        methods
    }

    /// The password dialog's fingerprint or face button: checks `user`'s finger or face and
    /// returns a one-time token (a minute) for pam_asimaos_face.so (use=admin), or an error.
    async fn admin_token(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        kind: &str,
        user: &str,
    ) -> Result<String, Error> {
        let agent = self.require_agent(conn, &header).await?;
        polkit::authorize(conn, &header, "io.github.asimaos.face.admin", self.polkit).await?;
        let uid = uid_of(user).ok_or_else(|| Error::Failed("Unknown user".into()))?;
        let ok = match kind {
            "face" => self.look(uid).await?,
            "fingerprint" => {
                if self.paused(uid) {
                    return Err(Error::NoFace("Too many tries: use the password".into()));
                }
                let _guard =
                    self.reader.clone().try_lock_owned().map_err(|_| Error::Busy("The reader is in use".into()))?;
                let ok = fingerprint_matches(conn, user).await?;
                self.count(uid, ok);
                ok
            }
            _ => return Err(Error::Failed("Unknown kind".into())),
        };
        if !ok {
            return Err(Error::Failed(if kind == "face" {
                "Your face wasn't recognised. Try again or type the password.".into()
            } else {
                "Your fingerprint wasn't recognised. Try again or type the password.".into()
            }));
        }
        self.new_token(uid, agent)
    }

    /// For pam_asimaos_face.so only (root): is `token` a valid, unused token of `user`, given to the
    /// dialog process `agent` (the one polkit's helper is talking to)? It is used up either way.
    async fn consume_token(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        user: &str,
        token: &str,
        agent: u32,
    ) -> Result<bool, Error> {
        if self.polkit && Self::caller_uid(conn, &header).await? != 0 {
            return Err(Error::AccessDenied("Only the system may check a token".into()));
        }
        let uid = uid_of(user).ok_or_else(|| Error::Failed("Unknown user".into()))?;
        let entry = self.tokens.lock().unwrap().remove(token);
        Ok(entry.is_some_and(|(owner, given_to, at)| {
            owner == uid && (given_to == 0 || given_to == agent) && at.elapsed() < TOKEN_LIFE
        }))
    }

    #[zbus(signal)]
    async fn enroll_progress(emitter: &SignalEmitter<'_>, progress: f64, hint: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn enroll_done(emitter: &SignalEmitter<'_>, ok: bool, message: &str) -> zbus::Result<()>;
}

/// The process tracing this one ("TracerPid:" in /proc/PID/status), 0 when none.
fn tracer(status: &str) -> Option<u32> {
    status.lines().find_map(|l| l.strip_prefix("TracerPid:")).and_then(|v| v.trim().parse().ok())
}

/// A variable in a process's starting environment that loads foreign code into it.
fn injected(environ: &[u8]) -> Option<String> {
    const LOADERS: [&str; 9] = [
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "QT_PLUGIN_PATH",
        "QT_QPA_PLATFORM_PLUGIN_PATH",
        "QML_IMPORT_PATH",
        "QML2_IMPORT_PATH",
        "QT_QUICK_CONTROLS_STYLE_PATH",
        "GCONV_PATH",
    ];
    environ.split(|&b| b == 0).find_map(|var| {
        let name = String::from_utf8_lossy(var.split(|&b| b == b'=').next().unwrap_or_default()).into_owned();
        LOADERS.contains(&name.as_str()).then_some(name)
    })
}

fn load_picture(path: &str) -> anyhow::Result<face::Frame> {
    // Test pictures are binary PGM (P5), so no image library is needed in the service.
    let data = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&data[..data.len().min(64)]).into_owned();
    let mut it = text.split_whitespace();
    anyhow::ensure!(it.next() == Some("P5"), "test pictures must be binary PGM");
    let (w, h, max): (usize, usize, usize) =
        (it.next().context("w")?.parse()?, it.next().context("h")?.parse()?, it.next().context("max")?.parse()?);
    anyhow::ensure!(max == 255, "8-bit PGM only");
    let header_len = data.len() - w * h;
    Ok(face::Frame { width: w, height: h, channels: 1, data: data[header_len..].to_vec() })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut session, mut models, mut store, mut picture) = (false, PathBuf::from(MODELS), PathBuf::from(STORE), None);
    let mut sync_only = false;
    let (mut picture_webcam, mut conf) = (false, PathBuf::from(FACE_CONF));
    let (mut pam_file, mut pam_template) = (PathBuf::from(LOCK_SCREEN_FACE), PathBuf::from(LOCK_SCREEN_FACE_TEMPLATE));
    while let Some(a) = args.next() {
        match a.as_str() {
            "--session" => session = true,
            "--models" => models = PathBuf::from(args.next().context("--models needs a directory")?),
            "--store" => store = PathBuf::from(args.next().context("--store needs a directory")?),
            "--picture" => picture = Some(load_picture(&args.next().context("--picture needs a file")?)?),
            "--webcam" => picture_webcam = true,
            // asimaos-face-sync.service at start-up: the lock screen's Face button as the faces say.
            "--sync-lock-screen" => sync_only = true,
            "--conf" => conf = PathBuf::from(args.next().context("--conf needs a file")?),
            "--pam-file" => pam_file = PathBuf::from(args.next().context("--pam-file needs a file")?),
            "--pam-template" => pam_template = PathBuf::from(args.next().context("--pam-template needs a file")?),
            "--version" => {
                println!("asimaos-faced {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    anyhow::ensure!(picture.is_none() || session, "--picture is for tests on a private bus (--session)");
    anyhow::ensure!(!session || store != Path::new(STORE), "--session needs a scratch --store");
    anyhow::ensure!(!session || pam_file != Path::new(LOCK_SCREEN_FACE), "--session needs a scratch --pam-file");
    let store = store::Store(store);
    lock_screen_button(&store, &pam_file, &pam_template);
    if sync_only {
        return Ok(());
    }
    let service = FaceService {
        models: Arc::new(face::Models::load(&models)?),
        store: Arc::new(store),
        pam: Arc::new((pam_file, pam_template)),
        picture,
        picture_webcam,
        conf,
        polkit: !session,
        busy: Arc::new(Mutex::new(())),
        reader: Arc::new(Mutex::new(())),
        cancel: Arc::new(AtomicBool::new(false)),
        fails: std::sync::Mutex::new(HashMap::new()),
        tokens: std::sync::Mutex::new(HashMap::new()),
    };
    let builder = if session { connection::Builder::session()? } else { connection::Builder::system()? };
    let conn = builder.name(NAME)?.serve_at(PATH, service)?.build().await?;
    let idle = if session { Duration::ZERO } else { Duration::from_secs(300) };
    asimaos_svc::idle::exit_when_idle(&conn, asimaos_svc::idle::after(idle), || false);
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tampered_dialogs() {
        assert_eq!(tracer("Name:\tasimaos-polkit\nTracerPid:\t0\nUid:\t1000\n"), Some(0));
        assert_eq!(tracer("TracerPid:\t4242\n"), Some(4242));
        assert_eq!(tracer("Name:\tx\n"), None);
        assert_eq!(injected(b"HOME=/home/asha\0LANG=en_IN.UTF-8\0"), None);
        assert_eq!(injected(b"HOME=/home/asha\0LD_PRELOAD=/tmp/x.so\0"), Some("LD_PRELOAD".into()));
        assert_eq!(injected(b"QT_PLUGIN_PATH=/tmp/p\0"), Some("QT_PLUGIN_PATH".into()));
        assert_eq!(injected(b"MY_LD_PRELOAD_NOTE=1\0"), None);
    }

    #[test]
    fn lock_screen_face_button() {
        let d = std::env::temp_dir().join(format!("faced-pam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("store")).unwrap();
        let store = store::Store(d.join("store"));
        let (file, template) = (d.join("kde-face"), d.join("template"));
        std::fs::write(&template, "auth required pam_asimaos_face.so use=unlock\n").unwrap();
        lock_screen_button(&store, &file, &template);
        assert!(!file.exists(), "no face set up: no button");
        store.save(1000, &store::UserFaces::default()).unwrap();
        lock_screen_button(&store, &file, &template);
        assert!(file.exists(), "a face set up: the button");
        store.delete(1000).unwrap();
        lock_screen_button(&store, &file, &template);
        assert!(!file.exists(), "removed again");
        std::fs::remove_dir_all(d).unwrap();
    }
}
