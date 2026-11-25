//! Secure Boot with the Asima OS key (D16) and the disk's secrets: TPM unlock (D10), passphrase and
//! recovery key. Secrets reach systemd-cryptenroll through its environment ($PASSWORD,
//! $NEWPASSWORD), never through its command line.

use anyhow::{bail, ensure, Context, Result};
use asimaos_svc::{cmd, efi, Root};
use serde_json::{json, Value};

pub const ASIMAOS_KEY: &str = "/usr/share/asimaos/keys/asimaos.der";
/// TPM unlock with systemd-pcrlock: the helper that keeps the policy in the TPM up to date, and
/// the policy file systemd-cryptenroll binds the disk to.
pub const PCRLOCK_HELPER: &str = "/usr/lib/asimaos/asimaos-pcrlock";
pub const PCRLOCK_JSON: &str = "/var/lib/systemd/pcrlock.json";
/// A re-enrolment waiting for its second step: {"new": slot, "old": [slots]}.
pub const TPM_ROTATION: &str = "/var/lib/asimaos/tpm2-rotation.json";
/// A TPM PIN (D49): at least this many characters, of any kind.
pub const PIN_MIN: usize = 4;
const PIN_MAX: usize = 64;
/// The Argon2id memory that hardens a TPM PIN. systemd sizes it from this computer's RAM by
/// default (up to half of it), and every start must find that much free before it can unlock.
const PIN_ARGON2ID_MEMORY: &str = "--tpm2-argon2id-memory=512M";
pub const POOL_MAPPING: &str = "crypt_pool";

pub async fn status(root: &Root) -> Value {
    let key = root.path(ASIMAOS_KEY);
    let key_s = key.to_string_lossy().into_owned();
    let mok = cmd::exists("mokutil") && key.is_file();
    let key_approved =
        mok && cmd::probe("mokutil", &["--test-key", &key_s]).await.is_some_and(|o| o.contains("already enrolled"));
    let key_pending = mok && cmd::probe("mokutil", &["--list-new"]).await.is_some_and(|o| o.contains("[key"));
    let encrypted = root.path(&format!("/dev/mapper/{POOL_MAPPING}")).exists();
    let (mut tpm_enrolled, mut tpm_pin, mut recovery) = (false, false, false);
    if encrypted && root.is_system() {
        if let Ok(dev) = luks_device().await {
            for (_, kind) in keyslots(&dev).await.unwrap_or_default() {
                tpm_enrolled |= kind == "tpm2";
                recovery |= kind == "recovery";
            }
            tpm_pin = tpm_enrolled && tpm_pins(&luks_meta(&dev).await).iter().any(|&(_, pin)| pin);
        }
    }
    let lockdown = std::fs::read_to_string(root.path("/sys/kernel/security/lockdown")).unwrap_or_default();
    let lockdown =
        lockdown.split_whitespace().find(|w| w.starts_with('[')).map(|w| w.trim_matches(['[', ']']).to_string());
    let lsms: Vec<String> = std::fs::read_to_string(root.path("/sys/kernel/security/lsm"))
        .unwrap_or_default()
        .trim()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let verity = cmd::probe("dmsetup", &["status", "usr"]).await.is_some_and(|o| o.contains(" verity V"));
    let other_keys = if mok && root.is_system() { other_keys(root).await.unwrap_or_default() } else { Vec::new() };
    let retire_pending = mok && cmd::probe("mokutil", &["--list-delete"]).await.is_some_and(|o| o.contains("[key"));
    json!({
        "managed": true,
        "secureBoot": efi::secure_boot(root),
        "keyApproved": key_approved,
        "keyPending": key_pending,
        "encrypted": encrypted,
        "tpm": root.path("/sys/class/tpm/tpm0").exists(),
        "tpmEnrolled": tpm_enrolled,
        "tpmPin": tpm_pin,
        "tpmKnown": encrypted,
        "recoveryEnrolled": recovery,
        "lockdown": lockdown.unwrap_or_else(|| "none".into()),
        "lsms": lsms,
        "verity": verity,
        "otherKeys": other_keys,
        "ownKeys": crate::ownkeys::status(root).await,
        "retirePending": retire_pending,
        "extensions": extensions(root).await,
    })
}

/// Signed system extensions (D55): the ones merged into /usr now, and those asimaos-sysext left
/// out, with the reason.
async fn extensions(root: &Root) -> Value {
    let merged: Vec<String> = if root.is_system() {
        cmd::probe("systemd-sysext", &["list", "--json=short"])
            .await
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|e| e["name"].as_str().map(String::from))
            .collect()
    } else {
        Vec::new()
    };
    let dir = root.path("/var/lib/asimaos/refused-extensions");
    let mut refused: Vec<Value> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let file = e.file_name().to_string_lossy().into_owned();
                    let name = file.strip_suffix(".why")?;
                    let why = std::fs::read_to_string(e.path()).ok()?;
                    Some(json!({ "name": name, "why": why.trim() }))
                })
                .collect()
        })
        .unwrap_or_default();
    refused.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json!({ "merged": merged, "refused": refused })
}

// ───────────────────────────────────────────────────────────────────────── Secure Boot (MOK)

/// Queues the Asima OS key for approval at the next start and returns the one-time code to type on
/// the blue MokManager screen. Digits only: MokManager reads the keyboard with the US layout.
pub async fn approve_key(root: &Root, workdir: &std::path::Path) -> Result<String> {
    let key = root.path(ASIMAOS_KEY);
    ensure!(key.is_file(), "The Asima OS key is missing from this system");
    let code = asimaos_svc::random_digits(8)?;
    let hash = cmd::run("mokutil", &[&format!("--generate-hash={code}")]).await?.stdout;
    let hash_file = workdir.join("mok.hash");
    std::fs::write(&hash_file, hash.trim())?;
    let result =
        cmd::run("mokutil", &["--import", &key.to_string_lossy(), "--hash-file", &hash_file.to_string_lossy()]).await;
    let _ = std::fs::remove_file(&hash_file);
    result?;
    Ok(format!("{} {}", &code[..4], &code[4..]))
}

pub async fn cancel_key() -> Result<()> {
    cmd::run("mokutil", &["--revoke-import"]).await.map(|_| ())
}

/// A key approved on this computer (in shim's MOK list).
struct MokKey {
    file: std::path::PathBuf,
    sha256: String,
    name: String,
    since: String,
}

/// The MOK list's keys, exported into `dir` (`mokutil --export` writes MOK-NNNN.der there).
async fn mok_keys(dir: &std::path::Path) -> Result<Vec<MokKey>> {
    std::fs::create_dir_all(dir)?;
    let status = tokio::process::Command::new("mokutil")
        .arg("--export")
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context("mokutil could not be started")?;
    ensure!(status.success(), "The approved keys could not be read");
    let mut keys = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "der"))
        .collect();
    files.sort();
    for file in files {
        let f = file.to_string_lossy().into_owned();
        let out = cmd::run(
            "openssl",
            &[
                "x509",
                "-inform",
                "DER",
                "-in",
                &f,
                "-noout",
                "-subject",
                "-startdate",
                "-fingerprint",
                "-sha256",
                "-nameopt",
                "utf8,sep_comma_plus",
            ],
        )
        .await?;
        let (name, since, sha256) = describe_key(&out.stdout);
        keys.push(MokKey { file, sha256, name, since });
    }
    Ok(keys)
}

/// From `openssl x509 -subject -startdate -fingerprint`: the name (CN, or the whole subject), the
/// date it was made, and the SHA-256 fingerprint (hex, no colons).
fn describe_key(text: &str) -> (String, String, String) {
    let field = |prefix: &str| text.lines().find_map(|l| l.strip_prefix(prefix)).unwrap_or("").trim().to_string();
    let subject = field("subject=");
    let name = subject
        .split(',')
        .find_map(|part| part.trim().strip_prefix("CN=").or_else(|| part.trim().strip_prefix("CN = ")))
        .map(String::from)
        .unwrap_or(subject);
    let since = field("notBefore=");
    let sha = text
        .lines()
        .find_map(|l| l.split_once("Fingerprint=").map(|(_, v)| v))
        .unwrap_or("")
        .replace(':', "")
        .to_lowercase();
    (name, since, sha)
}

async fn key_fingerprint(der: &std::path::Path) -> Option<String> {
    let f = der.to_string_lossy().into_owned();
    let out =
        cmd::probe("openssl", &["x509", "-inform", "DER", "-in", &f, "-noout", "-fingerprint", "-sha256"]).await?;
    Some(describe_key(&out).2)
}

/// The approved keys besides this version's Asima OS key: those of earlier builds, or of
/// another system on this computer. The person chooses which to retire.
async fn other_keys(root: &Root) -> Result<Vec<Value>> {
    let dir = std::env::temp_dir().join(format!("asimaos-mok-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let keys = mok_keys(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
    let current = key_fingerprint(&root.path(ASIMAOS_KEY)).await.unwrap_or_default();
    Ok(keys?
        .into_iter()
        .filter(|k| k.sha256 != current)
        .map(|k| json!({ "id": k.sha256, "name": k.name, "since": k.since }))
        .collect())
}

/// Retires approved keys (by fingerprint) at the next start: shim's MokManager removes them from
/// the MOK list and adds them to MOKX, so nothing signed with them starts again, even if someone
/// approves them anew. Both requests take the same one-time code; this version's own key can't
/// be retired.
pub async fn retire_keys(root: &Root, workdir: &std::path::Path, ids: &[String]) -> Result<String> {
    ensure!(!ids.is_empty(), "Choose the keys to retire");
    let current = key_fingerprint(&root.path(ASIMAOS_KEY)).await.unwrap_or_default();
    ensure!(!ids.contains(&current), "This version's own Asima OS key can't be retired");
    let dir = workdir.join("mok");
    let keys = mok_keys(&dir).await?;
    let chosen: Vec<String> =
        keys.iter().filter(|k| ids.contains(&k.sha256)).map(|k| k.file.to_string_lossy().into_owned()).collect();
    ensure!(chosen.len() == ids.len(), "A chosen key is no longer approved on this computer");
    let code = asimaos_svc::random_digits(8)?;
    let hash = cmd::run("mokutil", &[&format!("--generate-hash={code}")]).await?.stdout;
    let hash_file = workdir.join("mok.hash");
    std::fs::write(&hash_file, hash.trim())?;
    let h = hash_file.to_string_lossy().into_owned();
    let mut delete = vec!["--delete"];
    delete.extend(chosen.iter().map(String::as_str));
    delete.extend(["--hash-file", h.as_str()]);
    let mut block = vec!["--mokx", "--import"];
    block.extend(chosen.iter().map(String::as_str));
    block.extend(["--hash-file", h.as_str()]);
    let result = match cmd::run("mokutil", &delete).await {
        Ok(_) => cmd::run("mokutil", &block).await.map(|_| ()),
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&hash_file);
    let _ = std::fs::remove_dir_all(&dir);
    if result.is_err() {
        let _ = cmd::run("mokutil", &["--revoke-delete"]).await;
    }
    result.context("The keys could not be queued for retiring")?;
    Ok(format!("{} {}", &code[..4], &code[4..]))
}

pub async fn cancel_retire() -> Result<()> {
    let _ = cmd::run("mokutil", &["--mokx", "--revoke-import"]).await;
    cmd::run("mokutil", &["--revoke-delete"]).await.map(|_| ())
}

// ───────────────────────────────────────────────────────────────────────── LUKS2

/// The LUKS2 partition behind the unlocked pool.
pub async fn luks_device() -> Result<String> {
    let out = cmd::run("cryptsetup", &["status", POOL_MAPPING]).await.context("This disk isn't encrypted")?;
    out.stdout
        .lines()
        .find_map(|l| l.trim().strip_prefix("device:").map(|d| d.trim().to_string()))
        .context("The encrypted partition was not found")
}

/// Enrolled key slots: (slot, kind) with kind password | recovery | tpm2 | fido2 | pkcs11.
pub async fn keyslots(dev: &str) -> Result<Vec<(u32, String)>> {
    let out = cmd::run("systemd-cryptenroll", &[dev]).await?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let slot = it.next()?.parse().ok()?;
            Some((slot, it.next()?.to_string()))
        })
        .collect())
}

/// A TPM PIN (D49): at least four characters of any kind (digits are the easiest to type at the
/// start), on one line.
pub fn check_pin(pin: &str) -> Result<()> {
    let n = pin.chars().count();
    ensure!(n >= PIN_MIN, "The PIN needs at least {PIN_MIN} characters");
    ensure!(n <= PIN_MAX, "The PIN can have at most {PIN_MAX} characters");
    ensure!(!pin.chars().any(char::is_control), "The PIN can't contain line breaks or other control characters");
    Ok(())
}

/// TPM unlock (D43): automatic (`pin` None, the recommended way) or, optionally, together with a
/// PIN typed at every start (D49).
pub async fn enroll_tpm(root: &Root, secret: &str, pin: Option<&str>) -> Result<()> {
    if let Some(pin) = pin {
        check_pin(pin)?;
        // Wrong PINs at the start lock the TPM for a while (its dictionary-attack protection);
        // a new PIN couldn't even be tried then, so say so instead of failing vaguely.
        let caps = cmd::probe("tpm2_getcap", &["properties-variable"]).await;
        if let Some(secs) = caps.as_deref().and_then(lockout_seconds) {
            bail!(
                "After several wrong PINs the security chip waits before it accepts one again: try again in about {} minutes. The passphrase works meanwhile.",
                secs.div_ceil(60).max(1)
            );
        }
    }
    ensure!(root.path("/sys/class/tpm/tpm0").exists(), "This computer has no TPM 2.0 chip");
    // The TPM hands out the disk key only when this computer starts one of the Asima OS
    // versions in the start menu (PCR 4: shim, the boot loader and the boot image, as the firmware
    // measured them) with Secure Boot as it is now (PCR 7). The allowed values live in a policy in
    // the TPM (systemd-pcrlock) that Asima Updater keeps up to date, so updates keep working;
    // Asima Rescue's boot image is never allowed. Without Secure Boot, PCR 7 would allow anything.
    ensure!(
        efi::secure_boot(root) == "on",
        "Switch Secure Boot on first (approve the Asima OS key above): TPM unlock relies on it."
    );
    ensure!(
        cmd::probe("/usr/lib/systemd/systemd-pcrlock", &["is-supported", "--quiet"]).await.is_some(),
        "This computer's TPM can't do TPM unlock: it needs TPM 2.0 revision 1.38 or newer with SHA-256."
    );
    let dev = luks_device().await?;
    cmd::run(PCRLOCK_HELPER, &["refresh", "--init"])
        .await
        .map_err(|e| anyhow::anyhow!("The TPM policy could not be made: {e}"))?;
    // Two steps: the new TPM slot is added next to an old one and tried at once; the old one is
    // wiped only after the next start, when the new one still opens the disk (finish_rotation).
    // So setting TPM unlock up again never leaves the disk without it in between.
    let before = tpm_slots(&dev).await?;
    let lock = format!("--tpm2-pcrlock={PCRLOCK_JSON}");
    // `--tpm2-public-key=` (empty): the pcrlock policy alone. Otherwise systemd-cryptenroll binds
    // the slot to the signed PCR 11 policy as well, because the boot image's public key lies in
    // /run/systemd at every start; PCRs that a PC doesn't measure (11 on some) then refuse it.
    let mut args = vec!["--tpm2-device=auto", "--tpm2-public-key=", lock.as_str()];
    // With a PIN, systemd hardens it with Argon2id before it reaches the TPM; wrong PINs count
    // towards the TPM's own lockout. The PIN goes through the environment, never the command line.
    if pin.is_some() {
        args.extend(["--tpm2-with-pin=yes", PIN_ARGON2ID_MEMORY]);
    }
    args.push(dev.as_str());
    let mut env = vec![("PASSWORD", secret)];
    if let Some(pin) = pin {
        env.push(("NEWPIN", pin));
    }
    cmd::run_with("systemd-cryptenroll", &args, None, &env).await.map_err(wrong_secret)?;
    let added: Vec<u32> = tpm_slots(&dev).await?.into_iter().filter(|s| !before.contains(s)).collect();
    let Some(&new) = added.first() else {
        // systemd-cryptenroll adds nothing when a slot with the very same policy exists (a PIN's
        // value isn't part of the policy). Automatic: when such a slot opens the disk now, TPM
        // unlock already works as it would be set up, so nothing changes. With a PIN, or when none
        // opens (the TPM was cleared, say), the old slots go and the slot is made again: trying a
        // new PIN on an old slot would only count as a wrong PIN in the TPM.
        if pin.is_none() {
            let pins = tpm_pins(&luks_meta(&dev).await);
            for &slot in &before {
                let asks = pins.iter().any(|&(s, p)| s == slot && p);
                if !asks && opens(&dev, slot, None).await {
                    return Ok(());
                }
            }
        }
        wipe(&dev, &before).await;
        let _ = std::fs::remove_file(root.path(TPM_ROTATION));
        cmd::run_with("systemd-cryptenroll", &args, None, &env).await.map_err(wrong_secret)?;
        let Some(&slot) = tpm_slots(&dev).await?.first() else { bail!("The TPM slot was not added") };
        if !opens(&dev, slot, pin).await {
            wipe(&dev, &[slot]).await;
            bail!("The TPM wouldn't open the disk with the new setting");
        }
        return Ok(());
    };
    if !opens(&dev, new, pin).await {
        wipe(&dev, &[new]).await;
        bail!("The TPM wouldn't open the disk with the new setting, so nothing was changed");
    }
    if before.is_empty() {
        return Ok(());
    }
    if pin.is_some() {
        // A slot with a PIN can't be tried again after the restart without asking for the PIN, so
        // the old slots go now: the new one has just opened the disk with it, and the passphrase
        // and the recovery key always work.
        wipe(&dev, &before).await;
        let _ = std::fs::remove_file(root.path(TPM_ROTATION));
        return Ok(());
    }
    let pending = serde_json::json!({ "new": new, "old": before });
    let file = root.path(TPM_ROTATION);
    std::fs::create_dir_all(file.parent().context("bad path")?)?;
    std::fs::write(file, pending.to_string())?;
    Ok(())
}

/// After a re-enrolment, once per start (asimaos-maintenanced --tpm-rotation): when the new TPM slot
/// still opens the disk after a fresh start, the old ones are wiped; when it doesn't, the new one
/// goes and the old ones stay.
pub async fn finish_rotation(root: &Root) -> Result<()> {
    let file = root.path(TPM_ROTATION);
    let Ok(text) = std::fs::read_to_string(&file) else { return Ok(()) };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let new = v["new"].as_u64().map(|n| n as u32);
    let old: Vec<u32> =
        v["old"].as_array().into_iter().flatten().filter_map(|s| s.as_u64().map(|n| n as u32)).collect();
    let dev = luks_device().await?;
    let slots = tpm_slots(&dev).await?;
    match new {
        Some(new) if slots.contains(&new) && opens(&dev, new, None).await => {
            let gone: Vec<u32> = old.into_iter().filter(|s| *s != new && slots.contains(s)).collect();
            wipe(&dev, &gone).await;
            eprintln!(
                "asimaos-maintenanced: TPM unlock: the new slot {new} works after a restart; old slots wiped: {gone:?}"
            );
        }
        Some(new) if slots.contains(&new) => {
            wipe(&dev, &[new]).await;
            eprintln!("asimaos-maintenanced: TPM unlock: the new slot {new} didn't open the disk after a restart; the old one stays");
        }
        _ => {}
    }
    std::fs::remove_file(file)?;
    Ok(())
}

/// The disk's TPM2 key slots.
async fn tpm_slots(dev: &str) -> Result<Vec<u32>> {
    Ok(keyslots(dev).await?.into_iter().filter(|(_, kind)| kind == "tpm2").map(|(s, _)| s).collect())
}

/// Whether the TPM opens the disk with key slot `slot` right now (its token, tried without
/// unlocking anything). A slot with a PIN gets `pin` on stdin, never on the command line.
async fn opens(dev: &str, slot: u32, pin: Option<&str>) -> bool {
    let meta = luks_meta(dev).await;
    let Some(token) = token_of(&meta, slot) else { return false };
    let id = token.to_string();
    let args = ["open", "--test-passphrase", "--token-only", "--token-id", &id, dev];
    cmd::run_with("cryptsetup", &args, pin.map(str::as_bytes), &[]).await.is_ok()
}

/// The disk's LUKS2 header as JSON (keyslots, tokens).
async fn luks_meta(dev: &str) -> Value {
    cmd::run("cryptsetup", &["luksDump", "--dump-json-metadata", dev])
        .await
        .ok()
        .and_then(|out| serde_json::from_str(&out.stdout).ok())
        .unwrap_or_default()
}

/// When wrong PINs have locked the TPM (`tpm2_getcap properties-variable`): about how many seconds
/// until it accepts one again (each lockout interval forgives one wrong try).
fn lockout_seconds(caps: &str) -> Option<u64> {
    let value = |key: &str| {
        caps.lines().find_map(|l| l.trim().strip_prefix(key)?.trim_start().strip_prefix(':')).and_then(|v| {
            let v = v.trim();
            match v.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).ok(),
                None => v.parse().ok(),
            }
        })
    };
    if value("inLockout")? != 1 {
        return None;
    }
    let (counter, max) = (value("TPM2_PT_LOCKOUT_COUNTER").unwrap_or(1), value("TPM2_PT_MAX_AUTH_FAIL").unwrap_or(1));
    Some((counter + 1).saturating_sub(max).max(1) * value("TPM2_PT_LOCKOUT_INTERVAL").unwrap_or(0))
}

/// The TPM2 key slots, and whether each asks for a PIN (`tpm2-pin` in its token).
fn tpm_pins(meta: &Value) -> Vec<(u32, bool)> {
    let Some(tokens) = meta["tokens"].as_object() else { return Vec::new() };
    tokens
        .values()
        .filter(|t| t["type"] == "systemd-tpm2")
        .flat_map(|t| {
            let pin = t["tpm2-pin"].as_bool().unwrap_or(false);
            t["keyslots"].as_array().into_iter().flatten().filter_map(move |s| Some((s.as_str()?.parse().ok()?, pin)))
        })
        .collect()
}

/// The LUKS2 token that unlocks key slot `slot`.
fn token_of(meta: &serde_json::Value, slot: u32) -> Option<u32> {
    meta["tokens"].as_object()?.iter().find_map(|(id, tok)| {
        tok["keyslots"]
            .as_array()?
            .iter()
            .any(|s| s.as_str() == Some(&slot.to_string()))
            .then(|| id.parse().ok())
            .flatten()
    })
}

async fn wipe(dev: &str, slots: &[u32]) {
    if slots.is_empty() {
        return;
    }
    let list = slots.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let _ = cmd::run("systemd-cryptenroll", &[&format!("--wipe-slot={list}"), dev]).await;
}

pub async fn remove_tpm(root: &Root) -> Result<()> {
    let _ = std::fs::remove_file(root.path(TPM_ROTATION));
    let dev = luks_device().await?;
    cmd::run("systemd-cryptenroll", &["--wipe-slot=tpm2", &dev]).await?;
    // The policy in the TPM and its copies go too; the disk no longer refers to them.
    let _ = cmd::run(PCRLOCK_HELPER, &["remove"]).await;
    Ok(())
}

/// Replaces the passphrase: a new password slot is added, then every other password slot is
/// removed (the recovery key and TPM unlock stay).
pub async fn change_passphrase(current: &str, new: &str) -> Result<()> {
    ensure!(new.chars().count() >= 8, "The new passphrase needs at least 8 characters");
    let dev = luks_device().await?;
    cmd::run_with(
        "systemd-cryptenroll",
        &["--password", "--wipe-slot=password", &dev],
        None,
        &[("PASSWORD", current), ("NEWPASSWORD", new)],
    )
    .await
    .map(|_| ())
    .map_err(wrong_secret)
}

/// A new recovery key replaces the old one. It is returned once and never stored.
pub async fn new_recovery_key(current: &str) -> Result<String> {
    let dev = luks_device().await?;
    let out = cmd::run_with(
        "systemd-cryptenroll",
        &["--recovery-key", "--wipe-slot=recovery", &dev],
        None,
        &[("PASSWORD", current)],
    )
    .await
    .map_err(wrong_secret)?;
    find_recovery_key(&out.stdout).context("The new recovery key could not be read")
}

/// Recovery keys are 8 groups of 8 "modhex" characters.
pub fn find_recovery_key(text: &str) -> Option<String> {
    const MODHEX: &str = "cbdefghijklnrtuv";
    text.split_whitespace()
        .find(|w| {
            let groups: Vec<&str> = w.split('-').collect();
            groups.len() == 8 && groups.iter().all(|g| g.len() == 8 && g.chars().all(|c| MODHEX.contains(c)))
        })
        .map(String::from)
}

/// A rejected passphrase deserves a plain message.
fn wrong_secret(e: anyhow::Error) -> anyhow::Error {
    let s = e.to_string();
    if s.contains("No key available")
        || s.contains("ermission")
        || s.contains("ailed to unlock")
        || s.contains("Incorrect")
    {
        anyhow::anyhow!("That passphrase or recovery key doesn't unlock this disk")
    } else {
        e
    }
}

pub fn require_encrypted(root: &Root) -> Result<()> {
    if !root.path(&format!("/dev/mapper/{POOL_MAPPING}")).exists() {
        bail!("This disk isn't encrypted (it was installed without encryption)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn recovery_key() {
        let out = "A secret recovery key has been generated for this volume:\n\n    \
                   fkcrtjbg-ndhvhjrl-bucdbhit-tirlukce-hkjgrdlg-kbtnbilh-djvbrkfu-ihfcehcn\n\nPlease save it.";
        assert_eq!(super::find_recovery_key(out).unwrap().len(), 71);
        assert!(super::find_recovery_key("no key here").is_none());
    }

    #[test]
    fn approved_keys() {
        let out = "subject=CN=Asima OS Secure Boot (2026-11-21),O=Asima OS\n\
                   notBefore=Nov 21 09:00:00 2026 GMT\n\
                   sha256 Fingerprint=AB:CD:01\n";
        let (name, since, sha) = super::describe_key(out);
        assert_eq!(name, "Asima OS Secure Boot (2026-11-21)");
        assert_eq!(since, "Nov 21 09:00:00 2026 GMT");
        assert_eq!(sha, "abcd01");
        assert_eq!(super::describe_key("subject=O=Someone\n").0, "O=Someone");
    }

    #[test]
    fn token_of_a_slot() {
        let meta = serde_json::json!({ "tokens": {
            "0": { "type": "systemd-recovery", "keyslots": ["1"] },
            "2": { "type": "systemd-tpm2", "keyslots": ["3"] },
        }});
        assert_eq!(super::token_of(&meta, 3), Some(2));
        assert_eq!(super::token_of(&meta, 1), Some(0));
        assert_eq!(super::token_of(&meta, 5), None);
    }

    #[test]
    fn tpm_pins_and_pin_rules() {
        let meta = serde_json::json!({ "tokens": {
            "0": { "type": "systemd-recovery", "keyslots": ["1"] },
            "1": { "type": "systemd-tpm2", "keyslots": ["2"], "tpm2-pin": false },
            "3": { "type": "systemd-tpm2", "keyslots": ["4"], "tpm2-pin": true, "tpm2_pcrlock": true },
        }});
        let mut pins = super::tpm_pins(&meta);
        pins.sort();
        assert_eq!(pins, vec![(2, false), (4, true)]);
        assert!(super::tpm_pins(&serde_json::json!({})).is_empty());
        assert!(super::check_pin("2468").is_ok() && super::check_pin("blue sky 7").is_ok());
        assert!(super::check_pin("123").is_err());
        assert!(super::check_pin("12\n34").is_err());
        assert!(super::check_pin(&"9".repeat(65)).is_err());
    }

    #[test]
    fn tpm_lockout() {
        // As tpm2_getcap printed it in the VM after one wrong PIN at the start (3 of 3 tries).
        let locked = "TPM2_PT_PERMANENT:\n  lockoutAuthSet: 0\n  inLockout: 1\nTPM2_PT_LOCKOUT_COUNTER: 0x3\n\
                      TPM2_PT_MAX_AUTH_FAIL: 0x3\nTPM2_PT_LOCKOUT_INTERVAL: 0x3E8\nTPM2_PT_LOCKOUT_RECOVERY: 0x3E8\n";
        assert_eq!(super::lockout_seconds(locked), Some(1000));
        let open = locked.replace("inLockout: 1", "inLockout: 0");
        assert_eq!(super::lockout_seconds(&open), None);
        assert_eq!(super::lockout_seconds(""), None);
    }
}
