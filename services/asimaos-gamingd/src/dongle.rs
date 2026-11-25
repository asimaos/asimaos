//! The Xbox Wireless Adapter (D55): xone drives it, but its firmware is Microsoft's and isn't
//! Asima OS's to ship. When an adapter is plugged in without it, Asima Gaming says so and, when the
//! person asks, this fetches the firmware from Microsoft's own Windows Update download (the files
//! and SHA-256 sums xone's own install uses), into /var/lib/asimaos/firmware, where the kernel
//! looks for firmware too (`firmware_class.path`, the boot image's command line). The adapter is
//! then started again with it.

use anyhow::{bail, ensure, Context, Result};
use asimaos_svc::{cmd, Root};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const FIRMWARE_DIR: &str = "/var/lib/asimaos/firmware";

/// Each adapter's product ID: Microsoft's driver package, the file in it, and that file's SHA-256.
const ADAPTERS: [(&str, &str, &str, &str); 4] = [
    (
        "02e6",
        "https://catalog.s.download.windowsupdate.com/d/msdownload/update/driver/drvs/2017/03/2ea9591b-f751-442c-80ce-8f4692cdc67b_6b555a3a288153cf04aec6e03cba360afe2fce34.cab",
        "FW_ACC_00U.bin",
        "080ce4091e53a4ef3e5fe29939f51fd91f46d6a88be6d67eb6e99a5723b3a223",
    ),
    (
        "02fe",
        "https://catalog.s.download.windowsupdate.com/c/msdownload/update/driver/drvs/2017/07/1cd6a87c-623f-4407-a52d-c31be49e925c_e19f60808bdcbfbd3c3df6be3e71ffc52e43261e.cab",
        "FW_ACC_00U.bin",
        "48084d9fa53b9bb04358f3bb127b7495dc8f7bb0b3ca1437bd24ef2b6eabdf66",
    ),
    (
        "02f9",
        "https://catalog.s.download.windowsupdate.com/c/msdownload/update/driver/drvs/2017/06/1dbd7cb4-53bc-4857-a5b0-5955c8acaf71_9081931e7d664429a93ffda0db41b7545b7ac257.cab",
        "FW_ACC_CL.bin",
        "0023a7bae02974834500c665a281e25b1ba52c9226c84989f9084fa5ce591d9b",
    ),
    (
        "091e",
        "https://catalog.s.download.windowsupdate.com/d/msdownload/update/driver/drvs/2017/08/aeff215c-3bc4-4d36-a3ea-e14bfa8fa9d2_e58550c4f74a27e51e5cb6868b10ff633fa77164.cab",
        "FW_ACC_BR.bin",
        "e2710daf81e7b36d35985348f68a81d18bc537a2b0c508ffdfde6ac3eae1bad7",
    ),
];

fn firmware_file(root: &Root, pid: &str) -> std::path::PathBuf {
    root.path(FIRMWARE_DIR).join(format!("xone_dongle_{pid}.bin"))
}

/// The adapters plugged in now: (USB device name, product ID).
fn plugged(root: &Root) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root.path("/sys/bus/usb/devices")) else { return out };
    for e in rd.flatten() {
        let read = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default().trim().to_string();
        let pid = read("idProduct");
        if read("idVendor") == "045e" && ADAPTERS.iter().any(|a| a.0 == pid) {
            out.push((e.file_name().to_string_lossy().into_owned(), pid));
        }
    }
    out
}

/// Whether this version has xone's adapter driver: the build leaves it out when xone doesn't build
/// for the kernel (stage 16), and the adapter then isn't supported in that version.
fn driver(root: &Root) -> bool {
    let release = std::fs::read_to_string(root.path("/proc/sys/kernel/osrelease")).unwrap_or_default();
    root.path("/usr/lib/modules").join(release.trim()).join("updates/asimaos/xone_dongle.ko.zst").is_file()
}

/// For Asima Gaming: each adapter plugged in, whether this version has its driver, and whether its
/// firmware is here.
pub fn status(root: &Root) -> Value {
    let driver = driver(root);
    Value::Array(
        plugged(root)
            .into_iter()
            .map(|(_, pid)| json!({ "product": pid, "driver": driver, "firmware": firmware_file(root, &pid).is_file() }))
            .collect(),
    )
}

/// Fetches the firmware for adapter `pid` from Microsoft, checks it, keeps it, and starts the
/// adapter again with it.
pub async fn get_firmware(root: &Root, pid: &str) -> Result<()> {
    let (_, url, member, sum) =
        ADAPTERS.iter().find(|a| a.0 == pid).copied().context("That isn't an Xbox Wireless Adapter")?;
    ensure!(driver(root), "This version of Asima OS has no driver for the Xbox Wireless Adapter");
    let work = root.path("/var/cache/asimaos-gamingd");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let cab = work.join("driver.cab");
    let cab_s = cab.to_string_lossy().into_owned();
    cmd::run("curl", &["-sSfL", "--max-time", "300", "--retry", "2", "-o", &cab_s, url])
        .await
        .context("Microsoft's download can't be reached right now")?;
    let work_s = work.to_string_lossy().into_owned();
    cmd::run("bsdtar", &["-xf", &cab_s, "-C", &work_s, member]).await.context("The firmware isn't in Microsoft's file")?;
    let data = std::fs::read(work.join(member))?;
    ensure!(hex::encode(Sha256::digest(&data)) == sum, "The firmware from Microsoft isn't the one expected; nothing was kept");
    let dest = firmware_file(root, pid);
    std::fs::create_dir_all(dest.parent().unwrap())?;
    let tmp = dest.with_extension("bin.part");
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &dest)?;
    let _ = std::fs::remove_dir_all(&work);
    restart(root, pid);
    Ok(())
}

/// The adapter's driver tries again, now with its firmware.
fn restart(root: &Root, pid: &str) {
    if !root.is_system() {
        return;
    }
    for (dev, p) in plugged(root) {
        if p != pid {
            continue;
        }
        // Its interface (DEVICE:1.0) is what xone-dongle drives.
        let intf = format!("{dev}:1.0");
        let drv = Path::new("/sys/bus/usb/drivers/xone-dongle");
        if drv.join(&intf).exists() {
            let _ = std::fs::write(drv.join("unbind"), &intf);
        }
        let _ = std::fs::write("/sys/bus/usb/drivers_probe", &intf);
    }
}

/// Only these four product IDs are known.
pub fn check_pid(pid: &str) -> Result<()> {
    if !ADAPTERS.iter().any(|a| a.0 == pid) {
        bail!("That isn't an Xbox Wireless Adapter");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_adapters() {
        let t = std::env::temp_dir().join(format!("asimaos-dongle-{}", std::process::id()));
        for (dev, vid, pid) in [("1-2", "045e", "02fe"), ("1-3", "045e", "0b12"), ("1-4", "046d", "02fe")] {
            let d = t.join("sys/bus/usb/devices").join(dev);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("idVendor"), format!("{vid}\n")).unwrap();
            std::fs::write(d.join("idProduct"), format!("{pid}\n")).unwrap();
        }
        let root = Root(t.clone());
        assert_eq!(status(&root), json!([{ "product": "02fe", "driver": false, "firmware": false }]));
        std::fs::create_dir_all(t.join("proc/sys/kernel")).unwrap();
        std::fs::write(t.join("proc/sys/kernel/osrelease"), "7.3.4-arch1-1\n").unwrap();
        let module = t.join("usr/lib/modules/7.3.4-arch1-1/updates/asimaos/xone_dongle.ko.zst");
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(&module, b"ko").unwrap();
        std::fs::create_dir_all(firmware_file(&root, "02fe").parent().unwrap()).unwrap();
        std::fs::write(firmware_file(&root, "02fe"), b"fw").unwrap();
        assert_eq!(status(&root), json!([{ "product": "02fe", "driver": true, "firmware": true }]));
        assert!(check_pid("091e").is_ok() && check_pid("0b12").is_err());
        let _ = std::fs::remove_dir_all(t);
    }
}
