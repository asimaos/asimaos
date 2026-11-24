//! Shared plumbing for the Asima OS system services (asimaos-updaterd, asimaos-maintenanced,
//! asimaos-managerd, asimaos-gamingd): polkit checks, running system tools without putting secrets on
//! their command line, small validation helpers, EFI variables, where the system keeps its parts,
//! the snapshots, idle exit, and a filesystem `Root` that tests point at a scratch tree.

pub mod authenticode;
pub mod cmd;
pub mod efi;
pub mod idle;
pub mod polkit;
pub mod snapshots;
pub mod system;
pub mod version;

use std::path::{Path, PathBuf};

/// The version of the services' contracts: every Status() carries it as "contract". It
/// goes up only when a field disappears or changes meaning; new fields keep it. Apps that ship on
/// their own (Asima Hub, a newer Asima OS app on an older system) check it before reading.
pub const CONTRACT: u32 = 1;

/// Where a service finds the system. Production: `/`. Tests: a scratch directory that mimics it.
#[derive(Clone, Debug)]
pub struct Root(pub PathBuf);

impl Root {
    pub fn system() -> Self {
        Root(PathBuf::from("/"))
    }

    /// `root.path("/run/initramfs/slots")` → `<root>/run/initramfs/slots`.
    pub fn path(&self, absolute: &str) -> PathBuf {
        self.0.join(absolute.trim_start_matches('/'))
    }

    pub fn is_system(&self) -> bool {
        self.0 == Path::new("/")
    }
}

/// Usernames the installer and the reset accept (same rule as `asimaos-install --user`).
pub fn valid_username(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    name.len() <= 32 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Display names end up in /etc/passwd: no colon, no control characters, at most 64 characters.
pub fn valid_fullname(name: &str) -> bool {
    name.chars().count() <= 64 && !name.chars().any(|c| c == ':' || c.is_control())
}

/// Free bytes on the filesystem that holds `path`.
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` a properly sized out-parameter.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_bavail as u64 * st.f_frsize as u64)
}

/// Total bytes of the filesystem that holds `path`.
pub fn total_bytes(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some(st.f_blocks as u64 * st.f_frsize as u64)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm), to compare dates
/// without a calendar crate.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "2027-03-01" → days since the epoch.
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.trim().split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

pub fn today() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86_400) as i64
}

/// A random decimal code of `digits` digits from the kernel's random source.
pub fn random_digits(digits: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = vec![0u8; digits * 2];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.chunks(2).map(|b| char::from(b'0' + ((u16::from(b[0]) << 8 | u16::from(b[1])) % 10) as u8)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert!(valid_username("asha"));
        assert!(valid_username("_svc-1"));
        assert!(!valid_username("Asha"));
        assert!(!valid_username("1asha"));
        assert!(!valid_username(""));
        assert!(!valid_username(&"a".repeat(33)));
    }

    #[test]
    fn fullnames() {
        assert!(valid_fullname("Asha Roy"));
        assert!(valid_fullname("অমিত রায়"));
        assert!(!valid_fullname("a:b"));
        assert!(!valid_fullname("a\nb"));
    }

    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse_date("2000-03-01"), Some(11017));
        assert_eq!(parse_date("2027-13-01"), None);
        assert!(parse_date("2027-03-01").unwrap() > parse_date("2026-11-16").unwrap());
    }

    #[test]
    fn digits() {
        let d = random_digits(8).unwrap();
        assert_eq!(d.len(), 8);
        assert!(d.chars().all(|c| c.is_ascii_digit()));
    }
}
