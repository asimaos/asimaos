//! The problem report: what someone helping needs, collected on this computer only, as
//! plain text. Asima Maintenance shows it in full, the person reads and edits it, and only then
//! saves or copies it; nothing leaves the computer (Asima Hub adds "send" later).
//!
//! It never contains secrets: no passwords, keys or recovery keys, no file contents from /home.
//! Logs can contain names (the computer's, an account's); the person sees them before saving.

use crate::{crashes, health};
use asimaos_svc::{cmd, efi, system, Root};
use std::fmt::Write;
use std::fs;

const JOURNAL_LINES: &str = "150";

fn section(r: &mut String, title: &str, body: &str) {
    let _ = writeln!(r, "## {title}\n");
    let body = body.trim_end();
    let _ = writeln!(r, "{}\n", if body.is_empty() { "(nothing)" } else { body });
}

async fn tool(program: &str, args: &[&str]) -> String {
    if !cmd::exists(program) {
        return format!("({program} is not available)");
    }
    match cmd::run(program, args).await {
        Ok(o) => o.stdout,
        Err(e) => format!("({e})"),
    }
}

pub async fn make(root: &Root) -> String {
    let mut r = String::new();
    let _ = writeln!(r, "# Asima OS problem report\n");
    let _ = writeln!(r, "Version {} · Secure Boot {}\n", system::running_version(root), efi::secure_boot(root));

    let os = fs::read_to_string(root.path("/usr/lib/os-release")).unwrap_or_default();
    let os: String = os
        .lines()
        .filter(|l| l.starts_with("VERSION=") || l.starts_with("BUILD_ID="))
        .map(|l| format!("{l}\n"))
        .collect();
    section(&mut r, "System", &os);

    let health = health::run(root).await;
    let mut checks = String::new();
    for c in health["checks"].as_array().into_iter().flatten() {
        let id = c["id"].as_str().unwrap_or("?");
        let mut detail = c["detail"].as_str().unwrap_or("").to_string();
        // The free space comes in bytes.
        if let (true, Ok(free)) = (id == "space", detail.parse::<u64>()) {
            detail = format!("{:.1} GB free", free as f64 / 1e9);
        }
        let _ = writeln!(
            checks,
            "{} {}{}",
            if c["ok"] == true { "ok  " } else { "FAIL" },
            id,
            if detail.is_empty() { String::new() } else { format!(": {detail}") }
        );
    }
    section(&mut r, "Health check", &checks);

    let mut starts = String::new();
    for c in crashes::list(root).as_array().into_iter().flatten() {
        if c["kind"] == "start" {
            let _ = writeln!(
                starts,
                "failed start of {} at {}: {} ({})",
                c["version"].as_str().unwrap_or("?"),
                c["at"],
                c["detail"].as_str().unwrap_or(""),
                c["check"].as_str().unwrap_or("")
            );
        } else {
            let _ = writeln!(starts, "kernel crash at {}:\n{}", c["at"], c["log"].as_str().unwrap_or(""));
        }
    }
    section(&mut r, "Failed starts and crashes", &starts);

    if !root.is_system() {
        section(&mut r, "Logs", "(test mode: no logs)");
        return r;
    }
    section(&mut r, "Services that failed", &tool("systemctl", &["--failed", "--no-legend", "--plain"]).await);
    let lockdown = fs::read_to_string("/sys/kernel/security/lockdown").unwrap_or_default();
    section(
        &mut r,
        "Security",
        &format!(
            "lockdown: {}{}",
            lockdown.trim(),
            tool("mokutil", &["--sb-state"]).await.lines().next().map(|l| format!("\n{l}")).unwrap_or_default()
        ),
    );
    section(
        &mut r,
        "Storage",
        &tool("df", &["-h", "--output=target,size,used,avail", "/", "/usr", "/var", "/home"]).await,
    );
    section(
        &mut r,
        "Hardware",
        &format!("{}\n{}", tool("lspci", &["-nn"]).await.trim_end(), tool("lsusb", &[]).await.trim_end()),
    );
    section(
        &mut r,
        "Errors since this start",
        &tool("journalctl", &["-b", "-p", "3", "-o", "short-monotonic", "--no-pager", "-n", JOURNAL_LINES]).await,
    );
    section(
        &mut r,
        "Errors of the previous start",
        &tool("journalctl", &["-b", "-1", "-p", "3", "-o", "short-monotonic", "--no-pager", "-n", JOURNAL_LINES]).await,
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn report_has_the_sections() {
        let t = std::env::temp_dir().join(format!("asimaos-report-{}", std::process::id()));
        let root = Root(t.clone());
        fs::create_dir_all(root.path("/usr/lib")).unwrap();
        fs::write(root.path("/usr/lib/os-release"), "IMAGE_VERSION=2026.11.18.1\nVERSION=\"2026.11.18.1 (Beta 5)\"\n")
            .unwrap();
        let r = make(&root).await;
        assert!(
            r.starts_with("# Asima OS problem report") && r.contains("## Health check") && r.contains("2026.11.18.1")
        );
        assert!(!r.to_lowercase().contains("passphrase"));
        let _ = fs::remove_dir_all(t);
    }
}
