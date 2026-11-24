//! Own Secure Boot keys (Asima Maintenance → Security): instead of approving the Asima OS key
//! on shim's blue screen (MOK, D16), this computer gets keys of its own in the firmware, the way
//! `sbctl enroll-keys --microsoft` does, and Microsoft's and the computer maker's certificates stay.
//!
//! 1. Prepare: the firmware's db, KEK and dbx as they are now are saved, and this computer's own
//!    Platform Key and KEK are made (RSA-2048, kept root-only in /var/lib/asimaos/secureboot; they
//!    never leave this computer).
//! 2. The person puts the firmware into Setup Mode ("Reset to Setup Mode" / "Clear Secure Boot
//!    keys" in its settings).
//! 3. Enroll: dbx, db (the saved certificates the person keeps, plus the Asima OS key), KEK
//!    (the saved ones plus this computer's) and last the Platform Key, which ends Setup Mode. Each
//!    is an authenticated variable (UEFI 2.10, 8.2.2: EFI_VARIABLE_AUTHENTICATION_2) signed with
//!    OpenSSL: db and dbx by this computer's KEK, KEK and PK by its Platform Key.
//!
//! Old certificates the person unticks (an earlier build's, another system's own keys) are left
//! out, which is how every old key is wiped from the firmware.

use crate::security::ASIMAOS_KEY;
use anyhow::{ensure, Context, Result};
use asimaos_svc::{authenticode, cmd, efi, Root};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const DIR: &str = "/var/lib/asimaos/secureboot";
const DB_GUID: &str = authenticode::SECURITY_DATABASE;
/// The owner of the entries Asima OS adds.
const OWNER: &str = "a7d3c1e0-5b2f-4e8a-9c61-2d4f6b8e0a13";
const PKCS7_GUID: &str = "4aafd29d-68df-49ee-8aa9-347d375665a7";
const X509_GUID: &str = "a5c059a1-94e4-4aa7-87b5-ab155c2bf072";
/// Non-volatile, boot and run-time access, time-based authenticated writes.
const ATTRIBUTES: u32 = 0x27;

fn dir(root: &Root) -> PathBuf {
    root.path(DIR)
}

/// An EFI_SIGNATURE_LIST holding one X.509 certificate.
pub fn x509_list(der: &[u8]) -> Vec<u8> {
    let size = 16 + der.len();
    let mut v = efi::guid_bytes(X509_GUID).unwrap().to_vec();
    v.extend(((28 + size) as u32).to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v.extend((size as u32).to_le_bytes());
    v.extend(efi::guid_bytes(OWNER).unwrap());
    v.extend(der);
    v
}

/// The signature lists of `esl` without the X.509 entries whose SHA-256 is in `drop` (each kept
/// certificate becomes a list of its own; hash lists stay as they are).
pub fn without(esl: &[u8], drop: &[String]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut out = Vec::new();
    let mut d = esl;
    while d.len() >= 28 {
        let list = u32::from_le_bytes(d[16..20].try_into().unwrap()) as usize;
        let header = u32::from_le_bytes(d[20..24].try_into().unwrap()) as usize;
        let size = u32::from_le_bytes(d[24..28].try_into().unwrap()) as usize;
        if list < 28 + header || list > d.len() || size <= 16 {
            break;
        }
        if d[..16] == efi::guid_bytes(X509_GUID).unwrap() {
            let mut at = 28 + header;
            while at + size <= list {
                let cert = &d[at + 16..at + size];
                let id = hex(&Sha256::digest(cert));
                if !drop.contains(&id) {
                    // The original owner GUID stays.
                    let mut one = d[..16].to_vec();
                    one.extend(((28 + size) as u32).to_le_bytes());
                    one.extend(0u32.to_le_bytes());
                    one.extend((size as u32).to_le_bytes());
                    one.extend(&d[at..at + size]);
                    out.extend(one);
                }
                at += size;
            }
        } else {
            out.extend(&d[..list]);
        }
        d = &d[list..];
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// EFI_TIME for now (UTC), as the authenticated write's time stamp.
fn efi_time(secs: i64) -> [u8; 16] {
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468; // H. Hinnant's days → civil date
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    let mut t = [0u8; 16];
    t[0..2].copy_from_slice(&year.to_le_bytes());
    t[2] = month;
    t[3] = day;
    t[4] = (rest / 3600) as u8;
    t[5] = ((rest % 3600) / 60) as u8;
    t[6] = (rest % 60) as u8;
    t
}

/// What the firmware hashes for an authenticated write: the variable's name (UTF-16, no
/// terminator), its GUID, the attributes, the time stamp and the data.
fn to_sign(name: &str, guid: &str, time: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let mut v: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    v.extend(efi::guid_bytes(guid).unwrap());
    v.extend(ATTRIBUTES.to_le_bytes());
    v.extend(time);
    v.extend(data);
    v
}

/// The bytes written to efivarfs: the attributes, EFI_VARIABLE_AUTHENTICATION_2 (the time stamp,
/// then WIN_CERTIFICATE_UEFI_GUID with the PKCS#7 signature), then the data.
fn authenticated(time: &[u8; 16], signature: &[u8], data: &[u8]) -> Vec<u8> {
    let mut v = ATTRIBUTES.to_le_bytes().to_vec();
    v.extend(time);
    v.extend(((8 + 16 + signature.len()) as u32).to_le_bytes());
    v.extend(0x0200u16.to_le_bytes()); // WIN_CERT_REVISION_2_0
    v.extend(0x0EF1u16.to_le_bytes()); // WIN_CERT_TYPE_EFI_GUID
    v.extend(efi::guid_bytes(PKCS7_GUID).unwrap());
    v.extend(signature);
    v.extend(data);
    v
}

/// Signs and writes one Secure Boot variable with `key` (a name in DIR: "PK" or "KEK").
async fn write_signed(root: &Root, name: &str, guid: &str, data: &[u8], key: &str) -> Result<()> {
    let d = dir(root);
    let time = efi_time(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64);
    let input = d.join(format!("{name}.to-sign"));
    let sig = d.join(format!("{name}.p7"));
    std::fs::write(&input, to_sign(name, guid, &time, data))?;
    let (i, s) = (input.to_string_lossy().into_owned(), sig.to_string_lossy().into_owned());
    let (crt, k) = (d.join(format!("{key}.crt")), d.join(format!("{key}.key")));
    let (crt, k) = (crt.to_string_lossy().into_owned(), k.to_string_lossy().into_owned());
    let signed = cmd::run(
        "openssl",
        &[
            "cms", "-sign", "-binary", "-noattr", "-md", "sha256", "-outform", "DER", "-signer", &crt, "-inkey", &k,
            "-in", &i, "-out", &s,
        ],
    )
    .await;
    let _ = std::fs::remove_file(&input);
    signed.with_context(|| format!("{name} could not be signed"))?;
    let signature = std::fs::read(&sig)?;
    let _ = std::fs::remove_file(&sig);
    efi::write_raw(root, name, guid, &authenticated(&time, &signature, data))
        .with_context(|| format!("The firmware refused the new {name}"))
}

fn setup_mode(root: &Root) -> bool {
    efi::read(root, "SetupMode", efi::GLOBAL).is_some_and(|v| v.first() == Some(&1))
}

async fn der_of(crt: &Path) -> Result<Vec<u8>> {
    let c = crt.to_string_lossy().into_owned();
    let out = crt.with_extension("der");
    let o = out.to_string_lossy().into_owned();
    cmd::run("openssl", &["x509", "-in", &c, "-outform", "DER", "-out", &o]).await?;
    let der = std::fs::read(&out)?;
    let _ = std::fs::remove_file(&out);
    Ok(der)
}

/// This computer's own Platform Key and KEK, kept in DIR (made once).
async fn ensure_keys(root: &Root) -> Result<()> {
    let d = dir(root);
    std::fs::create_dir_all(&d)?;
    let host = std::fs::read_to_string(root.path("/etc/hostname")).unwrap_or_default().trim().to_string();
    for (name, what) in [("PK", "Platform Key"), ("KEK", "Key Exchange Key")] {
        if d.join(format!("{name}.key")).is_file() && d.join(format!("{name}.crt")).is_file() {
            continue;
        }
        let subj = format!("/CN={} {what} (Asima OS)/", if host.is_empty() { "This computer" } else { &host });
        let (k, c) = (d.join(format!("{name}.key")), d.join(format!("{name}.crt")));
        let (k, c) = (k.to_string_lossy().into_owned(), c.to_string_lossy().into_owned());
        cmd::run(
            "openssl",
            &[
                "req", "-new", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256", "-days", "7300", "-subj", &subj,
                "-keyout", &k, "-out", &c,
            ],
        )
        .await
        .with_context(|| format!("The {what} could not be made"))?;
    }
    Ok(())
}

/// Step 1: saves the firmware's databases as they are and makes this computer's keys.
pub async fn prepare(root: &Root) -> Result<()> {
    ensure!(efi::is_efi(root), "This computer didn't start in UEFI mode");
    ensure!(
        !setup_mode(root),
        "The firmware is in Setup Mode already, so its certificates are gone: set Secure Boot back to its factory keys first"
    );
    let db = efi::read(root, "db", DB_GUID).context("The firmware's certificate list (db) can't be read")?;
    ensure!(!db.is_empty(), "The firmware's certificate list (db) is empty");
    let d = dir(root);
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("saved-db.esl"), &db)?;
    std::fs::write(d.join("saved-KEK.esl"), efi::read(root, "KEK", efi::GLOBAL).unwrap_or_default())?;
    std::fs::write(d.join("saved-dbx.esl"), efi::read(root, "dbx", DB_GUID).unwrap_or_default())?;
    ensure_keys(root).await
}

/// Step 3, in Setup Mode: dbx, db, KEK, then the Platform Key. `drop`: SHA-256 fingerprints of
/// saved certificates to leave out.
pub async fn enroll(root: &Root, drop: &[String]) -> Result<()> {
    let d = dir(root);
    ensure!(d.join("saved-db.esl").is_file(), "Prepare first: the firmware's certificates haven't been saved");
    ensure!(setup_mode(root), "The firmware isn't in Setup Mode: choose “Reset to Setup Mode” in its settings first");
    ensure_keys(root).await?;
    let asimaos = std::fs::read(root.path(ASIMAOS_KEY)).context("The Asima OS key is missing from this system")?;
    let saved_db = std::fs::read(d.join("saved-db.esl"))?;
    let mut db = without(&saved_db, drop);
    if !authenticode::parse_database(&db).certs.contains(&asimaos) {
        db.extend(x509_list(&asimaos));
    }
    ensure!(
        authenticode::parse_database(&db).certs.len() > 1,
        "Keep Microsoft's certificates: without them Windows and some graphics cards wouldn't start"
    );
    let mut kek = without(&std::fs::read(d.join("saved-KEK.esl")).unwrap_or_default(), drop);
    kek.extend(x509_list(&der_of(&d.join("KEK.crt")).await?));
    let pk = x509_list(&der_of(&d.join("PK.crt")).await?);
    let dbx = std::fs::read(d.join("saved-dbx.esl")).unwrap_or_default();
    // A cleared firmware may have lost its revocation list too: it comes back first.
    if !dbx.is_empty() && efi::read(root, "dbx", DB_GUID).unwrap_or_default().is_empty() {
        write_signed(root, "dbx", DB_GUID, &dbx, "KEK").await?;
    }
    write_signed(root, "db", DB_GUID, &db, "KEK").await?;
    write_signed(root, "KEK", efi::GLOBAL, &kek, "PK").await?;
    write_signed(root, "PK", efi::GLOBAL, &pk, "PK").await?;
    Ok(())
}

/// For the Security page: Setup Mode, whether the firmware has this computer's own keys, and once
/// prepared the saved certificates (name, fingerprint) to choose from.
pub async fn status(root: &Root) -> Value {
    let d = dir(root);
    let prepared = d.join("saved-db.esl").is_file();
    let pk_now = efi::read(root, "PK", efi::GLOBAL).unwrap_or_default();
    let own = match der_of(&d.join("PK.crt")).await {
        Ok(der) if !pk_now.is_empty() => pk_now.windows(der.len()).any(|w| w == der.as_slice()),
        _ => false,
    };
    let mut certificates = Vec::new();
    if prepared && !own {
        use sha2::{Digest, Sha256};
        for (list, file) in [("db", "saved-db.esl"), ("KEK", "saved-KEK.esl")] {
            let esl = std::fs::read(d.join(file)).unwrap_or_default();
            for cert in authenticode::parse_database(&esl).certs {
                certificates
                    .push(json!({ "id": hex(&Sha256::digest(&cert)), "list": list, "name": subject(&d, &cert).await }));
            }
        }
    }
    json!({ "setupMode": setup_mode(root), "prepared": prepared, "own": own, "certificates": certificates })
}

async fn subject(d: &Path, der: &[u8]) -> String {
    let f = d.join("subject.der");
    if std::fs::write(&f, der).is_err() {
        return String::new();
    }
    let p = f.to_string_lossy().into_owned();
    let out = cmd::probe(
        "openssl",
        &["x509", "-inform", "DER", "-in", &p, "-noout", "-subject", "-nameopt", "utf8,sep_comma_plus"],
    )
    .await;
    let _ = std::fs::remove_file(&f);
    let s = out.unwrap_or_default();
    let s = s.trim().trim_start_matches("subject=");
    s.split(',').find_map(|p| p.trim().strip_prefix("CN=").map(String::from)).unwrap_or_else(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_lists() {
        let a = x509_list(b"cert-a");
        let mut esl = a.clone();
        esl.extend(x509_list(b"cert-b"));
        let parsed = authenticode::parse_database(&esl);
        assert_eq!(parsed.certs, vec![b"cert-a".to_vec(), b"cert-b".to_vec()]);
        use sha2::{Digest, Sha256};
        let drop_b = vec![hex(&Sha256::digest(b"cert-b"))];
        assert_eq!(authenticode::parse_database(&without(&esl, &drop_b)).certs, vec![b"cert-a".to_vec()]);
        assert_eq!(without(&esl, &[]), esl);
    }

    #[test]
    fn authenticated_write_layout() {
        // 2026-11-22 10:30:15 UTC
        let t = efi_time(1_795_343_415);
        assert_eq!(u16::from_le_bytes([t[0], t[1]]), 2026);
        assert_eq!((t[2], t[3], t[4], t[5], t[6]), (11, 22, 10, 30, 15));
        let data = b"lists";
        let all = authenticated(&t, b"SIG", data);
        assert_eq!(u32::from_le_bytes(all[0..4].try_into().unwrap()), 0x27);
        assert_eq!(&all[4..20], &t);
        assert_eq!(u32::from_le_bytes(all[20..24].try_into().unwrap()), 8 + 16 + 3);
        assert_eq!(&all[24..28], &[0x00, 0x02, 0xF1, 0x0E]);
        assert!(all.ends_with(b"SIGlists"));
        let s = to_sign("db", DB_GUID, &t, data);
        assert_eq!(&s[..4], &[b'd', 0, b'b', 0]);
        assert!(s.ends_with(data));
    }
}
