//! What the three engines share: their output (plain text for a terminal, or one JSON object per
//! line for the graphical apps), running system tools, the Asima OS disk's partitions, and
//! reading secrets without showing them.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Progress and messages. With `json`, one object per line on stdout:
/// step {id, state} · progress {value, detail} · log {message} · error {message} · done …
pub struct Out {
    pub json: bool,
    /// Plain output's step titles in bold red (the installer), or "==> " (Rescue, repair).
    pub fancy: bool,
}

impl Out {
    pub fn event(&self, v: Value) {
        if self.json {
            println!("{v}");
            let _ = std::io::stdout().flush();
        }
    }

    pub fn say(&self, msg: &str) {
        if self.json {
            self.event(json!({ "event": "log", "message": msg }));
        } else {
            println!("{msg}");
        }
    }

    /// A step: `title` is for the terminal.
    pub fn step(&self, id: &str, state: &str, title: &str) {
        if self.json {
            self.event(json!({ "event": "step", "id": id, "state": state }));
        } else if state == "active" {
            if self.fancy {
                println!("\n\x1b[1;31m==> {title}\x1b[0m");
            } else {
                println!("==> {title}");
            }
        }
    }

    pub fn progress(&self, value: f64, detail: &str) {
        if self.json {
            self.event(json!({ "event": "progress", "value": (value * 1000.0).round() / 1000.0, "detail": detail }));
        } else if self.fancy && !detail.is_empty() {
            print!("\r    {detail}        ");
            let _ = std::io::stdout().flush();
        }
    }

    /// The final error: an event, or a line on stderr with the engine's name.
    pub fn error(&self, name: &str, msg: &str) {
        if self.json {
            self.event(json!({ "event": "error", "message": msg }));
        } else {
            eprintln!("{name}: {msg}");
        }
    }
}

/// An error meant for people: shown as is.
#[derive(Debug)]
pub struct Stop(pub String);

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Stop {}

/// Stops the engine with a message for people.
pub fn stop<T>(msg: impl Into<String>) -> Result<T> {
    Err(Stop(msg.into()).into())
}

/// Runs a tool; its standard output when it succeeds.
pub fn run(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("{program} could not be started"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{program} failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs a tool and says only whether it succeeded (its output goes nowhere).
pub fn ok(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Runs a tool with `input` on its standard input (secrets never go on the command line).
pub fn run_input(program: &str, args: &[&str], input: &[u8], env: &[(&str, &str)]) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().with_context(|| format!("{program} could not be started"))?;
    child.stdin.take().context("no stdin")?.write_all(input)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{program} failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The partition of `disk` with GPT label `label` (lsblk), if any.
pub fn part_of(disk: &str, label: &str) -> Option<String> {
    let out = run("lsblk", &["-nrpo", "NAME,PARTLABEL", disk]).ok()?;
    out.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (name, lab) = (it.next()?, it.next()?);
        (lab == label).then(|| name.to_string())
    })
}

/// The disk a partition is on ("/dev/nvme0n1" for "/dev/nvme0n1p2").
pub fn disk_of(part: &str) -> Option<String> {
    let parent = run("lsblk", &["-no", "PKNAME", part]).ok()?;
    let p = parent.lines().next()?.trim();
    (!p.is_empty()).then(|| format!("/dev/{p}"))
}

/// One lsblk column of one device, trimmed ("" when missing).
pub fn lsblk_value(dev: &str, column: &str) -> String {
    run("lsblk", &["-dno", column, dev]).map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default()
}

/// The disks holding Asima OS: an ESP and a pool partition.
pub fn asimaos_disks() -> Vec<String> {
    let out = run("lsblk", &["-dnrpo", "NAME,TYPE"]).unwrap_or_default();
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (name, kind) = (it.next()?, it.next()?);
            (kind == "disk").then(|| name.to_string())
        })
        .filter(|d| {
            part_of(d, "ESP").is_some() && (part_of(d, "crypt_pool").is_some() || part_of(d, "asimaos_pool").is_some())
        })
        .collect()
}

/// One line of standard input (a secret), without its line end.
pub fn read_line(stdin: &mut impl BufRead) -> Option<String> {
    let mut s = String::new();
    match stdin.read_line(&mut s) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(s.trim_end_matches(['\n', '\r']).to_string()),
    }
}

/// Asks on the terminal without showing what is typed.
pub fn ask_secret(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let fd = libc::STDIN_FILENO;
    // SAFETY: termios is plain data; tcgetattr/tcsetattr only read and write it for stdin.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    let have = unsafe { libc::tcgetattr(fd, &mut term) } == 0;
    if have {
        let mut quiet = term;
        quiet.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &quiet) };
    }
    let line = read_line(&mut std::io::stdin().lock());
    if have {
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) };
    }
    eprintln!();
    line.context("nothing was typed")
}

/// Asks on the terminal, showing the answer.
pub fn ask(prompt: &str) -> String {
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    read_line(&mut std::io::stdin().lock()).unwrap_or_default()
}

/// A recovery key (8 groups of 8 "modhex" letters) in a tool's output.
pub fn recovery_key(text: &str) -> Option<String> {
    const MODHEX: &str = "cbdefghijklnrtuv";
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).find_map(|w| {
        let groups: Vec<&str> = w.split('-').collect();
        (groups.len() == 8 && groups.iter().all(|g| g.len() == 8 && g.chars().all(|c| MODHEX.contains(c))))
            .then(|| w.to_string())
    })
}

/// A version like 2026.11.22.1.
pub fn valid_version(v: &str) -> bool {
    let p: Vec<&str> = v.split('.').collect();
    p.len() == 4
        && [4, 2, 2].iter().zip(&p).all(|(n, s)| s.len() == *n && s.bytes().all(|b| b.is_ascii_digit()))
        && !p[3].is_empty()
        && p[3].bytes().all(|b| b.is_ascii_digit())
}

/// Whether version `a` is older than `b` (both YYYY.MM.DD.N).
pub fn older(a: &str, b: &str) -> bool {
    let key = |v: &str| v.split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    key(a) < key(b)
}

/// A Linux account name: lowercase letters, digits, - and _, starting with a letter or _, at most 32.
pub fn valid_username(name: &str) -> bool {
    let mut c = name.chars();
    matches!(c.next(), Some('a'..='z' | '_'))
        && name.len() <= 32
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

pub fn read_file(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let out =
            "A secret recovery key:\n    fkcrtjbg-ndhvhjrl-bucdbhit-tirlukce-hkjgrdlg-kbtnbilh-djvbrkfu-ihfcehcn\n";
        assert_eq!(recovery_key(out).unwrap().len(), 71);
        assert!(recovery_key("no key").is_none());
        assert!(valid_version("2026.11.22.1") && !valid_version("2026.11.22") && !valid_version("26.11.22.1"));
        assert!(older("2026.11.21.1", "2026.11.22.1") && older("2026.11.22.2", "2026.11.22.10"));
        assert!(!older("2026.11.22.1", "2026.11.22.1"));
        assert!(valid_username("asha") && valid_username("_svc") && !valid_username("Asha") && !valid_username("1x"));
    }
}
