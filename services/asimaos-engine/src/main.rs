//! asimaos-engine: Asima OS's one engine for installing (`asimaos-install`), repairing
//! (`asimaos-repair`) and Asima Rescue (`asimaos-rescue-engine`). The three names are small
//! wrappers that start this program with `install`, `repair` or `rescue`; it also answers to them
//! as its own name.

mod common;
mod install;
mod repair;
mod rescue;

use common::Out;

const USAGE: &str = "\
asimaos-engine install [ARGS]    the installer (asimaos-install)
asimaos-engine repair [ARGS]     \"Repair this computer\" (asimaos-repair)
asimaos-engine rescue ACTION     Asima Rescue's actions (asimaos-rescue-engine)";

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let own = args.first().and_then(|a| a.rsplit('/').next()).unwrap_or("").to_string();
    let sub = match own.as_str() {
        "asimaos-install" => "install".to_string(),
        "asimaos-repair" => "repair".to_string(),
        "asimaos-rescue-engine" => "rescue".to_string(),
        _ if args.len() > 1 => args.remove(1),
        _ => String::new(),
    };
    let rest = &args[1..];
    // The installer and repair write files into a new system: /etc must stay world-readable.
    // Rescue writes only its own working files (a key, the reset plan): root's alone.
    let (name, mask, fancy) = match sub.as_str() {
        "install" => ("asimaos-install: ERROR", 0o022, true),
        "repair" => ("asimaos-repair", 0o022, false),
        "rescue" => ("asimaos-rescue-engine", 0o077, false),
        "-h" | "--help" => {
            println!("{USAGE}");
            return;
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    // SAFETY: umask has no preconditions.
    unsafe { libc::umask(mask) };
    let mut out = Out { json: rest.iter().any(|a| a == "--json"), fancy };
    let result = match sub.as_str() {
        "install" => install::main(rest, &mut out),
        "repair" => repair::main(rest, &mut out),
        _ => rescue::main(rest, &mut out),
    };
    if let Err(e) = result {
        out.error(name, &format!("{e:#}"));
        std::process::exit(1);
    }
}
