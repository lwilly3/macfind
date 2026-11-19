//! Détection des anomalies à partir des baux DHCP et de la table ARP d'un
//! routeur. Fonctions pures : aucun accès réseau.

use crate::mac;
use crate::routeros::{ArpEntry, Lease};
use std::collections::{BTreeSet, HashMap};

fn lease_ip(l: &Lease) -> &str {
    l.active_address
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(l.address.as_deref())
        .unwrap_or("")
}

fn lease_mac(l: &Lease) -> String {
    mac::normalize(l.mac_address.as_deref().unwrap_or(""))
}

fn is_bound(l: &Lease) -> bool {
    l.status.as_deref() == Some("bound")
}

/// Une entrée ARP compte si elle porte une MAC et n'est pas marquée incomplète.
fn arp_usable(a: &ArpEntry) -> Option<(&str, String)> {
    let ip = a.address.as_deref().filter(|s| !s.is_empty())?;
    let m = mac::normalize(a.mac_address.as_deref().unwrap_or(""));
    (!m.is_empty() && a.complete.as_deref() != Some("false")).then_some((ip, m))
}

/// Ce qui a été relevé sur un routeur.
#[derive(Default)]
pub struct Report {
    /// (MAC, serveur DHCP) → nombre de baux actifs.
    bound: HashMap<(String, String), usize>,
    /// MAC → nombre total de baux actifs.
    bound_total: HashMap<String, usize>,
    /// IP → MAC distinctes qui l'utilisent (baux actifs et ARP).
    owners: HashMap<String, BTreeSet<String>>,
    /// MAC → adresses où elle répond (ARP) sans aucun bail correspondant,
    /// alors qu'elle détient par ailleurs des baux actifs.
    stray: HashMap<String, BTreeSet<String>>,
}

impl Report {
    pub fn new(leases: &[Lease], arp: &[ArpEntry]) -> Self {
        let mut r = Self::default();
        for l in leases.iter().filter(|l| is_bound(l)) {
            let m = lease_mac(l);
            if m.is_empty() {
                continue;
            }
            let server = l.server.clone().unwrap_or_default();
            *r.bound.entry((m.clone(), server)).or_default() += 1;
            *r.bound_total.entry(m.clone()).or_default() += 1;
            let ip = lease_ip(l);
            if !ip.is_empty() {
                r.owners.entry(ip.to_string()).or_default().insert(m);
            }
        }
        for (ip, m) in arp.iter().filter_map(arp_usable) {
            r.owners
                .entry(ip.to_string())
                .or_default()
                .insert(m.clone());
            let has_lease_here = leases.iter().any(|l| {
                lease_mac(l) == m
                    && (l.address.as_deref() == Some(ip) || l.active_address.as_deref() == Some(ip))
            });
            if r.bound_total.contains_key(&m) && !has_lease_here {
                r.stray.entry(m).or_default().insert(ip.to_string());
            }
        }
        r
    }

    /// « IP aussi utilisée par … » si une autre MAC occupe la même adresse.
    fn conflict(&self, ip: &str, m: &str) -> Option<String> {
        let others: Vec<String> = self
            .owners
            .get(ip)?
            .iter()
            .filter(|o| o.as_str() != m)
            .map(|o| mac::pretty(o))
            .collect();
        (!others.is_empty()).then(|| format!("IP aussi utilisée par {}", others.join(", ")))
    }

    /// Anomalies d'un bail ; chaîne vide s'il n'y en a pas.
    pub fn for_lease(&self, l: &Lease) -> String {
        if !is_bound(l) {
            return String::new();
        }
        let m = lease_mac(l);
        let server = l.server.clone().unwrap_or_default();
        let mut found = Vec::new();
        if let Some(n) = self
            .bound
            .get(&(m.clone(), server.clone()))
            .filter(|n| **n > 1)
        {
            found.push(format!("{n} baux actifs sur {server}"));
        }
        found.extend(self.conflict(lease_ip(l), &m));
        if let Some(ips) = self.stray.get(&m) {
            let ips: Vec<&str> = ips.iter().map(String::as_str).collect();
            found.push(format!("répond aussi en {} sans bail", ips.join(", ")));
        }
        found.join(" ; ")
    }

    /// Anomalies d'une entrée ARP ; chaîne vide s'il n'y en a pas.
    pub fn for_arp(&self, a: &ArpEntry) -> String {
        let Some((ip, m)) = arp_usable(a) else {
            return String::new();
        };
        let mut found = Vec::new();
        found.extend(self.conflict(ip, &m));
        if self.stray.get(&m).is_some_and(|ips| ips.contains(ip)) {
            let n = self.bound_total.get(&m).copied().unwrap_or(0);
            found.push(if n > 1 {
                format!("adresse hors de ses {n} baux actifs")
            } else {
                "adresse hors de son bail actif".to_string()
            });
        }
        found.join(" ; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Valeurs fictives uniquement.
    fn lease(ip: &str, m: &str, server: &str, status: &str) -> Lease {
        Lease {
            address: Some(ip.to_string()),
            mac_address: Some(m.to_string()),
            active_address: Some(ip.to_string()),
            host_name: None,
            server: Some(server.to_string()),
            status: Some(status.to_string()),
            last_seen: None,
            dynamic: Some("true".to_string()),
            comment: None,
        }
    }

    fn arp(ip: &str, m: &str) -> ArpEntry {
        ArpEntry {
            address: Some(ip.to_string()),
            mac_address: Some(m.to_string()),
            interface: Some("bridge".to_string()),
            complete: Some("true".to_string()),
            comment: None,
        }
    }

    const A: &str = "00:15:5D:00:00:01";
    const B: &str = "00:15:5D:00:00:02";

    #[test]
    fn nothing_on_a_healthy_network() {
        let leases = [
            lease("192.0.2.10", A, "dhcp1", "bound"),
            lease("192.0.2.11", B, "dhcp1", "bound"),
        ];
        let table = [arp("192.0.2.10", A), arp("192.0.2.11", B)];
        let r = Report::new(&leases, &table);
        assert!(leases.iter().all(|l| r.for_lease(l).is_empty()));
        assert!(table.iter().all(|a| r.for_arp(a).is_empty()));
    }

    #[test]
    fn several_bound_leases_for_one_mac() {
        let leases = [
            lease("192.0.2.10", A, "dhcp1", "bound"),
            lease("192.0.2.11", A, "dhcp1", "bound"),
            lease("192.0.2.12", A, "dhcp1", "waiting"),
            lease("192.0.2.13", B, "dhcp1", "bound"),
        ];
        let r = Report::new(&leases, &[]);
        assert_eq!(r.for_lease(&leases[0]), "2 baux actifs sur dhcp1");
        // Un bail en attente n'est ni compté ni signalé.
        assert_eq!(r.for_lease(&leases[2]), "");
        assert_eq!(r.for_lease(&leases[3]), "");
    }

    #[test]
    fn roaming_between_servers_is_not_flagged() {
        let leases = [
            lease("192.0.2.10", A, "dhcp1", "bound"),
            lease("198.51.100.10", A, "dhcp2", "bound"),
        ];
        let r = Report::new(&leases, &[]);
        assert!(leases.iter().all(|l| r.for_lease(l).is_empty()));
    }

    #[test]
    fn same_ip_for_two_macs() {
        let leases = [lease("192.0.2.10", A, "dhcp1", "bound")];
        let table = [arp("192.0.2.10", B)];
        let r = Report::new(&leases, &table);
        assert_eq!(
            r.for_lease(&leases[0]),
            format!("IP aussi utilisée par {B}")
        );
        assert_eq!(r.for_arp(&table[0]), format!("IP aussi utilisée par {A}"));
    }

    #[test]
    fn device_answering_outside_its_leases() {
        // IP fixe en .25 et un bail dynamique en .118 pour la même machine.
        let leases = [lease("192.0.2.118", A, "dhcp1", "bound")];
        let table = [arp("192.0.2.25", A), arp("192.0.2.118", A)];
        let r = Report::new(&leases, &table);
        assert_eq!(
            r.for_lease(&leases[0]),
            "répond aussi en 192.0.2.25 sans bail"
        );
        assert_eq!(r.for_arp(&table[0]), "adresse hors de son bail actif");
        assert_eq!(r.for_arp(&table[1]), "");
    }

    #[test]
    fn fixed_ip_without_any_lease_is_normal() {
        let r = Report::new(&[], &[arp("192.0.2.25", A)]);
        assert_eq!(r.for_arp(&arp("192.0.2.25", A)), "");
    }

    #[test]
    fn incomplete_arp_entries_are_ignored() {
        let leases = [lease("192.0.2.10", A, "dhcp1", "bound")];
        let mut ghost = arp("192.0.2.10", B);
        ghost.complete = Some("false".to_string());
        let r = Report::new(&leases, &[ghost]);
        assert_eq!(r.for_lease(&leases[0]), "");
    }
}
