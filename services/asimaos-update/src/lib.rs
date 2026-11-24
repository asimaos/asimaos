//! The Asima OS update file: `.aosupd`, one file with everything a new version needs.
//!
//! ```text
//! offset 0   "AOSUPD\0\0"          magic
//!        8   u16 format = 1       little endian
//!       10   u16 reserved = 0
//!       12   u32 manifest length M (≤ 1 MiB)
//!       16   u32 signature length S (= 64)
//!       20   u32 reserved = 0
//!       24   manifest (M bytes of UTF-8 text)
//!   24 + M   Ed25519 signature over bytes [0, 24 + M): the header and the manifest
//!            … zero padding to 4 KiB, then every part at the offset the manifest names
//! ```
//!
//! The manifest is KEY=VALUE lines and one `FILE` line per part:
//!
//! ```text
//! FORMAT=1
//! PRODUCT=Asima OS
//! VERSION=2026.12.01.1
//! ARCH=x86_64
//! CHANNEL=stable
//! EXPIRES=2027-03-01              the update can't be installed after this day (no replays)
//! MIN_FROM=2026.11.01.1           optional: the oldest version it may be installed on
//! UNPACKED_SIZE=5368709120
//! FILE <name> <kind> <offset> <stored size> <size> <sha256 of content> <sha256 as stored> <zstd|none>
//! ```
//!
//! Kinds: `slot` (/usr image and its verity data), `uki` (the boot entry), `bootloader`, `shim`
//! (`shimx64.efi` and its `mmx64.efi`, the Microsoft-signed first step of Secure Boot) and
//! `rescue` (all replaced only after the new version started fine), `migration` (run at the new
//! version's first start), `notes` (release notes, shown before installing).
//!
//! **Any other kind** (a lowercase name of letters, digits and `-`) is a part this updater doesn't
//! know yet. It is checked like every other part, then kept in `staged/<version>/parts/<kind>/`
//! for the new version's own code to use. So a new kind of part never needs a stepping-stone update.
//!
//! **Fixed for good in format 1**, because the *installed* updater reads every later file: this
//! header, the manifest's syntax, the Ed25519 signature over header and manifest, the required
//! parts (the `/usr` image, its verity data and the boot entry, named after the version), zstd with
//! at most a 128 MiB window, and where parts go (the slots, `EFI/Asima/asimaos_<version>.efi`
//! made "current" with three tries, `staged/<version>/`). Changing any of these needs a new
//! FORMAT, which older updaters refuse ("install the update before it first").
//!
//! Safety: nothing is trusted before the signature checks out; the manifest is parsed strictly;
//! the file's length must match exactly (a cut-off download is caught at once); every stored part
//! can be hashed before anything is written; content is hashed again while it is written.

use anyhow::{bail, ensure, Context, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub mod boot;

pub const MAGIC: &[u8; 8] = b"AOSUPD\0\0";
pub const FORMAT: u16 = 1;
pub const EXTENSION: &str = "aosupd";
const HEADER: usize = 24;
const MAX_MANIFEST: usize = 1 << 20;
const SIG_LEN: usize = 64;
const ALIGN: u64 = 4096;
const WINDOW_LOG: u32 = 27; // zstd long-distance window: 128 MiB, also the decoder's limit

/// Why a file is refused, so Asima Updater and Asima Rescue can say what to do about it.
/// Errors that carry none of these are other reasons a signed update can't be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Not an Asima OS update file, or not signed with the Asima OS release key.
    NotSigned,
    /// Made in a later update-file format: an update in between is needed first.
    NewerFormat,
    /// Cut off, or changed on its way: download it again.
    Damaged,
}

impl Refused {
    pub fn code(self) -> &'static str {
        match self {
            Refused::NotSigned => "not-signed",
            Refused::NewerFormat => "newer-format",
            Refused::Damaged => "damaged",
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for Refused {}

/// The reason behind an error from `Update::open` or `Update::check_parts`, if it is one of them.
pub fn refused(e: &anyhow::Error) -> Option<Refused> {
    e.downcast_ref::<Refused>().copied()
}

/// `refuse!(kind, "message", args…)`: return an error with this message, marked with `kind`.
macro_rules! refuse {
    ($kind:expr, $($msg:tt)+) => {
        return Err(anyhow::Error::new($kind).context(format!($($msg)+)))
    };
}

macro_rules! refuse_unless {
    ($cond:expr, $kind:expr, $($msg:tt)+) => {
        if !$cond {
            refuse!($kind, $($msg)+);
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Slot,
    Uki,
    Bootloader,
    Shim,
    Rescue,
    Migration,
    Notes,
    /// A kind this updater doesn't know: kept for the new version (see the top of this file).
    Extra,
}

/// A kind of part: a lowercase name of letters, digits and `-`, at most 32 characters. It is also
/// a folder name in `staged/<version>/parts/`, so nothing else is allowed.
fn valid_kind(k: &str) -> bool {
    k.len() <= 32
        && k.starts_with(|c: char| c.is_ascii_lowercase())
        && k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Kind {
    pub fn parse(s: &str) -> Result<Kind> {
        Ok(match s {
            "slot" => Kind::Slot,
            "uki" => Kind::Uki,
            "bootloader" => Kind::Bootloader,
            "shim" => Kind::Shim,
            "rescue" => Kind::Rescue,
            "migration" => Kind::Migration,
            "notes" => Kind::Notes,
            other if valid_kind(other) => Kind::Extra,
            other => bail!("malformed part kind {other}"),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Slot => "slot",
            Kind::Uki => "uki",
            Kind::Bootloader => "bootloader",
            Kind::Shim => "shim",
            Kind::Rescue => "rescue",
            Kind::Migration => "migration",
            Kind::Notes => "notes",
            Kind::Extra => "extra",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    /// The kind as the manifest names it (for [`Kind::Extra`], the kind this updater doesn't know).
    pub kind_name: String,
    pub offset: u64,
    pub stored: u64,
    pub size: u64,
    pub sha256: String,
    pub stored_sha256: String,
    pub zstd: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    pub product: String,
    pub version: String,
    pub arch: String,
    pub channel: String,
    pub expires: Option<String>,
    pub min_from: Option<String>,
    pub created: String,
    pub unpacked: u64,
    pub files: Vec<Entry>,
}

impl Manifest {
    pub fn files_of(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.files.iter().filter(move |e| e.kind == kind)
    }
}

fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 128
        && !n.starts_with('.')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

fn valid_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=6).contains(&parts.len())
        && parts.iter().all(|p| !p.is_empty() && p.len() <= 9 && p.chars().all(|c| c.is_ascii_digit()))
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Strict parsing: every part named in the manifest must be well formed; the required parts must
/// be there exactly once and be named after the version.
pub fn parse_manifest(text: &str) -> Result<Manifest> {
    let mut m = Manifest::default();
    let mut format = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("FILE ") {
            let f: Vec<&str> = rest.split(' ').collect();
            ensure!(f.len() == 8, "malformed FILE line");
            let e = Entry {
                name: f[0].to_string(),
                kind: Kind::parse(f[1])?,
                kind_name: f[1].to_string(),
                offset: f[2].parse().context("bad offset")?,
                stored: f[3].parse().context("bad size")?,
                size: f[4].parse().context("bad size")?,
                sha256: f[5].to_string(),
                stored_sha256: f[6].to_string(),
                zstd: match f[7] {
                    "zstd" => true,
                    "none" => false,
                    other => bail!("unknown compression {other}"),
                },
            };
            ensure!(valid_name(&e.name), "a part has an unsafe name");
            ensure!(valid_sha(&e.sha256) && valid_sha(&e.stored_sha256), "malformed checksum");
            ensure!(e.zstd || e.stored == e.size, "uncompressed part with two sizes");
            m.files.push(e);
            continue;
        }
        let (k, v) = line.split_once('=').context("malformed manifest line")?;
        match k {
            "FORMAT" => format = v.parse::<u16>().ok(),
            "PRODUCT" => m.product = v.to_string(),
            "VERSION" => m.version = v.to_string(),
            "ARCH" => m.arch = v.to_string(),
            "CHANNEL" => m.channel = v.to_string(),
            "EXPIRES" => m.expires = Some(v.to_string()),
            "MIN_FROM" => m.min_from = Some(v.to_string()),
            "CREATED" => m.created = v.to_string(),
            "UNPACKED_SIZE" => m.unpacked = v.parse().context("bad UNPACKED_SIZE")?,
            _ => {} // newer optional keys are ignored; FORMAT guards real changes
        }
    }
    ensure!(format == Some(FORMAT), "this update needs a newer Asima Updater");
    ensure!(m.product == "Asima OS", "this is not an Asima OS update");
    ensure!(valid_version(&m.version), "the update's version is not valid");
    if let Some(v) = &m.min_from {
        ensure!(valid_version(v), "the update's MIN_FROM is not valid");
    }
    if let Some(e) = &m.expires {
        ensure!(parse_date(e).is_some(), "the update's expiry date is not valid");
    }
    let v = m.version.clone();
    let names: Vec<&str> = m.files.iter().map(|e| e.name.as_str()).collect();
    for (i, n) in names.iter().enumerate() {
        ensure!(!names[..i].contains(n), "a part is listed twice");
    }
    let slots: Vec<&str> = m.files_of(Kind::Slot).map(|e| e.name.as_str()).collect();
    ensure!(
        slots.len() == 2
            && slots.contains(&format!("usr_{v}.erofs").as_str())
            && slots.contains(&format!("verity_{v}.img").as_str()),
        "the update must carry the /usr image and its verity data for {v}"
    );
    let ukis: Vec<&str> = m.files_of(Kind::Uki).map(|e| e.name.as_str()).collect();
    ensure!(ukis == [format!("asimaos_{v}.efi").as_str()], "the update must carry one boot entry for {v}");
    ensure!(
        m.files_of(Kind::Bootloader).count() <= 1
            && m.files_of(Kind::Rescue).count() <= 1
            && m.files_of(Kind::Notes).count() <= 1,
        "too many parts of one kind"
    );
    let shims: Vec<&str> = m.files_of(Kind::Shim).map(|e| e.name.as_str()).collect();
    ensure!(
        shims.is_empty() || (shims.len() == 2 && shims.contains(&"shimx64.efi") && shims.contains(&"mmx64.efi")),
        "a shim comes as shimx64.efi together with its mmx64.efi"
    );
    Ok(m)
}

/// Days since 1970-01-01 for "YYYY-MM-DD".
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let (y, m, d): (i64, i64, i64) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    Some(era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468)
}

// ───────────────────────────────────────────────────────────────────────── keys

/// Key files: a comment line, then the 32-byte key in hex.
fn read_key(path: &Path) -> Result<[u8; 32]> {
    let text = fs::read_to_string(path).with_context(|| format!("can't read the key {}", path.display()))?;
    let hex_line =
        text.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.starts_with('#')).context("empty key file")?;
    let bytes = hex::decode(hex_line).context("the key file is damaged")?;
    bytes.try_into().map_err(|_| anyhow::anyhow!("the key file is damaged"))
}

pub fn load_public_key(path: &Path) -> Result<VerifyingKey> {
    VerifyingKey::from_bytes(&read_key(path)?).context("the release key is not valid")
}

/// The release keys a system trusts: the key file itself and, beside a `release.pub`, a
/// `release-next.pub` that is shipped ahead of a planned key change (or to retire a leaked key
/// without reinstalling). A directory means every `*.pub` in it.
pub fn load_trusted_keys(path: &Path) -> Result<Vec<VerifyingKey>> {
    let mut files = Vec::new();
    if path.is_dir() {
        let mut found: Vec<PathBuf> = fs::read_dir(path)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pub"))
            .collect();
        found.sort();
        files.extend(found);
    } else {
        files.push(path.to_path_buf());
        let next = path.with_file_name("release-next.pub");
        if path.file_name().is_some_and(|n| n == "release.pub") && next.is_file() {
            files.push(next);
        }
    }
    ensure!(!files.is_empty(), "no release key in {}", path.display());
    files.iter().map(|f| load_public_key(f)).collect()
}

/// Signs `path` with an Ed25519 detached signature, written to `path.sig` (64 raw bytes, as
/// `openssl pkeyutl -verify -rawin` reads it): the checksums of an ISO.
pub fn sign_file(key: &SigningKey, path: &Path) -> Result<PathBuf> {
    let data = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    let sig_path = PathBuf::from(format!("{}.sig", path.display()));
    fs::write(&sig_path, key.sign(&data).to_bytes())?;
    Ok(sig_path)
}

/// Checks `path` against `path.sig` with any of `keys`.
pub fn verify_file(keys: &[VerifyingKey], path: &Path) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    let sig = fs::read(format!("{}.sig", path.display())).context("the signature file (.sig) is missing")?;
    let sig: [u8; 64] = sig.try_into().map_err(|_| anyhow::anyhow!("the signature file is damaged"))?;
    let sig = Signature::from_bytes(&sig);
    ensure!(
        keys.iter().any(|k| k.verify(&data, &sig).is_ok()),
        "the signature doesn't match: this file isn't from Asima OS"
    );
    Ok(())
}

/// The key as PEM (SubjectPublicKeyInfo), for checking signatures with OpenSSL elsewhere.
pub fn public_key_pem(key: &VerifyingKey) -> String {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut der = vec![0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
    der.extend_from_slice(key.as_bytes());
    let mut b64 = String::new();
    for chunk in der.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            b64.push(if i <= chunk.len() { B64[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    format!("-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----\n")
}

/// The key's fingerprint, to publish: SHA-256 of the key, in groups of four hex digits.
pub fn fingerprint(key: &VerifyingKey) -> String {
    let hex = hex::encode(Sha256::digest(key.as_bytes()));
    hex.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(" ")
}

pub fn load_secret_key(path: &Path) -> Result<SigningKey> {
    Ok(SigningKey::from_bytes(&read_key(path)?))
}

/// Writes PREFIX.key (secret, 0600) and PREFIX.pub. Real release keys live on the signing vault.
pub fn keygen(prefix: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut seed = [0u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let key = SigningKey::from_bytes(&seed);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(prefix.with_extension("key"))?;
    writeln!(f, "# Asima OS update signing key (secret)\n{}", hex::encode(key.to_bytes()))?;
    fs::write(
        prefix.with_extension("pub"),
        format!("# Asima OS update key (public)\n{}\n", hex::encode(key.verifying_key().to_bytes())),
    )?;
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────── reading

pub struct Update {
    pub manifest: Manifest,
    file: File,
    data_start: u64,
}

impl Update {
    /// Opens `file` and checks everything that can be checked without reading the parts.
    pub fn open(file: File, key: &VerifyingKey) -> Result<Update> {
        Self::open_any(file, std::slice::from_ref(key))
    }

    /// The same, accepting a signature by any of the trusted `keys`.
    pub fn open_any(mut file: File, keys: &[VerifyingKey]) -> Result<Update> {
        let len = file.metadata()?.len();
        let mut head = [0u8; HEADER];
        file.seek(SeekFrom::Start(0)).context("the update must be a regular file")?;
        if file.read_exact(&mut head).is_err() || &head[..8] != MAGIC {
            refuse!(Refused::NotSigned, "this is not an Asima OS update file");
        }
        let format = u16::from_le_bytes([head[8], head[9]]);
        refuse_unless!(
            format <= FORMAT,
            Refused::NewerFormat,
            "this update is in a newer format (version {format}): install the update before it first"
        );
        refuse_unless!(format == FORMAT, Refused::Damaged, "the update file is damaged: download it again");
        let mlen = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
        let slen = u32::from_le_bytes(head[16..20].try_into().unwrap()) as usize;
        refuse_unless!(
            mlen > 0 && mlen <= MAX_MANIFEST && slen == SIG_LEN,
            Refused::Damaged,
            "the update file is damaged: download it again"
        );
        refuse_unless!(
            (HEADER + mlen + slen) as u64 <= len,
            Refused::Damaged,
            "the update file is incomplete: download it again"
        );
        let mut signed = head.to_vec();
        signed.resize(HEADER + mlen, 0);
        let mut sig = [0u8; SIG_LEN];
        if file.read_exact(&mut signed[HEADER..]).is_err() || file.read_exact(&mut sig).is_err() {
            refuse!(Refused::Damaged, "the update file can't be read completely: download it again");
        }
        let sig = Signature::from_bytes(&sig);
        if !keys.iter().any(|k| k.verify(&signed, &sig).is_ok()) {
            refuse!(Refused::NotSigned, "this file isn't signed by Asima OS");
        }
        // Only signed bytes are interpreted from here on.
        let text = std::str::from_utf8(&signed[HEADER..]).context("the update's manifest is damaged")?;
        let manifest = parse_manifest(text)?;
        let data_start = (HEADER + mlen + slen) as u64;
        let mut parts: Vec<&Entry> = manifest.files.iter().collect();
        parts.sort_by_key(|e| e.offset);
        let mut end = data_start;
        for e in &parts {
            ensure!(e.offset >= end, "the update's parts overlap");
            end = e.offset.checked_add(e.stored).context("the update file is damaged")?;
        }
        refuse_unless!(
            end == len,
            Refused::Damaged,
            "{}",
            if len < end {
                "the update file is incomplete: download it again"
            } else {
                "the update file has extra data at its end: download it again"
            }
        );
        Ok(Update { manifest, file, data_start })
    }

    fn stored_reader(&mut self, e: &Entry) -> Result<impl Read + '_> {
        self.file.seek(SeekFrom::Start(e.offset))?;
        Ok((&mut self.file).take(e.stored))
    }

    /// Hashes every part as stored, and checks that the padding between parts is zero: then every
    /// byte of the file is signed, checksummed or known. `progress(done, total)`.
    pub fn check_parts(&mut self, progress: &mut dyn FnMut(u64, u64)) -> Result<()> {
        let total: u64 = self.manifest.files.iter().map(|e| e.stored).sum();
        let mut done = 0u64;
        let mut parts = self.manifest.files.clone();
        parts.sort_by_key(|e| e.offset);
        let mut pos = self.data_start;
        for e in &parts {
            let gap = e.offset - pos;
            ensure!(gap < ALIGN, "the update file is damaged");
            let mut pad = vec![0u8; gap as usize];
            self.file.seek(SeekFrom::Start(pos))?;
            self.file.read_exact(&mut pad)?;
            refuse_unless!(
                pad.iter().all(|b| *b == 0),
                Refused::Damaged,
                "the update file is damaged in this download: download it again"
            );
            pos = e.offset + e.stored;
        }
        for e in self.manifest.files.clone() {
            let mut r = self.stored_reader(&e)?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = match r.read(&mut buf) {
                    Ok(n) => n,
                    Err(err) => {
                        refuse!(Refused::Damaged, "the update file can't be read ({err}): copy or download it again")
                    }
                };
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                done += n as u64;
                progress(done, total);
            }
            refuse_unless!(
                hex::encode(h.finalize()) == e.stored_sha256,
                Refused::Damaged,
                "part {} is damaged in this download: download the update again",
                e.name
            );
        }
        Ok(())
    }

    /// Writes the content of `e` to `out`, decompressing and verifying its checksum.
    pub fn extract(&mut self, e: &Entry, out: &mut dyn Write, progress: &mut dyn FnMut(u64)) -> Result<()> {
        let stored = self.stored_reader(e)?;
        let mut reader: Box<dyn Read> = if e.zstd {
            let mut d = zstd::stream::read::Decoder::new(stored)?;
            d.window_log_max(WINDOW_LOG)?;
            Box::new(d)
        } else {
            Box::new(BufReader::new(stored))
        };
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        let mut written = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            written += n as u64;
            refuse_unless!(
                written <= e.size,
                Refused::Damaged,
                "part {} is larger than signed: download the update again",
                e.name
            );
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
            progress(n as u64);
        }
        refuse_unless!(
            written == e.size && hex::encode(h.finalize()) == e.sha256,
            Refused::Damaged,
            "part {} is damaged: nothing was changed, download the update again",
            e.name
        );
        Ok(())
    }

    /// Small text parts (release notes), capped.
    pub fn read_text(&mut self, e: &Entry, max: u64) -> Result<String> {
        ensure!(e.size <= max, "part {} is too large", e.name);
        let mut out = Vec::with_capacity(e.size as usize);
        self.extract(e, &mut out, &mut |_| {})?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    pub fn notes(&mut self) -> Option<String> {
        let e = self.manifest.files_of(Kind::Notes).next()?.clone();
        self.read_text(&e, 64 * 1024).ok()
    }
}

// ───────────────────────────────────────────────────────────────────────── installing

/// Where an installation writes.
pub struct Targets {
    pub slots: PathBuf,   // the @slots subvolume
    pub uki_dir: PathBuf, // ESP: EFI/Asima (boot images; the start menu entries are in boot.rs)
    pub staged: PathBuf,  // @slots/staged: parts applied after the new version started fine
}

/// How [`install`] treats parts that are already there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Keep a part that is already in place when its size and signed checksum match: an update
    /// that was interrupted (power cut, crash) continues where it stopped (D21).
    Resume,
    /// Write every part again: "Reinstall this version" repairs damaged files (D21).
    Rewrite,
}

pub struct Installed {
    pub version: String,
    pub files: Vec<PathBuf>,
    /// Parts that were already in place and kept (Mode::Resume).
    pub kept: Vec<String>,
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// True when `target` already holds exactly the part's signed content.
fn already_there(e: &Entry, target: &Path) -> bool {
    fs::metadata(target).is_ok_and(|m| m.is_file() && m.len() == e.size)
        && sha256_file(target).is_ok_and(|h| h == e.sha256)
}

fn write_part(
    u: &mut Update,
    e: &Entry,
    dir: &Path,
    final_name: &str,
    progress: &mut dyn FnMut(u64),
) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let partial = dir.join(format!(".{final_name}.partial"));
    let target = dir.join(final_name);
    let _ = fs::remove_file(&partial); // left over from an interrupted attempt
    let mut out = File::create(&partial).with_context(|| format!("can't write {}", partial.display()))?;
    let r = u.extract(e, &mut out, progress).and_then(|_| Ok(out.sync_all()?));
    drop(out);
    if let Err(err) = r {
        let _ = fs::remove_file(&partial);
        return Err(err);
    }
    fs::rename(&partial, &target)?;
    File::open(dir)?.sync_all()?;
    Ok(target)
}

/// Writes the new version: the /usr image and verity data into the slots, the parts applied later
/// into `staged/<version>`, and the boot image last (`EFI/Asima/asimaos_<version>.efi`; the caller
/// then puts it in the start menu with [`boot::promote`]). Each part goes to a `.partial` file and
/// is renamed only when its checksum matches, so a power cut never leaves a half-written part
/// under its real name; finished parts stay, and the next attempt continues (Mode::Resume).
pub fn install(u: &mut Update, t: &Targets, mode: Mode, progress: &mut dyn FnMut(f64, String)) -> Result<Installed> {
    let v = u.manifest.version.clone();
    let total: u64 = u.manifest.files.iter().filter(|e| e.kind != Kind::Notes).map(|e| e.size).sum::<u64>().max(1);
    let mut done = 0u64;
    let mut written: Vec<PathBuf> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut order: Vec<Entry> = u.manifest.files.iter().filter(|e| e.kind != Kind::Notes).cloned().collect();
    order.sort_by_key(|e| match e.kind {
        Kind::Slot => 0,
        Kind::Bootloader | Kind::Shim | Kind::Rescue | Kind::Migration | Kind::Extra => 1,
        Kind::Uki => 2,
        Kind::Notes => 3,
    });
    let result = (|| -> Result<()> {
        for e in &order {
            let (dir, name, label) = match e.kind {
                Kind::Slot => (
                    t.slots.clone(),
                    e.name.clone(),
                    if e.name.starts_with("usr_") { "Asima OS" } else { "integrity data" },
                ),
                Kind::Uki => (t.uki_dir.clone(), boot::uki_name(&v), "boot image"),
                Kind::Bootloader => (t.staged.join(&v), e.name.clone(), "boot loader"),
                Kind::Shim => (t.staged.join(&v), e.name.clone(), "Secure Boot shim"),
                Kind::Rescue => (t.staged.join(&v), e.name.clone(), "Asima Rescue"),
                Kind::Migration => (t.staged.join(&v).join("migrations"), e.name.clone(), "migration steps"),
                // Checked like every other part; the new version's own code knows what it is for.
                Kind::Extra => {
                    (t.staged.join(&v).join("parts").join(&e.kind_name), e.name.clone(), "parts for the new version")
                }
                Kind::Notes => continue,
            };
            let mut report = |n: u64| {
                done += n;
                progress(
                    done as f64 / total as f64,
                    format!("Writing {label} · {:.1} of {:.1} GB", done as f64 / 1e9, total as f64 / 1e9),
                );
            };
            let target = dir.join(&name);
            if mode == Mode::Resume && already_there(e, &target) {
                report(e.size);
                kept.push(name);
                written.push(target);
                continue;
            }
            written.push(write_part(u, e, &dir, &name, &mut report)?);
        }
        Ok(())
    })();
    result?;
    Ok(Installed { version: v, files: written, kept })
}

// ───────────────────────────────────────────────────────────────────────── packing

pub struct PackOptions {
    pub version: String,
    pub channel: String,
    pub expires: Option<String>,
    pub min_from: Option<String>,
    pub created: String,
    pub parts: Vec<(Kind, PathBuf)>,
    /// Parts of other kinds, by name (`aosupd pack --part KIND=FILE`): kinds that only later
    /// versions know. The installed updater keeps them for the new version.
    pub extra: Vec<(String, PathBuf)>,
    pub level: i32,
}

fn sha_file(path: &Path) -> Result<(String, u64)> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let n = std::io::copy(&mut f, &mut h)?;
    Ok((hex::encode(h.finalize()), n))
}

/// Builds and signs an update file. Each part is compressed with zstd (long window) and kept
/// compressed only when that makes it smaller.
pub fn pack(o: &PackOptions, key: &SigningKey, out: &Path) -> Result<()> {
    let work = out.with_extension("packing");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    struct Part {
        name: String,
        kind: Kind,
        kind_name: String,
        stored: PathBuf,
        stored_size: u64,
        size: u64,
        sha: String,
        stored_sha: String,
        zstd: bool,
    }
    let all: Vec<(String, PathBuf)> =
        o.parts.iter().map(|(k, p)| (k.name().to_string(), p.clone())).chain(o.extra.iter().cloned()).collect();
    let mut parts = Vec::new();
    for (kind_name, path) in &all {
        let kind = Kind::parse(kind_name)?;
        let name = path.file_name().context("part without a name")?.to_string_lossy().into_owned();
        let (sha, size) = sha_file(path)?;
        let z = work.join(format!("{name}.zst"));
        {
            let mut enc = zstd::stream::write::Encoder::new(File::create(&z)?, o.level)?;
            enc.long_distance_matching(true)?;
            enc.window_log(WINDOW_LOG)?;
            enc.multithread(std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1))?;
            std::io::copy(&mut File::open(path)?, &mut enc)?;
            enc.finish()?.sync_all()?;
        }
        let zsize = fs::metadata(&z)?.len();
        let (stored, zstd) = if zsize < size - size / 32 { (z, true) } else { (path.clone(), false) };
        let (stored_sha, stored_size) = sha_file(&stored)?;
        parts.push(Part { name, kind, kind_name: kind_name.clone(), stored, stored_size, size, sha, stored_sha, zstd });
    }
    // What the free-space check counts: the system image and boot entry, and parts of new kinds
    // (they may be large, and they wait in the pool until the new version uses them).
    let unpacked: u64 =
        parts.iter().filter(|p| matches!(p.kind, Kind::Slot | Kind::Uki | Kind::Extra)).map(|p| p.size).sum();
    let head = |mlen: usize| HEADER + mlen + SIG_LEN;
    // Offsets depend on the manifest's length, which contains the offsets: repeat until the
    // length no longer changes (aligned offsets make that two passes in practice).
    let mut mlen = 0usize;
    let mut manifest = String::new();
    let mut settled = false;
    for _ in 0..4 {
        let mut offset = (head(mlen) as u64).div_ceil(ALIGN) * ALIGN;
        let mut m = format!("FORMAT={FORMAT}\nPRODUCT=Asima OS\nVERSION={}\nARCH=x86_64\nCHANNEL={}\nCREATED={}\nUNPACKED_SIZE={unpacked}\n",
                            o.version, o.channel, o.created);
        if let Some(e) = &o.expires {
            m += &format!("EXPIRES={e}\n");
        }
        if let Some(f) = &o.min_from {
            m += &format!("MIN_FROM={f}\n");
        }
        for p in &parts {
            m += &format!(
                "FILE {} {} {offset} {} {} {} {} {}\n",
                p.name,
                p.kind_name,
                p.stored_size,
                p.size,
                p.sha,
                p.stored_sha,
                if p.zstd { "zstd" } else { "none" }
            );
            offset = (offset + p.stored_size).div_ceil(ALIGN) * ALIGN;
        }
        if m.len() == mlen {
            manifest = m;
            settled = true;
            break;
        }
        mlen = m.len();
    }
    ensure!(settled, "the manifest did not settle");
    parse_manifest(&manifest).context("the update would not be valid")?;

    let mut header = Vec::with_capacity(HEADER);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&FORMAT.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&(mlen as u32).to_le_bytes());
    header.extend_from_slice(&(SIG_LEN as u32).to_le_bytes());
    header.extend_from_slice(&0u32.to_le_bytes());
    let mut signed = header;
    signed.extend_from_slice(manifest.as_bytes());
    let sig = key.sign(&signed);

    let tmp = out.with_extension("partial");
    let mut f = File::create(&tmp)?;
    f.write_all(&signed)?;
    f.write_all(&sig.to_bytes())?;
    let mut pos = (signed.len() + SIG_LEN) as u64;
    for p in &parts {
        let aligned = pos.div_ceil(ALIGN) * ALIGN;
        f.write_all(&vec![0u8; (aligned - pos) as usize])?;
        std::io::copy(&mut File::open(&p.stored)?, &mut f)?;
        pos = aligned + p.stored_size;
    }
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, out)?;
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_keys_and_file_signatures() {
        let dir = std::env::temp_dir().join(format!("au-keys-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        keygen(&dir.join("release")).unwrap();
        // Only the current key at first.
        assert_eq!(load_trusted_keys(&dir.join("release.pub")).unwrap().len(), 1);
        keygen(&dir.join("release-next")).unwrap();
        assert_eq!(load_trusted_keys(&dir.join("release.pub")).unwrap().len(), 2);
        assert_eq!(load_trusted_keys(&dir).unwrap().len(), 2);
        // A file signed with the next key is accepted by a system that trusts both.
        let sums = dir.join("SHA256SUMS");
        fs::write(&sums, "abc  asimaos.iso\n").unwrap();
        sign_file(&load_secret_key(&dir.join("release-next.key")).unwrap(), &sums).unwrap();
        verify_file(&load_trusted_keys(&dir.join("release.pub")).unwrap(), &sums).unwrap();
        assert!(verify_file(&[load_public_key(&dir.join("release.pub")).unwrap()], &sums).is_err());
        // A changed file is refused.
        fs::write(&sums, "abd  asimaos.iso\n").unwrap();
        assert!(verify_file(&load_trusted_keys(&dir).unwrap(), &sums).is_err());
        let pem = public_key_pem(&load_public_key(&dir.join("release.pub")).unwrap());
        assert!(pem.starts_with("-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA") && pem.len() == 113);
        assert_eq!(fingerprint(&load_public_key(&dir.join("release.pub")).unwrap()).len(), 79);
        let _ = fs::remove_dir_all(&dir);
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aosupd-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn build(d: &Path, version: &str) -> (PathBuf, VerifyingKey) {
        keygen(&d.join("k")).unwrap();
        let key = load_secret_key(&d.join("k.key")).unwrap();
        let usr = d.join(format!("usr_{version}.erofs"));
        // Compressible "filesystem" content and incompressible boot data.
        fs::write(&usr, b"Asima OS system files ".repeat(200_000)).unwrap();
        let mut noise = vec![0u8; 300_000];
        File::open("/dev/urandom").unwrap().read_exact(&mut noise).unwrap();
        fs::write(d.join(format!("verity_{version}.img")), &noise).unwrap();
        fs::write(d.join(format!("asimaos_{version}.efi")), &noise[..200_000]).unwrap();
        fs::write(d.join("NOTES"), "- Plasma 6.8.4\n- Faster start-up\n").unwrap();
        fs::write(d.join("example.bin"), "a part only later versions know").unwrap();
        let o = PackOptions {
            version: version.into(),
            channel: "stable".into(),
            expires: Some("2099-12-31".into()),
            min_from: None,
            created: "2026-12-01T10:00:00Z".into(),
            parts: vec![
                (Kind::Slot, usr),
                (Kind::Slot, d.join(format!("verity_{version}.img"))),
                (Kind::Uki, d.join(format!("asimaos_{version}.efi"))),
                (Kind::Notes, d.join("NOTES")),
            ],
            extra: vec![("example".into(), d.join("example.bin"))],
            level: 19,
        };
        let out = d.join(format!("asimaos-{version}.aosupd"));
        pack(&o, &key, &out).unwrap();
        (out, key.verifying_key())
    }

    #[test]
    fn round_trip() {
        let d = scratch("rt");
        let (file, pubkey) = build(&d, "2026.12.01.1");
        let mut u = Update::open(File::open(&file).unwrap(), &pubkey).unwrap();
        assert_eq!(u.manifest.version, "2026.12.01.1");
        u.check_parts(&mut |_, _| {}).unwrap();
        assert!(u.notes().unwrap().contains("Plasma"));
        let usr = u.manifest.files_of(Kind::Slot).find(|e| e.name.starts_with("usr_")).unwrap().clone();
        assert!(usr.zstd && usr.stored * 10 < usr.size, "compressible parts are compressed");
        let uki = u.manifest.files_of(Kind::Uki).next().unwrap().clone();
        assert!(!uki.zstd, "incompressible parts are stored as they are");
        let t = Targets { slots: d.join("slots"), uki_dir: d.join("esp"), staged: d.join("slots/staged") };
        let done = install(&mut u, &t, Mode::Resume, &mut |_, _| {}).unwrap();
        assert!(done.kept.is_empty());
        assert_eq!(
            fs::read(d.join("slots/usr_2026.12.01.1.erofs")).unwrap(),
            fs::read(d.join("usr_2026.12.01.1.erofs")).unwrap()
        );
        assert!(d.join("esp/asimaos_2026.12.01.1.efi").is_file());
        // A part of a kind this updater doesn't know waits, checked, for the new version.
        let extra = u.manifest.files.iter().find(|e| e.kind == Kind::Extra).unwrap();
        assert_eq!(extra.kind_name, "example");
        assert_eq!(
            fs::read_to_string(d.join("slots/staged/2026.12.01.1/parts/example/example.bin")).unwrap(),
            "a part only later versions know"
        );
        // Interrupted later: the image is complete, the verity data half-written. Resuming keeps
        // the image and writes only what is missing; rewriting writes everything.
        fs::write(d.join("slots/verity_2026.12.01.1.img"), b"half").unwrap();
        fs::write(d.join("slots/.verity_2026.12.01.1.img.partial"), b"junk").unwrap();
        let done = install(&mut u, &t, Mode::Resume, &mut |_, _| {}).unwrap();
        assert!(
            done.kept.contains(&"usr_2026.12.01.1.erofs".to_string())
                && !done.kept.contains(&"verity_2026.12.01.1.img".to_string())
        );
        assert_eq!(
            fs::read(d.join("slots/verity_2026.12.01.1.img")).unwrap(),
            fs::read(d.join("verity_2026.12.01.1.img")).unwrap()
        );
        assert!(!d.join("slots/.verity_2026.12.01.1.img.partial").exists());
        let done = install(&mut u, &t, Mode::Rewrite, &mut |_, _| {}).unwrap();
        assert!(done.kept.is_empty());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn refuses_bad_files() {
        let d = scratch("bad");
        let (file, pubkey) = build(&d, "2026.12.01.1");
        let good = fs::read(&file).unwrap();
        let open = |bytes: &[u8]| {
            let p = d.join("x.aosupd");
            fs::write(&p, bytes).unwrap();
            Update::open(File::open(&p).unwrap(), &pubkey).map(|_| ()).map_err(|e| (refused(&e), e.to_string()))
        };
        assert!(open(&good).is_ok());
        let (kind, msg) = open(&good[..good.len() - 1]).unwrap_err();
        assert!(kind == Some(Refused::Damaged) && msg.contains("incomplete"), "cut-off download: {msg}");
        let (kind, msg) = open(&good[..100]).unwrap_err();
        assert!(kind == Some(Refused::Damaged) && msg.contains("incomplete"), "cut inside the manifest: {msg}");
        let mut longer = good.clone();
        longer.push(0);
        assert_eq!(open(&longer).unwrap_err().0, Some(Refused::Damaged));
        let mut tampered = good.clone();
        tampered[40] ^= 1; // inside the manifest
        let (kind, msg) = open(&tampered).unwrap_err();
        assert!(kind == Some(Refused::NotSigned) && msg.contains("isn't signed"));
        let mut magic = good.clone();
        magic[0] = b'X';
        let (kind, msg) = open(&magic).unwrap_err();
        assert!(kind == Some(Refused::NotSigned) && msg.contains("not an Asima OS update"));
        assert_eq!(open(b"tiny").unwrap_err().0, Some(Refused::NotSigned));
        let mut future = good.clone();
        future[8] = 2; // format 2
        assert_eq!(open(&future).unwrap_err().0, Some(Refused::NewerFormat));
        // A flipped bit in a part: the signature still holds, the part check catches it.
        let mut part = good.clone();
        let last = part.len() - 10;
        part[last] ^= 1;
        let p = d.join("y.aosupd");
        fs::write(&p, &part).unwrap();
        let mut u = Update::open(File::open(&p).unwrap(), &pubkey).unwrap();
        let e = u.check_parts(&mut |_, _| {}).unwrap_err();
        assert!(refused(&e) == Some(Refused::Damaged) && e.to_string().contains("damaged"));
        // A flipped byte in the padding between parts is caught too.
        let first = Update::open(File::open(&file).unwrap(), &pubkey).unwrap();
        let mut offs: Vec<u64> = first.manifest.files.iter().map(|e| e.offset).collect();
        offs.sort();
        let pad_at = (offs[1] - 1) as usize;
        let mut padded = good.clone();
        padded[pad_at] = 7;
        let p = d.join("z.aosupd");
        fs::write(&p, &padded).unwrap();
        let mut u = Update::open(File::open(&p).unwrap(), &pubkey).unwrap();
        assert!(u.check_parts(&mut |_, _| {}).unwrap_err().to_string().contains("damaged"));
        // Another key's signature.
        keygen(&d.join("other")).unwrap();
        let other = load_public_key(&d.join("other.pub")).unwrap();
        assert!(Update::open(File::open(&file).unwrap(), &other).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn strict_manifest() {
        let sha = "a".repeat(64);
        let base = format!(
            "FORMAT=1\nPRODUCT=Asima OS\nVERSION=2026.12.01.1\nARCH=x86_64\n\
            FILE usr_2026.12.01.1.erofs slot 4096 10 10 {sha} {sha} none\n\
            FILE verity_2026.12.01.1.img slot 8192 10 10 {sha} {sha} none\n\
            FILE asimaos_2026.12.01.1.efi uki 12288 10 10 {sha} {sha} none\n"
        );
        assert!(parse_manifest(&base).is_ok());
        assert!(parse_manifest(&base.replace("usr_2026.12.01.1.erofs", "../usr.erofs")).is_err(), "path traversal");
        assert!(parse_manifest(&base.replace("FORMAT=1", "FORMAT=2")).is_err(), "newer format");
        assert!(parse_manifest(&base.replace("VERSION=2026.12.01.1", "VERSION=2026.12.01.1;rm")).is_err());
        assert!(parse_manifest(&base.replace(" uki ", " shell ")).is_err(), "the boot entry is required");
        // A kind this updater doesn't know is accepted (kept for the new version); a malformed
        // kind is not, since it also names a folder.
        let extra = format!("{base}FILE fw.bin firmware-2 16384 10 10 {sha} {sha} none\n");
        let m = parse_manifest(&extra).unwrap();
        let e = m.files.iter().find(|e| e.name == "fw.bin").unwrap();
        assert!(e.kind == Kind::Extra && e.kind_name == "firmware-2");
        let too_long = "f".repeat(33);
        for bad in ["Firmware", "../fw", "fw/x", "-fw", "2fw", too_long.as_str()] {
            assert!(parse_manifest(&extra.replace("firmware-2", bad)).is_err(), "malformed kind {bad}");
        }
        assert!(
            parse_manifest(&format!("{extra}FILE fw.bin other 20480 10 10 {sha} {sha} none\n")).is_err(),
            "a part is listed twice"
        );
        assert!(
            parse_manifest(&base.replace("asimaos_2026.12.01.1.efi", "asimaos_2026.11.01.1.efi")).is_err(),
            "boot entry of another version"
        );
    }

    /// The reader runs as root on files anyone can hand it, so damaged files must never crash it:
    /// each one is refused, or the part check catches it. Hundreds of random damages to a good file
    /// (flipped bits, overwritten bytes, cut-off downloads), then to the manifest text alone.
    #[test]
    fn damaged_files_never_crash() {
        let d = scratch("fuzz");
        let (file, pubkey) = build(&d, "2026.12.01.1");
        let good = fs::read(&file).unwrap();
        let p = d.join("damaged.aosupd");
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..400 {
            let mut bytes = good.clone();
            let at = |r: u64, len: usize| (r % len as u64) as usize;
            match round % 4 {
                0 => {
                    let i = at(next(), bytes.len());
                    bytes[i] ^= 1 << (next() % 8);
                }
                1 => {
                    let i = at(next(), 4096); // the header, manifest, signature and padding
                    bytes[i] = next() as u8;
                }
                2 => bytes.truncate(at(next(), bytes.len())),
                _ => {
                    for _ in 0..8 {
                        let i = at(next(), bytes.len());
                        bytes[i] = next() as u8;
                    }
                }
            }
            fs::write(&p, &bytes).unwrap();
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match Update::open(File::open(&p).unwrap(), &pubkey) {
                    Err(_) => true,
                    Ok(mut u) => bytes == good || u.check_parts(&mut |_, _| {}).is_err(),
                }
            }));
            assert!(caught.unwrap_or(false), "round {round}: a damaged file was accepted, or the reader crashed");
        }
        let mlen = u32::from_le_bytes(good[12..16].try_into().unwrap()) as usize;
        let text = good[HEADER..HEADER + mlen].to_vec();
        for round in 0..2000 {
            let mut t = text.clone();
            for _ in 0..1 + next() % 4 {
                let i = (next() % t.len() as u64) as usize;
                t[i] = b" =\n0aZ.-_/9"[(next() % 11) as usize];
            }
            let s = String::from_utf8_lossy(&t).into_owned();
            let survived = std::panic::catch_unwind(|| parse_manifest(&s).map(|_| ()));
            assert!(survived.is_ok(), "round {round}: the manifest parser crashed");
        }
        let _ = fs::remove_dir_all(d);
    }
}
