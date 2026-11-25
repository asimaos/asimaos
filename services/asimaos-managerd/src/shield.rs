//! Asima Shield: the only firewall on Asima OS (nftables). Four zones, one for each network (D55),
//! and a VPN kill-switch.
//!
//!   public     nothing can connect in (cafés, airports, unknown Wi-Fi)
//!   home       + KDE Connect, mDNS, game streaming (Steam Remote Play, Moonlight)
//!   developer  + local dev servers (3000–9000) and traffic from VM / container bridges
//!   open       not secure: nothing is stopped (the owner's choice, for networks a person trusts
//!              completely or for testing)
//!
//! Each network (a NetworkManager connection, Wi-Fi, cable or VPN) keeps the zone chosen while it
//! was in use; the others get the zone for other networks. The rules follow the interfaces: what
//! arrives on an interface gets the zone of its network. They are applied at every start by
//! asimaos-shield.service and again whenever a network is joined or left (NetworkManager's
//! dispatcher, 90-asimaos-shield). Connection attempts the rules stop are counted; the count
//! carries over each new set of rules, from the start of the computer.
//!
//! The choice lives in /etc/asimaos/shield.conf (written through /etc, never into the overlay's
//! upper directory).

use anyhow::{bail, ensure, Result};
use asimaos_svc::{cmd, Root};
use serde_json::{json, Value};

pub const CONF: &str = "/etc/asimaos/shield.conf";
pub const ZONES: [&str; 4] = ["public", "home", "developer", "open"];
const TABLE: &str = "asimaos_shield";
/// When the rules were last applied (seconds since 1970), for "stopped since".
const APPLIED: &str = "/run/asimaos/shield-applied";

/// A network with its own zone.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub uuid: String,
    pub zone: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shield {
    /// The zone for networks without their own.
    pub zone: String,
    pub kill_switch: bool,
    pub networks: Vec<Network>,
}

impl Default for Shield {
    fn default() -> Self {
        // The default for every Asima OS: a home network, no kill switch.
        Shield { zone: "home".into(), kill_switch: false, networks: Vec::new() }
    }
}

impl Shield {
    /// The zone of a network: its own, or the one for other networks.
    pub fn zone_of(&self, uuid: &str) -> &str {
        self.networks.iter().find(|n| n.uuid == uuid).map_or(self.zone.as_str(), |n| n.zone.as_str())
    }

    /// Remembers `zone` for a network (or forgets it, with None).
    pub fn set_network(&mut self, uuid: &str, name: &str, zone: Option<&str>) -> Result<()> {
        ensure!(valid_uuid(uuid), "That isn't a network's ID");
        self.networks.retain(|n| n.uuid != uuid);
        if let Some(z) = zone {
            ensure!(ZONES.contains(&z), "unknown zone");
            self.networks.push(Network { uuid: uuid.into(), zone: z.into(), name: clean_name(name) });
        }
        Ok(())
    }
}

fn valid_uuid(u: &str) -> bool {
    !u.is_empty() && u.len() <= 64 && u.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// A network's name as it is kept in the file: one line, no control characters.
fn clean_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_control()).take(120).collect()
}

/// An interface name nftables may quote: what the kernel allows, without quotes or backslashes.
fn valid_device(d: &str) -> bool {
    !d.is_empty() && d.len() <= 15 && d.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@'))
}

pub fn load(root: &Root) -> Shield {
    let mut s = Shield::default();
    if let Ok(text) = std::fs::read_to_string(root.path(CONF)) {
        for line in text.lines() {
            match line.split_once('=') {
                Some(("ZONE", z)) if ZONES.contains(&z.trim()) => s.zone = z.trim().to_string(),
                Some(("KILLSWITCH", v)) => s.kill_switch = v.trim() == "yes",
                Some(("NETWORK", v)) => {
                    let mut it = v.trim().splitn(3, ' ');
                    if let (Some(uuid), Some(zone)) = (it.next(), it.next()) {
                        let _ = s.set_network(uuid, it.next().unwrap_or_default(), Some(zone));
                    }
                }
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
    let mut text = format!(
        "# Asima Shield, written by Asima Manager. ZONE is for networks without their own.\nZONE={}\nKILLSWITCH={}\n",
        s.zone,
        if s.kill_switch { "yes" } else { "no" }
    );
    for n in &s.networks {
        text += &format!("NETWORK={} {} {}\n", n.uuid, n.zone, n.name);
    }
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// A network in use now, as NetworkManager has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Active {
    pub uuid: String,
    pub name: String,
    /// wifi, cable, vpn or other
    pub kind: String,
    pub device: String,
}

/// `nmcli -t` output: fields split at colons that aren't escaped.
fn nm_fields(line: &str) -> Vec<String> {
    let (mut fields, mut cur, mut esc) = (Vec::new(), String::new(), false);
    for c in line.chars() {
        match (esc, c) {
            (true, _) => {
                cur.push(c);
                esc = false;
            }
            (false, '\\') => esc = true,
            (false, ':') => fields.push(std::mem::take(&mut cur)),
            (false, _) => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// The networks NetworkManager has up now; none while it isn't running (early in the start).
pub fn parse_active(text: &str) -> Vec<Active> {
    text.lines()
        .filter_map(|l| {
            let f = nm_fields(l);
            let (uuid, name, kind, device) = (f.first()?, f.get(1)?, f.get(2)?, f.get(3)?);
            let kind = match kind.as_str() {
                "802-11-wireless" => "wifi",
                "802-3-ethernet" => "cable",
                "vpn" | "wireguard" | "tun" => "vpn",
                "loopback" | "bridge" => return None,
                _ => "other",
            };
            (valid_uuid(uuid) && valid_device(device)).then(|| Active {
                uuid: uuid.clone(),
                name: name.clone(),
                kind: kind.into(),
                device: device.clone(),
            })
        })
        .collect()
}

pub async fn active() -> Vec<Active> {
    cmd::probe("nmcli", &["-t", "-e", "yes", "-f", "UUID,NAME,TYPE,DEVICE", "connection", "show", "--active"])
        .await
        .map(|t| parse_active(&t))
        .unwrap_or_default()
}

/// How many connection attempts the rules in place have stopped.
pub async fn stopped() -> Option<u64> {
    let out = cmd::probe("nft", &["-j", "list", "counter", "inet", TABLE, "stopped"]).await?;
    let v: Value = serde_json::from_str(&out).ok()?;
    v["nftables"].as_array()?.iter().find_map(|o| o["counter"]["packets"].as_u64())
}

/// The complete nftables ruleset for a choice and the networks in use. Replacing the table is
/// atomic: `nft -f` applies the whole file as one transaction, so there is never a moment without
/// a firewall. `count` carries the stopped connection attempts over.
pub fn ruleset(s: &Shield, active: &[Active], count: u64) -> Result<String> {
    ensure!(ZONES.contains(&s.zone.as_str()), "unknown zone");
    let common = [
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
    let home = [
        "icmp type echo-request limit rate 5/second accept",
        "udp dport 5353 accept",      // mDNS: find printers, phones
        "tcp dport 1714-1764 accept", // KDE Connect
        "udp dport 1714-1764 accept",
        "udp dport 27031-27036 accept", // Steam Remote Play
        "tcp dport { 27036, 27037 } accept",
        "tcp dport { 47984, 47989, 47990, 48010 } accept", // Moonlight / Sunshine
        "udp dport { 47998-48000, 48002, 48010 } accept",
    ];
    let developer = [
        "tcp dport 3000-9000 accept",                              // local dev servers
        "iifname { \"virbr*\", \"podman*\", \"docker*\" } accept", // VMs and containers
    ];
    let mut out = format!(
        "table inet {TABLE} {{ }}\ndelete table inet {TABLE}\ntable inet {TABLE} {{\n  counter stopped {{ packets {count} bytes 0 }}\n"
    );
    for (zone, rules) in [
        ("public", Vec::new()),
        ("home", home.to_vec()),
        ("developer", home.iter().chain(developer.iter()).copied().collect()),
        // Not secure: everything that arrives is let in.
        ("open", vec!["accept"]),
    ] {
        out.push_str(&format!("  chain zone_{zone} {{\n"));
        for rule in rules {
            out.push_str(&format!("    {rule}\n"));
        }
        // What is left is dropped by the input chain's policy; attempts meant for this computer
        // (not the network's broadcast chatter) are counted.
        if zone != "open" {
            out.push_str("    meta pkttype host counter name \"stopped\"\n");
        }
        out.push_str("  }\n");
    }
    out.push_str("  chain input {\n    type filter hook input priority filter; policy drop;\n");
    for rule in common {
        out.push_str(&format!("    {rule}\n"));
    }
    for a in active {
        out.push_str(&format!("    iifname \"{}\" goto zone_{}\n", a.device, s.zone_of(&a.uuid)));
    }
    out.push_str(&format!("    goto zone_{}\n  }}\n", s.zone));
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

/// The rules for the networks in use now.
pub async fn apply(s: &Shield) -> Result<()> {
    if !cmd::exists("nft") {
        bail!("The firewall tools (nftables) are missing");
    }
    let count = stopped().await.unwrap_or(0);
    let rules = ruleset(s, &active().await, count)?;
    cmd::run_with("nft", &["-f", "-"], Some(rules.as_bytes()), &[]).await?;
    // The first rules since the start: what is stopped is counted from now.
    if !std::path::Path::new(APPLIED).exists() {
        let _ = std::fs::create_dir_all("/run/asimaos");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = std::fs::write(APPLIED, format!("{now}\n"));
    }
    Ok(())
}

/// Shield for Asima Manager: the zone of each network in use, the remembered ones, the VPN and
/// what was stopped.
pub async fn status(root: &Root) -> Value {
    let s = load(root);
    let active = if root.is_system() { active().await } else { Vec::new() };
    let networks: Vec<Value> = active
        .iter()
        .map(|a| {
            json!({ "uuid": a.uuid, "name": a.name, "kind": a.kind, "device": a.device,
                    "zone": s.zone_of(&a.uuid), "own": s.networks.iter().any(|n| n.uuid == a.uuid) })
        })
        .collect();
    let remembered: Vec<Value> = s
        .networks
        .iter()
        .filter(|n| !active.iter().any(|a| a.uuid == n.uuid))
        .map(|n| json!({ "uuid": n.uuid, "name": n.name, "zone": n.zone }))
        .collect();
    let vpn: Vec<String> = active.iter().filter(|a| a.kind == "vpn").map(|a| a.name.clone()).collect();
    let count = if root.is_system() { stopped().await } else { None };
    let since = std::fs::read_to_string(APPLIED).ok().and_then(|t| t.trim().parse::<u64>().ok());
    json!({
        "zone": s.zone,
        "killSwitch": s.kill_switch,
        "available": cmd::exists("nft"),
        "networks": networks,
        "remembered": remembered,
        "vpn": vpn,
        "stopped": count,
        "since": since,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(device: &str, uuid: &str) -> Active {
        Active { uuid: uuid.into(), name: "x".into(), kind: "wifi".into(), device: device.into() }
    }

    #[test]
    fn zones() {
        let mut s = Shield { zone: "public".into(), ..Shield::default() };
        let public = ruleset(&s, &[], 0).unwrap();
        assert!(public.contains("policy drop") && public.contains("goto zone_public") && !public.contains("chain output"));
        assert!(public.contains("\"waydroid*\" } udp dport { 53, 67 } accept"));
        assert!(public.contains("chain zone_home {\n    icmp") && public.contains("counter stopped { packets 0 bytes 0 }"));
        s.zone = "home".into();
        s.kill_switch = true;
        s.set_network("1111-aaaa", "Café", Some("public")).unwrap();
        let rules = ruleset(&s, &[net("wlan0", "1111-aaaa"), net("enp3s0", "2222-bbbb")], 42).unwrap();
        assert!(rules.contains("iifname \"wlan0\" goto zone_public\n    iifname \"enp3s0\" goto zone_home\n    goto zone_home"));
        assert!(rules.contains("chain output") && rules.contains("51820") && rules.contains("packets 42"));
        let dev = ruleset(&Shield { zone: "developer".into(), ..Shield::default() }, &[], 0).unwrap();
        assert!(dev.contains("chain zone_developer {\n    icmp") && dev.contains("3000-9000"));
        let open = ruleset(&Shield { zone: "open".into(), ..Shield::default() }, &[], 0).unwrap();
        assert!(open.contains("chain zone_open {\n    accept\n  }") && open.contains("goto zone_open"));
        assert!(ruleset(&Shield { zone: "everything".into(), ..Shield::default() }, &[], 0).is_err());
        assert!(s.set_network("x;y", "n", Some("home")).is_err() && s.set_network("12ab", "n", Some("all")).is_err());
    }

    #[test]
    fn networks_from_nmcli() {
        let text = "7f1c-22:Home\\: 5 GHz:802-11-wireless:wlan0\n0a0b-1:lo:loopback:lo\n9d9d-3:Office VPN:vpn:tun0\n\
                    bad id:x:802-3-ethernet:eth0\n55aa-9:Cable:802-3-ethernet:enp3s0\n";
        let a = parse_active(text);
        assert_eq!(a.len(), 3);
        assert_eq!((a[0].name.as_str(), a[0].kind.as_str(), a[0].device.as_str()), ("Home: 5 GHz", "wifi", "wlan0"));
        assert_eq!((a[1].kind.as_str(), a[2].kind.as_str()), ("vpn", "cable"));
    }

    #[test]
    fn conf_round_trip() {
        let dir = std::env::temp_dir().join(format!("shield-{}", std::process::id()));
        let root = Root(dir.clone());
        let mut s = Shield { zone: "developer".into(), kill_switch: true, networks: Vec::new() };
        s.set_network("1111-aaaa", "Airport Wi-Fi\n", Some("public")).unwrap();
        save(&root, &s).unwrap();
        let back = load(&root);
        assert_eq!(back.networks[0].name, "Airport Wi-Fi");
        assert_eq!((back.zone_of("1111-aaaa"), back.zone_of("other")), ("public", "developer"));
        assert_eq!(back.kill_switch, s.kill_switch);
        let _ = std::fs::remove_dir_all(dir);
    }
}
