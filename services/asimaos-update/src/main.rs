//! aosupd: make and check Asima OS update files (.aosupd).
//!
//!   aosupd keygen PREFIX                       PREFIX.key (secret) + PREFIX.pub
//!   aosupd pack --key K --version V [--channel C] [--expires YYYY-MM-DD] [--min-from V]
//!                [--level N] [--created TIME]
//!                --slot FILE --slot FILE --uki FILE [--bootloader FILE] [--shim FILE --shim FILE] [--rescue FILE]
//!                [--migration FILE]… [--notes FILE] [--part KIND=FILE]… -o OUT.aosupd
//!                                               --part: a part of a kind only later versions know
//!                                               (kept for the new version by the installed updater)
//!   aosupd inspect --key PUB FILE              manifest as JSON (signature checked)
//!   aosupd verify --key PUB FILE               signature, manifest and every part
//!                                               (--key: release.pub also trusts a release-next.pub
//!                                               beside it; a directory trusts every *.pub)
//!   aosupd sign --key KEY FILE                 FILE.sig: a detached Ed25519 signature
//!   aosupd verify-file --key PUB FILE          checks FILE against FILE.sig
//!   aosupd key-info --key PUB                  the key as PEM, and its fingerprint
//!   aosupd install --key PUB --slots DIR --esp DIR [--staged DIR] [--rewrite] [--json] FILE
//!                                               writes a version and makes it "current" in the start
//!                                               menu (used by Asima Rescue); JSON event lines
//!   aosupd boot list|loader-conf --esp DIR      the start menu: current / backup / failed (JSON;
//!                                               with --plain: "ROLE VERSION LEFT DONE ID" lines)
//!   aosupd boot promote|make-current --esp DIR --version V
//!   aosupd boot settle --esp DIR --running V    after a good start (fallback → working = current)
//!   aosupd boot rescue --esp DIR                the Rescue entry exists exactly when its image does

use anyhow::{bail, Context, Result};
use asimaos_update::{
    boot, fingerprint, install, keygen, load_public_key, load_secret_key, load_trusted_keys, pack, public_key_pem,
    sign_file, verify_file, Kind, Mode, PackOptions, Targets, Update,
};
use serde_json::json;
use std::fs::File;
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: aosupd keygen | pack | inspect | verify | install  (see the source header)");
    std::process::exit(2)
}

struct Args(Vec<String>);

impl Args {
    fn take(&mut self, flag: &str) -> Option<String> {
        let i = self.0.iter().position(|a| a == flag)?;
        self.0.remove(i);
        (i < self.0.len()).then(|| self.0.remove(i))
    }
    fn take_all(&mut self, flag: &str) -> Vec<String> {
        let mut v = Vec::new();
        while let Some(x) = self.take(flag) {
            v.push(x);
        }
        v
    }
    fn flag(&mut self, flag: &str) -> bool {
        match self.0.iter().position(|a| a == flag) {
            Some(i) => {
                self.0.remove(i);
                true
            }
            None => false,
        }
    }
    fn none(self) -> Result<()> {
        anyhow::ensure!(self.0.is_empty(), "unexpected {:?}", self.0);
        Ok(())
    }

    fn rest(self) -> Result<PathBuf> {
        match self.0.as_slice() {
            [one] => Ok(PathBuf::from(one)),
            _ => bail!("expected one file, got {:?}", self.0),
        }
    }
}

/// Takes --key and the one remaining argument, the update file; anything else is a mistake.
fn open(mut a: Args) -> Result<Update> {
    let keys = load_trusted_keys(&PathBuf::from(a.take("--key").context("--key PUB is required")?))?;
    let file = a.rest()?;
    Update::open_any(File::open(&file).with_context(|| format!("can't open {}", file.display()))?, &keys)
}

fn event(json_mode: bool, v: serde_json::Value) {
    if json_mode {
        println!("{v}");
    } else if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
        println!("{m}");
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let cmd = args.remove(0);
    let json_mode = args.iter().any(|a| a == "--json");
    if let Err(e) = run(&cmd, Args(args)) {
        if json_mode {
            println!("{}", json!({ "event": "error", "message": e.to_string() }));
        } else {
            eprintln!("aosupd: {e:#}");
        }
        std::process::exit(1);
    }
}

fn run(cmd: &str, mut a: Args) -> Result<()> {
    match cmd {
        "keygen" => keygen(&a.rest()?),
        "sign" => {
            let key = load_secret_key(&PathBuf::from(a.take("--key").context("--key KEY is required")?))?;
            let sig = sign_file(&key, &a.rest()?)?;
            println!("{}", sig.display());
            Ok(())
        }
        "verify-file" => {
            let keys = load_trusted_keys(&PathBuf::from(a.take("--key").context("--key PUB is required")?))?;
            let file = a.rest()?;
            verify_file(&keys, &file)?;
            println!("{}: the signature is good", file.display());
            Ok(())
        }
        "key-info" => {
            let key = load_public_key(&PathBuf::from(a.take("--key").context("--key PUB is required")?))?;
            a.none()?;
            print!("{}", public_key_pem(&key));
            println!("Fingerprint (SHA-256): {}", fingerprint(&key));
            Ok(())
        }
        "pack" => {
            let key = load_secret_key(&PathBuf::from(a.take("--key").context("--key is required")?))?;
            let mut parts: Vec<(Kind, PathBuf)> = Vec::new();
            for (flag, kind) in [
                ("--slot", Kind::Slot),
                ("--uki", Kind::Uki),
                ("--bootloader", Kind::Bootloader),
                ("--shim", Kind::Shim),
                ("--rescue", Kind::Rescue),
                ("--migration", Kind::Migration),
                ("--notes", Kind::Notes),
            ] {
                parts.extend(a.take_all(flag).into_iter().map(|p| (kind, PathBuf::from(p))));
            }
            let extra = a
                .take_all("--part")
                .into_iter()
                .map(|v| {
                    let (kind, file) = v.split_once('=').context("--part needs KIND=FILE")?;
                    Ok((kind.to_string(), PathBuf::from(file)))
                })
                .collect::<Result<Vec<_>>>()?;
            let o = PackOptions {
                version: a.take("--version").context("--version is required")?,
                channel: a.take("--channel").unwrap_or_else(|| "stable".into()),
                expires: a.take("--expires"),
                min_from: a.take("--min-from"),
                created: a.take("--created").unwrap_or_default(),
                parts,
                extra,
                level: a.take("--level").map(|l| l.parse()).transpose()?.unwrap_or(19),
            };
            let out = PathBuf::from(a.take("-o").context("-o OUT.aosupd is required")?);
            pack(&o, &key, &out)
        }
        "inspect" | "verify" => {
            let verify = cmd == "verify";
            let mut u = open(a)?;
            if verify {
                u.check_parts(&mut |_, _| {})?;
            }
            let m = &u.manifest;
            let files: Vec<_> = m
                .files
                .iter()
                .map(|e| {
                    json!({
                "name": e.name, "kind": e.kind_name, "size": e.size, "stored": e.stored, "zstd": e.zstd })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "signature": "valid", "partsChecked": verify, "version": m.version, "channel": m.channel,
                    "arch": m.arch, "expires": m.expires, "minFrom": m.min_from,
                    "unpackedSize": m.unpacked, "files": files,
                }))?
            );
            Ok(())
        }
        "install" => {
            let json_mode = a.flag("--json");
            let slots = PathBuf::from(a.take("--slots").context("--slots DIR is required")?);
            let esp = PathBuf::from(a.take("--esp").context("--esp DIR (the ESP's mount point) is required")?);
            let staged = a.take("--staged").map(PathBuf::from).unwrap_or_else(|| slots.join("staged"));
            let mode = if a.flag("--rewrite") { Mode::Rewrite } else { Mode::Resume };
            event(json_mode, json!({ "event": "step", "id": "verify", "state": "active" }));
            let mut u = open(a)?;
            u.check_parts(&mut |done, total| {
                if json_mode && done % (64 << 20) < (1 << 20) {
                    println!("{}", json!({ "event": "progress", "value": 0.05 * done as f64 / total.max(1) as f64, "detail": "Checking the update file" }));
                }
            })?;
            event(json_mode, json!({ "event": "step", "id": "verify", "state": "done" }));
            event(json_mode, json!({ "event": "step", "id": "write", "state": "active" }));
            let mut last = 0.0;
            let done =
                install(&mut u, &Targets { slots, uki_dir: esp.join(boot::UKI_DIR), staged }, mode, &mut |p, d| {
                    if json_mode && p - last >= 0.005 {
                        last = p;
                        println!("{}", json!({ "event": "progress", "value": 0.05 + 0.9 * p, "detail": d }));
                    }
                })?;
            let dropped = boot::promote(&esp, &done.version)?;
            event(json_mode, json!({ "event": "step", "id": "write", "state": "done" }));
            event(
                json_mode,
                json!({ "event": "done", "message": format!("Asima OS {} is written.", done.version),
                                     "version": done.version, "kept": done.kept, "dropped": dropped }),
            );
            Ok(())
        }
        "boot" => {
            let what = if a.0.is_empty() { String::new() } else { a.0.remove(0) };
            let esp = PathBuf::from(a.take("--esp").context("--esp DIR is required")?);
            match what.as_str() {
                "list" if a.flag("--plain") => {
                    // For shell scripts (Asima Rescue): "ROLE VERSION LEFT DONE ID", current first;
                    // LEFT and DONE are "-" for an entry that started fine.
                    let mut es = boot::entries(&esp);
                    es.sort_by_key(|e| {
                        ["current", "backup", "failed"].iter().position(|r| *r == e.role.name()).unwrap_or(9)
                    });
                    for e in es {
                        let (left, done) =
                            e.tries.map_or(("-".to_string(), "-".to_string()), |t| (t.0.to_string(), t.1.to_string()));
                        println!("{} {} {left} {done} {}", e.role.name(), e.version, e.id());
                    }
                }
                "list" => {
                    let list: Vec<_> = boot::entries(&esp).iter().map(|e| json!({
                        "role": e.role.name(), "version": e.version, "id": e.id(), "blessed": e.blessed(), "bad": e.bad(),
                        "triesLeft": e.tries.map(|t| t.0), "triesDone": e.tries.map(|t| t.1) })).collect();
                    println!("{}", serde_json::to_string_pretty(&list)?);
                }
                "loader-conf" => print!("{}", boot::loader_conf()),
                "promote" => println!(
                    "{}",
                    json!({ "dropped": boot::promote(&esp, &a.take("--version").context("--version V is required")?)? })
                ),
                "make-current" => println!(
                    "{}",
                    json!({ "dropped": boot::make_current(&esp, &a.take("--version").context("--version V is required")?)? })
                ),
                "settle" => println!(
                    "{}",
                    json!({ "changed": boot::settle(&esp, &a.take("--running").context("--running V is required")?)? })
                ),
                "rescue" => boot::sync_rescue(&esp)?,
                _ => bail!("aosupd boot list | loader-conf | promote | make-current | settle | rescue"),
            }
            Ok(())
        }
        _ => usage(),
    }
}
