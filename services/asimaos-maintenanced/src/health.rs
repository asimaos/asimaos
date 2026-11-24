//! The health check behind Asima Maintenance's Health page. Read-only; ids are mapped to words
//! by the GUI.

use asimaos_svc::{cmd, efi, system, Root};
use serde_json::{json, Value};

const GIB: u64 = 1024 * 1024 * 1024;
pub const MAINTENANCE: &str = "/var/lib/asimaos/maintenance";

pub async fn run(root: &Root) -> Value {
    let mut checks = Vec::new();
    let mut add = |id: &str, ok: bool, detail: String| checks.push(json!({ "id": id, "ok": ok, "detail": detail }));

    let verity = cmd::probe("dmsetup", &["status", "usr"]).await.unwrap_or_default();
    add(
        "system-files",
        verity.contains(" verity V"),
        if verity.contains(" verity C") { "dm-verity found damaged blocks".into() } else { String::new() },
    );

    let running = system::running_version(root);
    let blessed = asimaos_update::boot::entries(&system::esp(root)).iter().any(|e| e.version == running && e.blessed());
    add("boot", blessed, running);

    let home = system::pool_path(root);
    let (free, total) = (asimaos_svc::free_bytes(&home).unwrap_or(0), asimaos_svc::total_bytes(&home).unwrap_or(1));
    add("space", free >= 10 * GIB || free * 10 >= total, format!("{free}"));

    // `btrfs device stats -c` fails when any error counter isn't zero.
    let fs_ok = cmd::run("btrfs", &["device", "stats", "-c", &home.to_string_lossy()]).await.is_ok();
    add("filesystem", fs_ok, String::new());

    let failed = cmd::probe("systemctl", &["--failed", "--no-legend", "--plain"]).await.unwrap_or_default();
    let failed: Vec<&str> = failed.lines().filter_map(|l| l.split_whitespace().next()).collect();
    add("services", failed.is_empty(), failed.join(" "));

    add("secure-boot", efi::secure_boot(root) == "on", efi::secure_boot(root).into());
    add("reserve", root.path("/run/initramfs/reserve/.emergency_ballast").is_file(), String::new());

    // Results of the self-maintenance timers (asimaos-maintenance), when they have run.
    let read = |name: &str| {
        std::fs::read_to_string(root.path(&format!("{}/{name}", MAINTENANCE)))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    };
    if let Some(s) = read("scrub.json") {
        add("scrub", s["ok"] == true, s["detail"].as_str().unwrap_or("").to_string());
    }
    if let Some(s) = read("ssd.json") {
        let used = s["percentageUsed"].as_u64().unwrap_or(0);
        add("ssd", used < 90, format!("{used}"));
    }

    let ok = checks.iter().all(|c| c["ok"] == true || c["id"] == "secure-boot");
    json!({ "checks": checks, "ok": ok })
}
