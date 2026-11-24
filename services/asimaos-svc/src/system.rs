//! Where an installed Asima OS keeps its parts (docs/architecture.md §4), and the facts several
//! services need: the running version and a path on the pool for free-space questions.

use crate::Root;
use std::fs;
use std::path::PathBuf;

/// The OS images (`usr_<version>.erofs` + `verity_<version>.img`), mounted read-only.
pub const SLOTS: &str = "/run/initramfs/slots";
/// The EFI system partition: boot loader, start menu, each version's boot image, Asima Rescue.
/// It is not mounted for the desktop (an accident, a careless script or malware can't
/// reach it); the services that need it mount it at /boot in their own private mount namespace
/// with [`open_esp`].
pub const ESP: &str = "/boot";
/// The pool's @data subvolume: the /etc overlay and the flags the initramfs acts on.
pub const DATA: &str = "/run/initramfs/data";

pub fn esp(root: &Root) -> PathBuf {
    root.path(ESP)
}

/// The ESP this computer started from: the partition systemd-boot reports (LoaderDevicePartUUID),
/// else the partition labelled ESP.
pub fn esp_device() -> Option<PathBuf> {
    let var = "/sys/firmware/efi/efivars/LoaderDevicePartUUID-4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";
    if let Ok(raw) = fs::read(var) {
        // 4 bytes of attributes, then a UTF-16LE string.
        let utf16: Vec<u16> =
            raw.get(4..).unwrap_or(&[]).as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let uuid = String::from_utf16_lossy(&utf16).trim_end_matches('\0').to_lowercase();
        let dev = PathBuf::from(format!("/dev/disk/by-partuuid/{uuid}"));
        if !uuid.is_empty() && dev.exists() {
            return Some(dev);
        }
    }
    let dev = PathBuf::from("/dev/disk/by-partlabel/ESP");
    dev.exists().then_some(dev)
}

/// Mounts the ESP at /boot for this process. Only for a service with its own mount
/// namespace (PrivateMounts=yes): the desktop never sees it. Nothing to do in test mode or when
/// /boot is already mounted (an administrator opened it by hand).
pub async fn open_esp(root: &Root) -> anyhow::Result<()> {
    if !root.is_system() || crate::cmd::probe("mountpoint", &["-q", ESP]).await.is_some() {
        return Ok(());
    }
    let dev = esp_device().ok_or_else(|| anyhow::anyhow!("The boot partition (ESP) was not found"))?;
    // The root is new in RAM at every start; tmpfiles makes /boot, but never count on the order.
    let _ = std::fs::create_dir_all(ESP);
    crate::cmd::run("mount", &["-t", "vfat", "-o", "umask=0077,noatime,flush", &dev.to_string_lossy(), ESP])
        .await
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("The boot partition could not be opened: {e}"))
}

/// The version this system is running (os-release IMAGE_VERSION).
pub fn running_version(root: &Root) -> String {
    for path in ["/usr/lib/os-release", "/etc/os-release"] {
        if let Ok(text) = fs::read_to_string(root.path(path)) {
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("IMAGE_VERSION=") {
                    return v.trim_matches('"').to_string();
                }
            }
        }
    }
    String::new()
}

/// A path on the Btrfs pool for free-space questions. Not /home: the services run with
/// ProtectHome=yes, so /home is an empty placeholder in their view. /var is on the same pool.
pub fn pool_path(root: &Root) -> PathBuf {
    if root.is_system() {
        PathBuf::from("/var")
    } else {
        root.path("/home")
    }
}
