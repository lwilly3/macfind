//! Balayage actif d'un sous-réseau auquel ce Mac est directement relié.
//!
//! Un petit datagramme UDP est envoyé à chaque adresse : pour l'acheminer, le
//! noyau doit d'abord résoudre l'adresse par ARP. Tout équipement présent
//! répond à cette requête, même s'il bloque le ping. Il suffit ensuite de
//! lire la table ARP du Mac. Aucun droit administrateur n'est nécessaire.

use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, UdpSocket};
use std::process::Command;
use std::time::Duration;

/// Au-delà, le balayage est refusé (un /22 fait 1022 adresses).
const MAX_HOSTS: usize = 1024;

/// Une interface IPv4 du Mac.
#[derive(Debug, Clone, PartialEq)]
pub struct Iface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub mask: Ipv4Addr,
}

impl Iface {
    fn network(&self) -> u32 {
        u32::from(self.ip) & u32::from(self.mask)
    }

    fn broadcast(&self) -> u32 {
        self.network() | !u32::from(self.mask)
    }

    fn prefix_len(&self) -> u32 {
        u32::from(self.mask).count_ones()
    }

    fn describe(&self) -> String {
        format!("{} {}/{}", self.name, self.ip, self.prefix_len())
    }
}

/// Un équipement qui a répondu.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub ip: Ipv4Addr,
    /// MAC au format AA:BB:CC:DD:EE:FF.
    pub mac: String,
    pub iface: String,
    /// Vrai pour l'adresse du Mac lui-même.
    pub own: bool,
}

/// Interfaces IPv4 utilisables : ni boucle locale, ni adresse auto-attribuée
/// (169.254.x.x), ni liaison point à point (VPN).
pub fn local_ifaces() -> Result<Vec<Iface>> {
    let all = if_addrs::get_if_addrs().context("lecture des interfaces réseau impossible")?;
    Ok(all
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(Iface {
                name: i.name,
                ip: v4.ip,
                mask: v4.netmask,
            }),
            _ => None,
        })
        .filter(|i| !i.ip.is_link_local() && i.prefix_len() < 31)
        .collect())
}

/// Lit une cible : `192.168.1.` (tout le /24), `192.168.1.0/24` ou une IP seule.
/// Renvoie la première et la dernière adresse de la plage.
fn parse_target(target: &str) -> Result<(u32, u32)> {
    let t = target.trim();
    let invalid = || {
        anyhow::anyhow!(
            "Cible « {t} » invalide. Exemples : 192.168.1.   192.168.1.0/24   192.168.1.20"
        )
    };
    if let Some(prefix) = t.strip_suffix('.') {
        if prefix.split('.').count() != 3 {
            return Err(invalid());
        }
        let base: Ipv4Addr = format!("{prefix}.0").parse().map_err(|_| invalid())?;
        let base = u32::from(base);
        return Ok((base, base | 0xff));
    }
    if let Some((addr, len)) = t.split_once('/') {
        let addr: Ipv4Addr = addr.parse().map_err(|_| invalid())?;
        let len: u32 = len.parse().map_err(|_| invalid())?;
        if len > 32 {
            return Err(invalid());
        }
        let mask = if len == 0 { 0 } else { u32::MAX << (32 - len) };
        let base = u32::from(addr) & mask;
        return Ok((base, base | !mask));
    }
    let addr: Ipv4Addr = t.parse().map_err(|_| invalid())?;
    Ok((u32::from(addr), u32::from(addr)))
}

/// Choisit l'interface et la liste des adresses à balayer.
/// Sans cible, c'est tout le sous-réseau de l'unique interface du Mac.
pub fn plan(target: Option<&str>, ifaces: &[Iface]) -> Result<(Iface, Vec<Ipv4Addr>)> {
    let (iface, first, last) = match target {
        Some(t) => {
            let (first, last) = parse_target(t)?;
            let Some(iface) = ifaces
                .iter()
                .find(|i| i.network() <= first && last <= i.broadcast())
            else {
                let local = if ifaces.is_empty() {
                    "aucun".to_string()
                } else {
                    let names: Vec<_> = ifaces.iter().map(Iface::describe).collect();
                    names.join(", ")
                };
                bail!(
                    "{t} n'est pas un sous-réseau auquel ce Mac est relié (réseaux locaux : {local}).\n\
                     Le balayage ne fonctionne que sur le segment local. Pour un autre sous-réseau,\n\
                     `macfind {t}` liste ce que le routeur connaît déjà."
                );
            };
            (iface, first, last)
        }
        None => match ifaces {
            [] => bail!("Ce Mac n'est relié à aucun réseau IPv4."),
            [only] => (only, only.network(), only.broadcast()),
            several => {
                let lines: Vec<_> = several
                    .iter()
                    .map(|i| format!("  {}", i.describe()))
                    .collect();
                bail!(
                    "Ce Mac est relié à plusieurs réseaux :\n{}\nPrécise lequel, par exemple : macfind scan 192.168.1.",
                    lines.join("\n")
                );
            }
        },
    };

    // L'adresse du réseau et celle de diffusion ne désignent aucun équipement.
    let hosts: Vec<Ipv4Addr> = (first..=last)
        .filter(|a| first == last || (*a != iface.network() && *a != iface.broadcast()))
        .map(Ipv4Addr::from)
        .collect();
    if hosts.len() > MAX_HOSTS {
        bail!(
            "Plage trop grande ({} adresses, maximum {MAX_HOSTS}). Précise un sous-réseau, par exemple : macfind scan 192.168.1.",
            hosts.len()
        );
    }
    Ok((iface.clone(), hosts))
}

/// Provoque une résolution ARP pour chaque adresse.
fn stimulate(hosts: &[Ipv4Addr]) -> Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).context("ouverture UDP impossible")?;
    // Deux passes : la seconde rattrape les requêtes perdues ou les appareils
    // en veille qui répondent avec retard.
    for _ in 0..2 {
        for (n, host) in hosts.iter().enumerate() {
            // Port 9 (« discard ») ; l'erreur éventuelle est sans importance,
            // seule compte la requête ARP émise par le noyau.
            let _ = socket.send_to(&[0], (*host, 9));
            if n % 32 == 31 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        std::thread::sleep(Duration::from_millis(900));
    }
    Ok(())
}

/// Remet une MAC de `arp` ou `ifconfig` au format AA:BB:CC:DD:EE:FF.
/// macOS omet les zéros de tête : `4:f4:1c:a:b:c`.
fn pad_mac(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = Vec::with_capacity(6);
    for p in parts {
        if p.is_empty() || p.len() > 2 || !p.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        out.push(format!("{p:0>2}").to_ascii_uppercase());
    }
    Some(out.join(":"))
}

/// Lit une ligne de `arp -an` :
/// `? (192.168.1.1) at 4:f4:1c:a:b:c on en0 ifscope [ethernet]`
/// Les entrées incomplètes, de diffusion et de multidiffusion sont ignorées.
fn parse_arp_line(line: &str) -> Option<(Ipv4Addr, String, String)> {
    let ip = line.split_once('(')?.1.split_once(')')?.0.parse().ok()?;
    let mut after_at = line.split_once(" at ")?.1.split_whitespace();
    let mac = pad_mac(after_at.next()?)?;
    let iface = line.split_once(" on ")?.1.split_whitespace().next()?;
    let first_byte = u8::from_str_radix(&mac[..2], 16).ok()?;
    if first_byte & 1 == 1 {
        return None;
    }
    Some((ip, mac, iface.to_string()))
}

fn arp_table() -> Result<Vec<(Ipv4Addr, String, String)>> {
    let out = Command::new("/usr/sbin/arp")
        .arg("-an")
        .output()
        .context("lancement de /usr/sbin/arp impossible")?;
    if !out.status.success() {
        bail!("`arp -an` a échoué");
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(parse_arp_line)
        .collect())
}

/// MAC de l'interface, lue dans `ifconfig` (ligne `ether …`).
fn own_mac(iface: &str) -> Option<String> {
    let out = Command::new("/sbin/ifconfig").arg(iface).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix("ether "))
        .and_then(|m| pad_mac(m.trim()))
}

/// Balaie les adresses et renvoie les équipements trouvés, triés par IP.
pub fn run(iface: &Iface, hosts: &[Ipv4Addr]) -> Result<Vec<Found>> {
    stimulate(hosts)?;
    let mut found: Vec<Found> = arp_table()?
        .into_iter()
        .filter(|(ip, _, name)| *name == iface.name && hosts.contains(ip))
        .map(|(ip, mac, name)| Found {
            ip,
            mac,
            iface: name,
            own: false,
        })
        .collect();
    // Le Mac ne figure pas dans sa propre table ARP.
    if hosts.contains(&iface.ip) {
        if let Some(mac) = own_mac(&iface.name) {
            found.push(Found {
                ip: iface.ip,
                mac,
                iface: iface.name.clone(),
                own: true,
            });
        }
    }
    found.sort_by_key(|f| f.ip);
    found.dedup_by_key(|f| f.ip);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, ip: &str, mask: &str) -> Iface {
        Iface {
            name: name.to_string(),
            ip: ip.parse().unwrap(),
            mask: mask.parse().unwrap(),
        }
    }

    #[test]
    fn parses_arp_lines() {
        assert_eq!(
            parse_arp_line("? (192.168.1.1) at 4:f4:1c:a:b:c on en0 ifscope [ethernet]"),
            Some((
                "192.168.1.1".parse().unwrap(),
                "04:F4:1C:0A:0B:0C".to_string(),
                "en0".to_string()
            ))
        );
        // Sans réponse, diffusion, multidiffusion.
        assert_eq!(
            parse_arp_line("? (192.168.1.3) at (incomplete) on en0 ifscope [ethernet]"),
            None
        );
        assert_eq!(
            parse_arp_line("? (192.168.1.255) at ff:ff:ff:ff:ff:ff on en0 ifscope [ethernet]"),
            None
        );
        assert_eq!(
            parse_arp_line("? (224.0.0.251) at 1:0:5e:0:0:fb on en0 ifscope permanent [ethernet]"),
            None
        );
    }

    #[test]
    fn pads_mac() {
        assert_eq!(
            pad_mac("0:15:5d:a:b:c").as_deref(),
            Some("00:15:5D:0A:0B:0C")
        );
        assert_eq!(pad_mac("(incomplete)"), None);
        assert_eq!(pad_mac("aa:bb:cc"), None);
    }

    #[test]
    fn plans_whole_subnet_without_target() {
        let en0 = iface("en0", "192.168.1.150", "255.255.255.0");
        let (chosen, hosts) = plan(None, std::slice::from_ref(&en0)).unwrap();
        assert_eq!(chosen, en0);
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(hosts[253], Ipv4Addr::new(192, 168, 1, 254));
    }

    #[test]
    fn plans_target_forms() {
        let ifaces = [
            iface("en0", "192.168.1.150", "255.255.255.0"),
            iface("en1", "10.0.0.5", "255.255.252.0"),
        ];
        let (chosen, hosts) = plan(Some("192.168.1."), &ifaces).unwrap();
        assert_eq!((chosen.name.as_str(), hosts.len()), ("en0", 254));

        // Un /24 pris dans un /22 : .0 et .255 sont ici des adresses ordinaires.
        let (chosen, hosts) = plan(Some("10.0.1.0/24"), &ifaces).unwrap();
        assert_eq!((chosen.name.as_str(), hosts.len()), ("en1", 256));

        let (_, hosts) = plan(Some("192.168.1.20"), &ifaces).unwrap();
        assert_eq!(hosts, vec![Ipv4Addr::new(192, 168, 1, 20)]);
    }

    #[test]
    fn refuses_what_it_cannot_scan() {
        let ifaces = [
            iface("en0", "192.168.1.150", "255.255.255.0"),
            iface("en1", "10.0.0.5", "255.255.0.0"),
        ];
        // Sous-réseau distant.
        let err = plan(Some("192.168.20."), &ifaces).unwrap_err().to_string();
        assert!(err.contains("n'est pas un sous-réseau"), "{err}");
        // Plusieurs réseaux sans cible.
        let err = plan(None, &ifaces).unwrap_err().to_string();
        assert!(err.contains("plusieurs réseaux"), "{err}");
        // Plage trop grande.
        let err = plan(Some("10.0.0.0/16"), &ifaces).unwrap_err().to_string();
        assert!(err.contains("trop grande"), "{err}");
        // Cibles mal formées.
        for bad in ["192.168.", "abc", "192.168.1.0/40", "192.168.1.20."] {
            assert!(plan(Some(bad), &ifaces).is_err(), "{bad}");
        }
        assert!(plan(None, &[]).is_err());
    }
}
