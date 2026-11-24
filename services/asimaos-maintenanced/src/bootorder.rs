//! "Start Asima OS first": Windows updates (and some firmware updates) can put their own
//! entry at the front of the firmware's start order, so the computer starts Windows unless
//! Asima OS is picked by hand. The entry this start came from (BootCurrent) is Asima OS's own;
//! one click moves it back to the front of BootOrder.

use anyhow::{ensure, Context, Result};
use asimaos_svc::{efi, Root};
use serde_json::{json, Value};

/// A boot loader on trial (an update's new one) starts through a temporary entry: the order is
/// left alone until the trial is over (asimaos-updaterd).
const TRIAL_STATE: &str = "/var/lib/asimaos/loader-trial";

pub fn status(root: &Root) -> Value {
    let order = efi::boot_order(root);
    let (Some(current), Some(&first)) = (efi::boot_current(root), order.first()) else {
        return json!({ "known": false });
    };
    let on_trial = root.path(TRIAL_STATE).exists();
    json!({
        "known": !on_trial,
        "asimaosFirst": on_trial || first == current,
        "first": efi::boot_description(root, first).unwrap_or_else(|| format!("Boot{first:04X}")),
    })
}

/// Moves the entry this start came from to the front of the start order.
pub fn start_asimaos_first(root: &Root) -> Result<()> {
    ensure!(efi::is_efi(root), "This computer didn't start in UEFI mode");
    ensure!(!root.path(TRIAL_STATE).exists(), "A new boot loader is on trial: try again after the next restart");
    let current = efi::boot_current(root).context("The firmware doesn't say which entry started Asima OS")?;
    let mut order = efi::boot_order(root);
    order.retain(|&n| n != current);
    order.insert(0, current);
    efi::set_boot_order(root, &order).context("The firmware's start order could not be changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(root: &Root, name: &str, data: &[u8]) {
        let p = root.path(&format!("/sys/firmware/efi/efivars/{name}-{}", efi::GLOBAL));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut all = vec![7, 0, 0, 0];
        all.extend_from_slice(data);
        std::fs::write(p, all).unwrap();
    }

    fn option(name: &str) -> Vec<u8> {
        let mut o = vec![1, 0, 0, 0, 4, 0];
        o.extend(name.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()));
        o.extend([0x7f, 0xff, 0x04, 0x00]);
        o
    }

    #[test]
    fn windows_first_then_asimaos() {
        let d = std::env::temp_dir().join(format!("md-boot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let root = Root(d.clone());
        var(&root, "Boot0000", &option("Windows Boot Manager"));
        var(&root, "Boot0003", &option("Asima OS"));
        var(&root, "BootOrder", &[0, 0, 3, 0, 1, 0]);
        var(&root, "BootCurrent", &[3, 0]);
        let s = status(&root);
        assert_eq!(s["asimaosFirst"], false);
        assert_eq!(s["first"], "Windows Boot Manager");
        start_asimaos_first(&root).unwrap();
        assert_eq!(efi::boot_order(&root), vec![3, 0, 1]);
        assert_eq!(status(&root)["asimaosFirst"], true);
        // While a new boot loader is on trial, the order stays as it is.
        var(&root, "BootOrder", &[0, 0, 3, 0]);
        std::fs::create_dir_all(root.path("/var/lib/asimaos")).unwrap();
        std::fs::write(root.path(TRIAL_STATE), "0100 1\n").unwrap();
        assert_eq!(status(&root)["known"], false);
        assert!(start_asimaos_first(&root).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
}
