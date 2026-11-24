//! Everything Asima Maintenance shows, as one JSON object: storage, security, snapshots,
//! backups, the staged reset and the /etc files waiting to be restored.

use crate::{backup, bootorder, crashes, etc, schedule, security};
use asimaos_svc::{cmd, snapshots, system, Root};
use asimaos_update::boot;
use serde_json::{json, Value};
use std::fs;

pub async fn status(root: &Root, pool: &str) -> Value {
    let home = system::pool_path(root);
    let data = root.path(system::DATA);
    let reset = data.join("flags/reset.json");
    json!({
        "contract": asimaos_svc::CONTRACT,
        // Your changed /etc files whose factory default changed with an update.
        "etcNewDefaults": std::fs::read_to_string(root.path("/var/lib/asimaos/etc-new-defaults"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect::<Vec<_>>(),
        "version": system::running_version(root),
        "storage": {
            "total": asimaos_svc::total_bytes(&home),
            "free": asimaos_svc::free_bytes(&home),
            "reserve": root.path("/run/initramfs/reserve/.emergency_ballast").is_file(),
        },
        "security": security::status(root).await,
        "snapshots": match snapshots::Pool::open(root, pool).await {
            Ok(p) => Value::Array(p.list()),
            Err(_) => Value::Array(Vec::new()),
        },
        "backup": backup::status(root),
        "snapshotSchedule": schedule::get(root),
        "crashes": crashes::list(root),
        "rescue": system::esp(root).join(boot::UKI_DIR).join(boot::RESCUE_UKI).is_file(),
        "reset": {
            "prepared": reset.is_file(),
            "scheduled": reset.is_file() && fs::read_to_string(&reset).is_ok_and(|t| t.contains("\"scheduled\": true")),
        },
        "restoreScheduled": etc::scheduled(root),
        "settingsReset": root.path(etc::RESET_PEOPLE).is_file(),
        "bootOrder": bootorder::status(root),
        "tools": {
            "mokutil": cmd::exists("mokutil"),
            "cryptenroll": cmd::exists("systemd-cryptenroll"),
            "bootctl": cmd::exists("bootctl"),
        },
    })
}
