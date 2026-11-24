//! Idle exit: a service that has had nothing to do for a while exits, so its private
//! mount namespace goes away, and with it the boot partition (ESP) mounted there. D-Bus starts the
//! service again on the next call, and the ESP is mounted again only then.
//!
//! "Nothing to do" means: no D-Bus method call for the whole idle time, no request waiting for a
//! password (`Pending`, held by `polkit::authorize`), no job of the service's own (`busy`), and no
//! helper program it started still running (TPM enrolment, btrfs, restic, …).

use futures_util::StreamExt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static PENDING: AtomicUsize = AtomicUsize::new(0);

/// Held while a request waits (for example for the administrator's password).
pub struct Pending;

impl Pending {
    pub fn hold() -> Self {
        PENDING.fetch_add(1, Ordering::SeqCst);
        Pending
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Whether a program this process started is still running.
fn has_children() -> bool {
    let me = std::process::id().to_string();
    let Ok(rd) = std::fs::read_dir("/proc") else { return false };
    rd.flatten().any(|e| {
        e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())
            && std::fs::read_to_string(e.path().join("stat"))
                .ok()
                // "pid (comm) state ppid …": comm may hold spaces, so read after the last ')'.
                .and_then(|s| s.rsplit_once(')').map(|(_, rest)| rest.split_whitespace().nth(1) == Some(me.as_str())))
                .unwrap_or(false)
    })
}

/// The idle time: `ASIMAOS_IDLE_EXIT` seconds (tests), else `default`. 0 switches idle exit off.
pub fn after(default: Duration) -> Duration {
    std::env::var("ASIMAOS_IDLE_EXIT").ok().and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(default)
}

/// Watches `conn` and ends the process (exit code 0) after `after` with nothing to do.
pub fn exit_when_idle(conn: &zbus::Connection, after: Duration, busy: impl Fn() -> bool + Send + 'static) {
    if after.is_zero() {
        return;
    }
    let last = Arc::new(AtomicU64::new(now()));
    let seen = last.clone();
    let mut calls = zbus::MessageStream::from(conn);
    tokio::spawn(async move {
        while let Some(m) = calls.next().await {
            if m.is_ok_and(|m| m.message_type() == zbus::message::Type::MethodCall) {
                seen.store(now(), Ordering::SeqCst);
            }
        }
    });
    let every = Duration::from_secs((after.as_secs() / 4).clamp(1, 15));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            if busy() || PENDING.load(Ordering::SeqCst) > 0 || has_children() {
                last.store(now(), Ordering::SeqCst);
                continue;
            }
            if now().saturating_sub(last.load(Ordering::SeqCst)) >= after.as_secs() {
                std::process::exit(0);
            }
        }
    });
}
