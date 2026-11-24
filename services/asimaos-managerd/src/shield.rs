//! Asima Shield: the only firewall on Asima OS (nftables). Three zones and a VPN kill-switch.
//!
//!   public     nothing can connect in (cafés, airports, unknown Wi-Fi)
//!   home       + KDE Connect, mDNS, game streaming (Steam Remote Play, Moonlight)
//!   developer  + local dev servers (3000–9000) and traffic from VM / container bridges
//!
//! The choice lives in /etc/asimaos/shield.conf (written through /etc, never into the overlay's
//! upper directory) and is applied at every start by asimaos-shield.service.

use anyhow::{bail, ensure, Result};
use asimaos_svc::{cmd, Root};

pub const CONF: &str = "/etc/asimaos/shield.conf";
pub const ZONES: [&str; 3] = ["public", "home", "developer"];
const TABLE: &str = "asimaos_shield";

#[derive(Debug, Clone, PartialEq)]
pub struct Shield {
    pub zone: String,
    pub kill_switch: bool,
}

impl Default for Shield {
    fn default() -> Self {
        // The default for every Asima OS: a home network, no kill switch.
        Shield { zone: "home".into(), kill_switch: false }
    }
}

pub fn load(root: &Root) -> Shield {
    let mut s = Shield::default();
    if let Ok(text) = std::fs::read_to_string(root.path(CONF)) {
        for line in text.lines() {
            match line.split_once('=') {
                Some(("ZONE", z)) if ZONES.contains(&z.trim()) => s.zone = z.trim().to_string(),
                Some(("KILLSWITCH", v)) => s.kill_switch = v.trim() == "yes",
                _ => {}
            }
        }
    }
    s
}

pub fn save(root: &Root, s: &Shield) -> Result<()> {
    let path = root.path(CONF);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(
        &tmp,
        format!(
            "# Asima Shield, written by Asima Manager.\nZONE={}\nKILLSWITCH={}\n",
            s.zone,
            if s.kill_switch { "yes" } else { "no" }
        ),
    )?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// The complete nftables ruleset for a choice. Replacing the table is atomic: `nft -f` applies
/// the whole file as one transaction, so there is never a moment without a firewall.
pub fn ruleset(s: &Shield) -> Result<String> {
    ensure!(ZONES.contains(&s.zone.as_str()), "unknown zone");
    let mut input = vec![
        "ct state established,related accept",
        "ct state invalid drop",
        "iifname \"lo\" accept",
        // IPv6 needs neighbour discovery; DHCP answers come before a connection exists.
        "meta l4proto ipv6-icmp accept",
        "udp dport 68 accept",
        "udp dport 546 accept",
        // Virtual machines, containers and Android apps (Waydroid) get their addresses and names
        // from this computer over their own virtual bridges, in every zone.
        "iifname { \"virbr*\", \"podman*\", \"waydroid*\" } udp dport { 53, 67 } accept",
        "iifname { \"virbr*\", \"podman*\", \"waydroid*\" } tcp dport 53 accept",
    ];
    if s.zone != "public" {
        input.extend([
            "icmp type echo-request limit rate 5/second accept",
            "udp dport 5353 accept",      // mDNS: find printers, phones
            "tcp dport 1714-1764 accept", // KDE Connect
            "udp dport 1714-1764 accept",
            "udp dport 27031-27036 accept", // Steam Remote Play
            "tcp dport { 27036, 27037 } accept",
            "tcp dport { 47984, 47989, 47990, 48010 } accept", // Moonlight / Sunshine
            "udp dport { 47998-48000, 48002, 48010 } accept",
        ]);
    }
    if s.zone == "developer" {
        input.extend([
            "tcp dport 3000-9000 accept",                              // local dev servers
            "iifname { \"virbr*\", \"podman*\", \"docker*\" } accept", // VMs and containers
        ]);
    }
    let mut out = format!(
        "table inet {TABLE} {{ }}\ndelete table inet {TABLE}\ntable inet {TABLE} {{\n  chain input {{\n    type filter hook input priority filter; policy drop;\n"
    );
    for rule in input {
        out.push_str(&format!("    {rule}\n"));
    }
    out.push_str("  }\n");
    if s.kill_switch {
        // Nothing leaves except through the VPN: loopback, VPN interfaces, the VPN's own packets
        // (WireGuard marks them 51820) and DHCP to keep the link up.
        out.push_str(concat!(
            "  chain output {\n    type filter hook output priority filter; policy drop;\n",
            "    oifname \"lo\" accept\n",
            "    oifname { \"wg*\", \"tun*\", \"tailscale*\" } accept\n",
            "    meta mark 51820 accept\n",
            "    udp dport { 67, 547 } accept\n",
            "  }\n",
        ));
    }
    out.push_str("}\n");
    Ok(out)
}

pub async fn apply(s: &Shield) -> Result<()> {
    if !cmd::exists("nft") {
        bail!("The firewall tools (nftables) are missing");
    }
    cmd::run_with("nft", &["-f", "-"], Some(ruleset(s)?.as_bytes()), &[]).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones() {
        let public = ruleset(&Shield { zone: "public".into(), kill_switch: false }).unwrap();
        assert!(public.contains("policy drop") && !public.contains("1714") && !public.contains("chain output"));
        assert!(public.contains("\"waydroid*\" } udp dport { 53, 67 } accept"));
        let home = ruleset(&Shield { zone: "home".into(), kill_switch: false }).unwrap();
        assert!(home.contains("1714-1764") && !home.contains("3000-9000"));
        let dev = ruleset(&Shield { zone: "developer".into(), kill_switch: true }).unwrap();
        assert!(dev.contains("3000-9000") && dev.contains("chain output") && dev.contains("51820"));
        assert!(ruleset(&Shield { zone: "open".into(), kill_switch: false }).is_err());
    }

    #[test]
    fn conf_round_trip() {
        let dir = std::env::temp_dir().join(format!("shield-{}", std::process::id()));
        let root = Root(dir.clone());
        let s = Shield { zone: "developer".into(), kill_switch: true };
        save(&root, &s).unwrap();
        assert_eq!(load(&root), s);
        let _ = std::fs::remove_dir_all(dir);
    }
}
