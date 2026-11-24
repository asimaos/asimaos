//! Running system tools. Secrets never go on a command line: they go through stdin or through the
//! environment of the child (readable only by root), never both visible to other users.

use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub struct Output {
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program` with `args`, writing `stdin` (then closing it) and adding `env`. Fails with the
/// tool's last error line when it exits unsuccessfully.
pub async fn run_with(program: &str, args: &[&str], stdin: Option<&[u8]>, env: &[(&str, &str)]) -> Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().with_context(|| format!("{program} could not be started"))?;
    if let Some(data) = stdin {
        let mut pipe = child.stdin.take().context("no stdin pipe")?;
        pipe.write_all(data).await?;
        drop(pipe);
    }
    let out = child.wait_with_output().await?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        if last.is_empty() {
            bail!("{program} failed");
        }
        bail!("{program} failed: {last}");
    }
    Ok(Output { stdout, stderr })
}

pub async fn run(program: &str, args: &[&str]) -> Result<Output> {
    run_with(program, args, None, &[]).await
}

/// Like `run`, but a failure only means "no answer": for status probes.
pub async fn probe(program: &str, args: &[&str]) -> Option<String> {
    run(program, args).await.ok().map(|o| o.stdout)
}

/// True when `program` can be found in PATH.
pub fn exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}
