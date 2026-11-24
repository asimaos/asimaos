//! Failed starts and kernel crashes, for Asima Maintenance → Health.
//!
//! - A failed start: asimaos-health keeps one small JSON file per failure in
//!   `/var/lib/asimaos/failed-starts` (the newest 10): the version, the check that failed and why.
//! - A kernel crash: the kernel leaves its last messages in an EFI variable (efi_pstore, switched
//!   on in the boot image's command line); systemd-pstore moves them to
//!   `/var/lib/systemd/pstore/<id>/` at the next start.
//!
//! Read-only; newest first, at most 10 of each.

use asimaos_svc::Root;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub const FAILED_STARTS: &str = "/var/lib/asimaos/failed-starts";
pub const PSTORE: &str = "/var/lib/systemd/pstore";
const MAX: usize = 10;
const LOG_LINES: usize = 40;

fn mtime(p: &Path) -> i64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sorted_entries(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort_by_key(|p| std::cmp::Reverse(mtime(p)));
    v
}

/// The last `n` lines of the kernel's messages in one pstore record (its dmesg* files in order).
fn crash_log(dir: &Path, n: usize) -> String {
    let mut parts: Vec<_> = fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    parts.retain(|p| p.file_name().is_some_and(|f| f.to_string_lossy().starts_with("dmesg")));
    parts.sort();
    let text: String =
        parts.iter().filter_map(|p| fs::read(p).ok()).map(|b| String::from_utf8_lossy(&b).into_owned()).collect();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

pub fn list(root: &Root) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for p in sorted_entries(&root.path(FAILED_STARTS)).into_iter().take(MAX) {
        if let Some(v) = fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            out.push(json!({
                "kind": "start",
                "at": v["at"], "version": v["version"], "check": v["check"], "detail": v["detail"],
            }));
        }
    }
    for p in sorted_entries(&root.path(PSTORE)).into_iter().filter(|p| p.is_dir()).take(MAX) {
        out.push(json!({ "kind": "crash", "at": mtime(&p), "log": crash_log(&p, LOG_LINES) }));
    }
    out.sort_by_key(|v| std::cmp::Reverse(v["at"].as_i64().unwrap_or(0)));
    Value::Array(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_starts_and_crashes() {
        let t = std::env::temp_dir().join(format!("asimaos-crashes-{}", std::process::id()));
        let root = Root(t.clone());
        fs::create_dir_all(root.path(FAILED_STARTS)).unwrap();
        fs::write(
            root.path(FAILED_STARTS).join("100-desktop.json"),
            r#"{"at": 100, "version": "2026.12.01.1", "check": "desktop", "detail": "no login screen"}"#,
        )
        .unwrap();
        let rec = root.path(PSTORE).join("abc");
        fs::create_dir_all(&rec).unwrap();
        let log: String = (1..=60).map(|i| format!("line {i}\n")).collect();
        fs::write(rec.join("dmesg-efi-1"), log).unwrap();
        let v = list(&root);
        let a = v.as_array().unwrap();
        assert_eq!(a.len(), 2);
        let crash = a.iter().find(|c| c["kind"] == "crash").unwrap();
        assert!(
            crash["log"].as_str().unwrap().ends_with("line 60")
                && !crash["log"].as_str().unwrap().contains("line 20\n")
        );
        assert!(a.iter().any(|c| c["kind"] == "start" && c["detail"] == "no login screen"));
        let _ = fs::remove_dir_all(t);
    }
}
