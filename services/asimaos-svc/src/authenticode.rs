//! Would this PC's firmware start a signed EFI program under Secure Boot? Used before a new shim
//! from an update is tried: one the firmware refuses would stop the start at a Secure Boot
//! violation screen.
//!
//! The firmware (UEFI 2.10, 32.5) starts a program when one of its Authenticode signatures chains
//! to a certificate in `db` (or its hash is in `db`), and neither its hash nor a signer is in
//! `dbx`. The program's integrity is already guaranteed by the update file's signature; here only
//! the signer matters. OpenSSL checks each signature (`openssl smime -verify`, which reads
//! Authenticode's PKCS#7) against the firmware's certificates, ignoring expiry dates as firmware
//! does (Microsoft's UEFI CA 2011 expired in 2026 and still counts).

use crate::Root;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The GUID of `db` and `dbx`.
pub const SECURITY_DATABASE: &str = "d719b2cb-3d3a-4596-a3bc-dad00e67656f";
/// EFI_CERT_X509_GUID and EFI_CERT_SHA256_GUID as they are stored (the first three fields little-endian).
const CERT_X509: [u8; 16] =
    [0xa1, 0x59, 0xc0, 0xa5, 0xe4, 0x94, 0xa7, 0x4a, 0x87, 0xb5, 0xab, 0x15, 0x5c, 0x2b, 0xf0, 0x72];
const CERT_SHA256: [u8; 16] =
    [0x26, 0x16, 0xc4, 0xc1, 0x4c, 0x50, 0x92, 0x40, 0xac, 0xa9, 0x41, 0xf9, 0x36, 0x93, 0x43, 0x28];

#[derive(Debug, PartialEq)]
pub enum Trust {
    Trusted,
    /// No Authenticode signature at all.
    Unsigned,
    /// Signed, but by no one this firmware trusts.
    UnknownSigner,
    /// Its hash or a signer is in `dbx`.
    Revoked,
}

/// A signature database (`db`, `dbx`): X.509 certificates (DER) and SHA-256 hashes.
#[derive(Default, Debug)]
pub struct Database {
    pub certs: Vec<Vec<u8>>,
    pub hashes: Vec<[u8; 32]>,
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

/// Parses EFI_SIGNATURE_LISTs (a variable's data without its 4 attribute bytes).
pub fn parse_database(mut d: &[u8]) -> Database {
    let mut db = Database::default();
    while d.len() >= 28 {
        let kind: [u8; 16] = d[..16].try_into().unwrap();
        let (Some(list), Some(header), Some(size)) = (u32_at(d, 16), u32_at(d, 20), u32_at(d, 24)) else { break };
        let (list, header, size) = (list as usize, header as usize, size as usize);
        if list < 28 + header || list > d.len() || size <= 16 {
            break;
        }
        let mut at = 28 + header;
        while at + size <= list {
            let data = &d[at + 16..at + size]; // after the owner GUID
            if kind == CERT_X509 {
                db.certs.push(data.to_vec());
            } else if kind == CERT_SHA256 && data.len() == 32 {
                db.hashes.push(data.try_into().unwrap());
            }
            at += size;
        }
        d = &d[list..];
    }
    db
}

/// The firmware's `db` or `dbx`.
pub fn firmware_database(root: &Root, name: &str) -> Option<Database> {
    let data = std::fs::read(root.path(&format!("/sys/firmware/efi/efivars/{name}-{SECURITY_DATABASE}"))).ok()?;
    // Four attribute bytes, then the lists; an empty database trusts nothing.
    (data.len() >= 4).then(|| parse_database(&data[4..]))
}

/// Where a PE file keeps its checksum, its certificate-table entry and the table itself.
struct Layout {
    checksum: usize,
    cert_entry: usize,
    table: Option<(usize, usize)>,
}

fn layout(pe: &[u8]) -> Option<Layout> {
    let lfanew = u32_at(pe, 0x3c)? as usize;
    if pe.get(lfanew..lfanew + 4)? != b"PE\0\0" {
        return None;
    }
    let opt = lfanew + 24;
    let (dirs, count) = match u16_at(pe, opt)? {
        0x20b => (opt + 112, u32_at(pe, opt + 108)?),
        0x10b => (opt + 96, u32_at(pe, opt + 92)?),
        _ => return None,
    };
    if count < 5 {
        return None;
    }
    let cert_entry = dirs + 4 * 8;
    let (off, size) = (u32_at(pe, cert_entry)? as usize, u32_at(pe, cert_entry + 4)? as usize);
    let table = (off > 0 && size > 0 && off + size <= pe.len()).then_some((off, size));
    Some(Layout { checksum: opt + 64, cert_entry, table })
}

/// The PKCS#7 signatures in the certificate table (WIN_CERT_TYPE_PKCS_SIGNED_DATA entries).
pub fn signatures(pe: &[u8]) -> Vec<Vec<u8>> {
    let Some((off, size)) = layout(pe).and_then(|l| l.table) else { return Vec::new() };
    let mut out = Vec::new();
    let mut at = off;
    while at + 8 <= off + size {
        let (Some(len), Some(kind)) = (u32_at(pe, at), u16_at(pe, at + 6)) else { break };
        let len = len as usize;
        if len < 8 || at + len > off + size {
            break;
        }
        if kind == 0x0002 {
            out.push(pe[at + 8..at + len].to_vec());
        }
        at += (len + 7) & !7;
    }
    out
}

/// The Authenticode SHA-256 of a PE file: everything but the checksum, the certificate-table entry
/// and the table itself (the layout every signing tool produces: the table comes last).
pub fn authenticode_sha256(pe: &[u8]) -> Option<[u8; 32]> {
    let l = layout(pe)?;
    let end = l.table.map(|(off, _)| off).unwrap_or(pe.len());
    if end < l.cert_entry + 8 {
        return None;
    }
    let mut h = Sha256::new();
    h.update(&pe[..l.checksum]);
    h.update(&pe[l.checksum + 4..l.cert_entry]);
    h.update(&pe[l.cert_entry + 8..end]);
    Some(h.finalize().into())
}

fn pem(der: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in der.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    let lines: Vec<&str> = s.as_bytes().chunks(64).map(|l| std::str::from_utf8(l).unwrap()).collect();
    format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", lines.join("\n"))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("asimaos-authenticode-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether one of the signatures verifies against one of the certificates (OpenSSL).
fn signed_by(signatures: &[Vec<u8>], certs: &[Vec<u8>], dir: &Path) -> Result<bool> {
    if certs.is_empty() {
        return Ok(false);
    }
    let anchors = dir.join("anchors.pem");
    std::fs::write(&anchors, certs.iter().map(|c| pem(c)).collect::<String>())?;
    for (i, sig) in signatures.iter().enumerate() {
        let file = dir.join(format!("sig{i}.p7"));
        std::fs::write(&file, sig)?;
        let ok = Command::new("openssl")
            .args([
                "smime",
                "-verify",
                "-inform",
                "DER",
                "-binary",
                "-purpose",
                "any",
                "-partial_chain",
                "-no_check_time",
            ])
            .arg("-in")
            .arg(&file)
            .arg("-CAfile")
            .arg(&anchors)
            .args(["-out", "/dev/null"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("openssl could not be started")?
            .success();
        if ok {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether a firmware with these `db` and `dbx` would start `pe` under Secure Boot.
pub fn trust(pe: &[u8], db: &Database, dbx: &Database) -> Result<Trust> {
    let hash = authenticode_sha256(pe).context("not an EFI program")?;
    let sigs = signatures(pe);
    let dir = Scratch::new()?;
    if dbx.hashes.contains(&hash) || signed_by(&sigs, &dbx.certs, &dir.0)? {
        return Ok(Trust::Revoked);
    }
    if db.hashes.contains(&hash) {
        return Ok(Trust::Trusted);
    }
    if sigs.is_empty() {
        return Ok(Trust::Unsigned);
    }
    Ok(if signed_by(&sigs, &db.certs, &dir.0)? { Trust::Trusted } else { Trust::UnknownSigner })
}

/// The same with this PC's own `db` and `dbx`.
pub fn firmware_trust(root: &Root, pe: &[u8]) -> Result<Trust> {
    let Some(db) = firmware_database(root, "db") else { bail!("the firmware's db can't be read") };
    let dbx = firmware_database(root, "dbx").unwrap_or_default();
    trust(pe, &db, &dbx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(kind: [u8; 16], entries: &[&[u8]]) -> Vec<u8> {
        let size = 16 + entries[0].len();
        let mut v = kind.to_vec();
        v.extend(((28 + size * entries.len()) as u32).to_le_bytes());
        v.extend(0u32.to_le_bytes());
        v.extend((size as u32).to_le_bytes());
        for e in entries {
            v.extend([0x77u8; 16]); // owner
            v.extend(*e);
        }
        v
    }

    #[test]
    fn signature_lists() {
        let mut d = list(CERT_X509, &[b"cert-one", b"cert-two"]);
        d.extend(list(CERT_SHA256, &[&[7u8; 32]]));
        let db = parse_database(&d);
        assert_eq!(db.certs, vec![b"cert-one".to_vec(), b"cert-two".to_vec()]);
        assert_eq!(db.hashes, vec![[7u8; 32]]);
        assert!(parse_database(&d[..30]).certs.is_empty());
    }

    #[test]
    fn pem_encoding() {
        assert!(pem(b"Man").contains("\nTWFu\n"));
        assert!(pem(b"Ma").contains("\nTWE=\n"));
        assert!(pem(b"M").contains("\nTQ==\n"));
    }

    /// With Fedora's Microsoft-signed shim when the build has fetched it (build/shim): signed twice,
    /// by Microsoft's UEFI CA 2011 and 2023; each CA alone lets it start, an unrelated one doesn't,
    /// and its hash in dbx revokes it.
    #[test]
    fn real_shim() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/shim/shimx64.efi");
        let Ok(pe) = std::fs::read(&path) else { return };
        let sigs = signatures(&pe);
        assert_eq!(sigs.len(), 2);
        let dir = Scratch::new().unwrap();
        // The CA certificates the signatures carry, as DER.
        let mut cas = Vec::new();
        for (i, sig) in sigs.iter().enumerate() {
            let f = dir.0.join(format!("s{i}"));
            std::fs::write(&f, sig).unwrap();
            let out = Command::new("openssl")
                .args(["pkcs7", "-inform", "DER", "-print_certs", "-in"])
                .arg(&f)
                .output()
                .unwrap();
            let text = String::from_utf8(out.stdout).unwrap();
            let ca = text
                .split("subject=")
                .find(|b| b.contains("UEFI CA 20") && !b.contains("signer") && !b.contains("Publisher"))
                .unwrap();
            let body: String = ca.lines().skip_while(|l| !l.starts_with("-----BEGIN")).collect::<Vec<_>>().join("\n");
            let der = Command::new("openssl")
                .args(["x509", "-outform", "DER"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut c| {
                    use std::io::Write;
                    c.stdin.take().unwrap().write_all(body.as_bytes())?;
                    c.wait_with_output()
                })
                .unwrap()
                .stdout;
            cas.push(der);
        }
        let none = Database::default();
        for ca in &cas {
            let db = Database { certs: vec![ca.clone()], hashes: vec![] };
            assert_eq!(trust(&pe, &db, &none).unwrap(), Trust::Trusted);
        }
        let other = Database { certs: vec![b"not a certificate".to_vec()], hashes: vec![] };
        assert_eq!(trust(&pe, &other, &none).unwrap(), Trust::UnknownSigner);
        let db = Database { certs: cas.clone(), hashes: vec![] };
        let dbx = Database { certs: vec![], hashes: vec![authenticode_sha256(&pe).unwrap()] };
        assert_eq!(trust(&pe, &db, &dbx).unwrap(), Trust::Revoked);
        // The hash matches the one Microsoft signed (in the signature's SpcIndirectDataContent).
        let hash = authenticode_sha256(&pe).unwrap();
        assert!(sigs[0].windows(32).any(|w| w == hash), "the Authenticode hash differs from the signed one");
    }
}
