//! EFI variables: Secure Boot state and systemd-boot's loader interface.

use crate::Root;

pub const GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
pub const LOADER: &str = "4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";

/// A variable's data (without its 4 attribute bytes).
pub fn read(root: &Root, name: &str, guid: &str) -> Option<Vec<u8>> {
    let data = std::fs::read(root.path(&format!("/sys/firmware/efi/efivars/{name}-{guid}"))).ok()?;
    (data.len() > 4).then(|| data[4..].to_vec()) // first 4 bytes: attributes
}

/// A UTF-16LE string variable (systemd-boot's LoaderEntry* variables).
pub fn string(root: &Root, name: &str, guid: &str) -> Option<String> {
    let data = read(root, name, guid)?;
    let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    let s = String::from_utf16_lossy(&units);
    Some(s.trim_end_matches('\0').to_string())
}

pub fn is_efi(root: &Root) -> bool {
    root.path("/sys/firmware/efi").is_dir()
}

/// "on", "off" or "unsupported" (not booted in UEFI mode).
pub fn secure_boot(root: &Root) -> &'static str {
    if !is_efi(root) {
        return "unsupported";
    }
    match read(root, "SecureBoot", GLOBAL) {
        Some(v) if v.first() == Some(&1) => "on",
        _ => "off",
    }
}

// ───────────────────────────────────────────────────────────────────────── boot entries

/// The ESP's place on its disk, for a firmware boot entry: partition number, start and size in the
/// disk's logical blocks, and its GPT GUID.
#[derive(Clone, Debug, PartialEq)]
pub struct Partition {
    pub number: u32,
    pub start: u64,
    pub size: u64,
    pub guid: String,
}

/// The partition behind `dev` (a /dev path), read from sysfs.
pub fn partition_of(root: &Root, dev: &std::path::Path, guid: &str) -> Option<Partition> {
    let name = std::fs::canonicalize(dev).ok()?.file_name()?.to_string_lossy().into_owned();
    let sys = root.path(&format!("/sys/class/block/{name}"));
    let num = |f: &str| std::fs::read_to_string(sys.join(f)).ok()?.trim().parse::<u64>().ok();
    // sysfs counts 512-byte sectors; the firmware counts the disk's logical blocks.
    let disk = std::fs::canonicalize(sys.join("..")).ok()?;
    let lbs = std::fs::read_to_string(disk.join("queue/logical_block_size")).ok()?.trim().parse::<u64>().ok()?;
    let per = (lbs / 512).max(1);
    Some(Partition {
        number: num("partition")? as u32,
        start: num("start")? / per,
        size: num("size")? / per,
        guid: guid.to_lowercase(),
    })
}

/// "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" → the 16 bytes EFI stores (the first three fields
/// little-endian).
/// A GUID as EFI stores it (the first three fields little-endian).
pub fn guid_bytes(guid: &str) -> Option<[u8; 16]> {
    let hex: String = guid.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut b = [0u8; 16];
    for (i, byte) in b.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    b[0..4].reverse();
    b[4..6].reverse();
    b[6..8].reverse();
    Some(b)
}

/// An EFI_LOAD_OPTION that starts `file` (like "\EFI\asimaos\trial\shimx64.efi") from `part`.
pub fn load_option(description: &str, part: &Partition, file: &str) -> Option<Vec<u8>> {
    let mut path = Vec::new();
    // Hard drive media path: GPT partition.
    path.extend_from_slice(&[0x04, 0x01, 42, 0]);
    path.extend_from_slice(&part.number.to_le_bytes());
    path.extend_from_slice(&part.start.to_le_bytes());
    path.extend_from_slice(&part.size.to_le_bytes());
    path.extend_from_slice(&guid_bytes(&part.guid)?);
    path.extend_from_slice(&[0x02, 0x02]); // GPT, GUID signature
                                           // File path.
    let name: Vec<u8> = file.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect();
    path.extend_from_slice(&[0x04, 0x04]);
    path.extend_from_slice(&((4 + name.len()) as u16).to_le_bytes());
    path.extend_from_slice(&name);
    // End of the whole path.
    path.extend_from_slice(&[0x7f, 0xff, 0x04, 0x00]);
    let mut opt = Vec::new();
    opt.extend_from_slice(&1u32.to_le_bytes()); // LOAD_OPTION_ACTIVE
    opt.extend_from_slice(&(path.len() as u16).to_le_bytes());
    opt.extend(description.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()));
    opt.extend_from_slice(&path);
    Some(opt)
}

fn var_path(root: &Root, name: &str, guid: &str) -> std::path::PathBuf {
    root.path(&format!("/sys/firmware/efi/efivars/{name}-{guid}"))
}

/// efivarfs marks existing variables immutable; clears that so one can be changed or removed.
fn make_mutable(path: &std::path::Path) {
    use std::os::fd::AsRawFd;
    const FS_IOC_GETFLAGS: libc::c_ulong = 0x8008_6601;
    const FS_IOC_SETFLAGS: libc::c_ulong = 0x4008_6602;
    const FS_IMMUTABLE_FL: libc::c_long = 0x10;
    if let Ok(f) = std::fs::File::open(path) {
        let mut flags: libc::c_long = 0;
        // SAFETY: plain ioctls on an open file descriptor with a pointer to a local integer.
        unsafe {
            if libc::ioctl(f.as_raw_fd(), FS_IOC_GETFLAGS, &mut flags) == 0 && flags & FS_IMMUTABLE_FL != 0 {
                flags &= !FS_IMMUTABLE_FL;
                libc::ioctl(f.as_raw_fd(), FS_IOC_SETFLAGS, &flags);
            }
        }
    }
}

/// Writes a variable that persists (non-volatile, boot and runtime access), in one write as
/// efivarfs needs.
pub fn write(root: &Root, name: &str, guid: &str, data: &[u8]) -> std::io::Result<()> {
    let path = var_path(root, name, guid);
    make_mutable(&path);
    let mut all = 7u32.to_le_bytes().to_vec();
    all.extend_from_slice(data);
    std::fs::write(path, all)
}

/// Writes a variable with exactly these bytes (the 4 attribute bytes first), as authenticated
/// variables (Secure Boot's db, KEK, PK) need.
pub fn write_raw(root: &Root, name: &str, guid: &str, all: &[u8]) -> std::io::Result<()> {
    let path = var_path(root, name, guid);
    make_mutable(&path);
    std::fs::write(path, all)
}

pub fn delete(root: &Root, name: &str, guid: &str) {
    let path = var_path(root, name, guid);
    make_mutable(&path);
    let _ = std::fs::remove_file(path);
}

/// A Boot#### number nobody uses.
pub fn free_boot_number(root: &Root) -> Option<u16> {
    let used: Vec<u16> = std::fs::read_dir(root.path("/sys/firmware/efi/efivars"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let hex = n.strip_prefix("Boot")?.split('-').next()?.to_string();
            (hex.len() == 4).then(|| u16::from_str_radix(&hex, 16).ok()).flatten()
        })
        .collect();
    (0x0100u16..0xfff0).find(|n| !used.contains(n))
}

/// The firmware's start order (BootOrder): Boot#### numbers, first tried first.
pub fn boot_order(root: &Root) -> Vec<u16> {
    read(root, "BootOrder", GLOBAL)
        .map(|d| d.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect())
        .unwrap_or_default()
}

/// The Boot#### entry this start came from.
pub fn boot_current(root: &Root) -> Option<u16> {
    read(root, "BootCurrent", GLOBAL).filter(|d| d.len() >= 2).map(|d| u16::from_le_bytes([d[0], d[1]]))
}

/// A Boot#### entry's name ("Windows Boot Manager"), from its EFI_LOAD_OPTION.
pub fn boot_description(root: &Root, number: u16) -> Option<String> {
    let d = read(root, &format!("Boot{number:04X}"), GLOBAL)?;
    let units: Vec<u16> =
        d.get(6..)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&u| u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

pub fn set_boot_order(root: &Root, order: &[u16]) -> std::io::Result<()> {
    write(root, "BootOrder", GLOBAL, &order.iter().flat_map(|n| n.to_le_bytes()).collect::<Vec<u8>>())
}

/// Starts Boot#### `number` at the next start only (the firmware forgets BootNext once used).
pub fn set_boot_next(root: &Root, number: u16) -> std::io::Result<()> {
    write(root, "BootNext", GLOBAL, &number.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_option_layout() {
        let part =
            Partition { number: 1, start: 2048, size: 1_048_576, guid: "c12a7328-f81f-11d2-ba4b-00a0c93ec93b".into() };
        let o = load_option("Asima OS", &part, "\\EFI\\asimaos\\trial\\shimx64.efi").unwrap();
        assert_eq!(&o[0..4], &[1, 0, 0, 0]);
        let path_len = u16::from_le_bytes([o[4], o[5]]) as usize;
        let desc_len = ("Asima OS".len() + 1) * 2;
        assert_eq!(o.len(), 6 + desc_len + path_len);
        let path = &o[6 + desc_len..];
        assert_eq!(&path[0..4], &[0x04, 0x01, 42, 0]);
        // The GUID's first field is stored little-endian.
        assert_eq!(&path[24..28], &[0x28, 0x73, 0x2a, 0xc1]);
        assert_eq!(&path[40..42], &[0x02, 0x02]);
        assert_eq!(&path[42..44], &[0x04, 0x04]);
        assert_eq!(&path[path.len() - 4..], &[0x7f, 0xff, 0x04, 0x00]);
        assert!(load_option("x", &Partition { guid: "nope".into(), ..part }, "\\a").is_none());
    }

    #[test]
    fn variables_and_numbers() {
        let dir = std::env::temp_dir().join(format!("efi-{}", std::process::id()));
        let vars = dir.join("sys/firmware/efi/efivars");
        std::fs::create_dir_all(&vars).unwrap();
        let root = Root(dir.clone());
        std::fs::write(vars.join(format!("Boot0100-{GLOBAL}")), [7, 0, 0, 0, 1]).unwrap();
        assert_eq!(free_boot_number(&root), Some(0x0101));
        set_boot_next(&root, 0x0101).unwrap();
        assert_eq!(std::fs::read(vars.join(format!("BootNext-{GLOBAL}"))).unwrap(), vec![7, 0, 0, 0, 0x01, 0x01]);
        delete(&root, "BootNext", GLOBAL);
        assert!(!vars.join(format!("BootNext-{GLOBAL}")).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
