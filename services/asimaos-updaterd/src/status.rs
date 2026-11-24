//! Everything Asima Updater shows, as one JSON object: the running version and every version in
//! the slots or the start menu, with its boot state.

use crate::update;
use asimaos_svc::{efi, system, Root};
use asimaos_update::boot;
use serde_json::{json, Value};
use std::fs;

pub fn status(root: &Root) -> Value {
    let running = system::running_version(root);
    let selected = efi::string(root, "LoaderEntrySelected", efi::LOADER).unwrap_or_default();
    let oneshot = efi::string(root, "LoaderEntryOneShot", efi::LOADER).unwrap_or_default();
    let esp = update::esp(root);
    let entries = boot::entries(&esp);
    let slots = root.path(update::SLOTS);

    let versions: Vec<Value> = update::installed_versions(root)
        .into_iter()
        .map(|v| {
            let entry = entries.iter().find(|e| e.version == v);
            let uki_present = esp.join(boot::UKI_DIR).join(boot::uki_name(&v)).is_file();
            let image = slots.join(format!("usr_{v}.erofs"));
            let size: u64 = [image.clone(), slots.join(format!("verity_{v}.img"))]
                .iter()
                .filter_map(|p| fs::metadata(p).ok())
                .map(|m| m.len())
                .sum();
            let installed = fs::metadata(&image)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            json!({
                "version": v,
                "running": v == running,
                "image": image.is_file() && slots.join(format!("verity_{v}.img")).is_file(),
                // current / backup / failed in the start menu (none: not in the menu any more)
                "role": entry.map(|e| e.role.name()),
                "bootable": entry.is_some() && uki_present,
                // No counter left in the entry's name means systemd-bless-boot marked it good.
                "blessed": entry.is_some_and(|e| e.blessed()),
                "triesLeft": entry.and_then(|e| e.tries.map(|t| t.0)),
                "triesDone": entry.and_then(|e| e.tries.map(|t| t.1)),
                "booted": entry.is_some_and(|e| selected == e.id()),
                "nextOnce": entry.is_some_and(|e| oneshot == e.id()),
                "size": size,
                "installed": installed,
            })
        })
        .collect();

    json!({
        "contract": asimaos_svc::CONTRACT,
        // Settings go back at the next start to before the update to this version.
        "settingsRollback": std::fs::read_to_string(root.path(update::ROLLBACK_FLAG)).unwrap_or_default().trim(),
        "version": running,
        "versions": versions,
        // Free space for updates, on the pool that holds the slots.
        "free": asimaos_svc::free_bytes(&root.path(update::SLOTS)).unwrap_or(0),
        "rescue": esp.join(boot::UKI_DIR).join(boot::RESCUE_UKI).is_file(),
    })
}
