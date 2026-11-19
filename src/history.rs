//! Historique local : pour chaque MAC, les adresses IP successives et les
//! dates de première et de dernière vue. Rangé dans
//! `~/.config/macfind/history.json`, alimenté à chaque lecture des routeurs
//! ou balayage. Rien n'est envoyé ailleurs.

use crate::mac;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Nombre d'adresses gardées par équipement (les plus récentes).
const MAX_IPS: usize = 50;
const DAY: u64 = 86_400;

/// Une adresse IP occupée par un équipement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub ip: String,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// Ce qu'on sait d'un équipement. Les dates sont en secondes Unix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub first_seen: u64,
    pub last_seen: u64,
    /// Dernier nom d'hôte annoncé.
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub ips: Vec<Sighting>,
}

impl Device {
    /// Adresse la plus récemment vue ; vide si aucune n'est connue.
    pub fn last_ip(&self) -> &str {
        self.ips
            .iter()
            .max_by_key(|s| s.last_seen)
            .map(|s| s.ip.as_str())
            .unwrap_or("")
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// Date du premier relevé. Les équipements vus à ce moment-là forment
    /// l'état de départ et ne sont pas « nouveaux ».
    #[serde(default)]
    pub started: u64,
    /// Clé : MAC normalisée (`AABBCCDDEEFF`).
    #[serde(default)]
    pub devices: BTreeMap<String, Device>,
}

impl History {
    /// Note qu'une MAC a été vue maintenant, à cette adresse si elle est connue.
    pub fn record(&mut self, mac_addr: &str, ip: &str, host: &str, now: u64) {
        let key = mac::normalize(mac_addr);
        if key.len() != 12 {
            return;
        }
        if self.started == 0 {
            self.started = now;
        }
        let device = self.devices.entry(key).or_insert_with(|| Device {
            first_seen: now,
            last_seen: now,
            host: String::new(),
            ips: Vec::new(),
        });
        device.last_seen = now;
        if !host.is_empty() {
            device.host = host.to_string();
        }
        if ip.is_empty() {
            return;
        }
        match device.ips.iter_mut().find(|s| s.ip == ip) {
            Some(s) => s.last_seen = now,
            None => device.ips.push(Sighting {
                ip: ip.to_string(),
                first_seen: now,
                last_seen: now,
            }),
        }
        if device.ips.len() > MAX_IPS {
            device.ips.sort_by_key(|s| s.last_seen);
            let excess = device.ips.len() - MAX_IPS;
            device.ips.drain(..excess);
        }
    }

    pub fn get(&self, mac_addr: &str) -> Option<&Device> {
        self.devices.get(&mac::normalize(mac_addr))
    }

    /// Vrai si l'équipement est apparu après le premier relevé, depuis
    /// `days` jours au plus.
    pub fn is_new(&self, mac_addr: &str, now: u64, days: u64) -> bool {
        self.get(mac_addr).is_some_and(|d| {
            d.first_seen > self.started && now.saturating_sub(d.first_seen) <= days * DAY
        })
    }
}

pub fn path() -> PathBuf {
    crate::config::config_path().with_file_name("history.json")
}

/// Secondes Unix actuelles.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Durée écoulée, en clair : « à l'instant », « il y a 5 min », « il y a 3 j ».
pub fn ago(now: u64, then: u64) -> String {
    let s = now.saturating_sub(then);
    match s {
        0..=59 => "à l'instant".to_string(),
        60..=3_599 => format!("il y a {} min", s / 60),
        3_600..=86_399 => format!("il y a {} h", s / 3_600),
        _ => format!("il y a {} j", s / DAY),
    }
}

/// Historique enregistré ; vide si le fichier n'existe pas encore.
pub fn load() -> Result<History> {
    let path = path();
    if !path.exists() {
        return Ok(History::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Lecture impossible de {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("Historique invalide : {}", path.display()))
}

/// Écrit l'historique. Le fichier est remplacé d'un coup, pour ne jamais
/// laisser un fichier à moitié écrit.
pub fn save(history: &History) -> Result<()> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(history)?)
        .with_context(|| format!("Écriture impossible de {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("Écriture impossible de {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "00:15:5D:00:00:01";
    const B: &str = "00:15:5D:00:00:02";
    const T0: u64 = 1_000_000;

    #[test]
    fn records_first_and_last_seen() {
        let mut h = History::default();
        h.record(A, "192.0.2.10", "poste", T0);
        h.record("00-15-5d-00-00-01", "192.0.2.10", "", T0 + 60);
        let d = h.get(A).unwrap();
        assert_eq!((d.first_seen, d.last_seen), (T0, T0 + 60));
        // Un relevé sans nom d'hôte garde le dernier nom connu.
        assert_eq!(d.host, "poste");
        assert_eq!(d.ips.len(), 1);
        assert_eq!(d.ips[0].last_seen, T0 + 60);
    }

    #[test]
    fn keeps_successive_addresses() {
        let mut h = History::default();
        h.record(A, "192.0.2.10", "", T0);
        h.record(A, "192.0.2.25", "", T0 + DAY);
        h.record(A, "", "", T0 + 2 * DAY);
        let d = h.get(A).unwrap();
        let ips: Vec<_> = d.ips.iter().map(|s| s.ip.as_str()).collect();
        assert_eq!(ips, ["192.0.2.10", "192.0.2.25"]);
        assert_eq!(d.last_ip(), "192.0.2.25");
        assert_eq!(d.last_seen, T0 + 2 * DAY);
    }

    #[test]
    fn ignores_incomplete_mac() {
        let mut h = History::default();
        h.record("00:15:5D", "192.0.2.10", "", T0);
        h.record("", "192.0.2.10", "", T0);
        assert!(h.devices.is_empty());
        assert_eq!(h.started, 0);
    }

    #[test]
    fn new_devices_exclude_the_starting_state() {
        let mut h = History::default();
        h.record(A, "192.0.2.10", "", T0);
        h.record(B, "192.0.2.11", "", T0 + DAY);
        // A fait partie du premier relevé ; B est apparu ensuite.
        assert!(!h.is_new(A, T0 + DAY, 7));
        assert!(h.is_new(B, T0 + DAY, 7));
        assert!(h.is_new(B, T0 + 8 * DAY, 7));
        assert!(!h.is_new(B, T0 + 8 * DAY + 1, 7));
        assert!(!h.is_new("00:15:5D:00:00:09", T0, 7));
    }

    #[test]
    fn caps_the_number_of_addresses() {
        let mut h = History::default();
        for i in 0..(MAX_IPS as u64 + 5) {
            h.record(A, &format!("192.0.2.{i}"), "", T0 + i);
        }
        let d = h.get(A).unwrap();
        assert_eq!(d.ips.len(), MAX_IPS);
        // Les plus anciennes sont parties.
        assert!(d.ips.iter().all(|s| s.ip != "192.0.2.0"));
        assert_eq!(d.last_ip(), format!("192.0.2.{}", MAX_IPS + 4));
    }

    #[test]
    fn file_round_trip() {
        let mut h = History::default();
        h.record(A, "192.0.2.10", "poste", T0);
        let raw = serde_json::to_string(&h).unwrap();
        assert_eq!(serde_json::from_str::<History>(&raw).unwrap(), h);
        // Un fichier d'une version future avec des champs en moins reste lisible.
        assert_eq!(
            serde_json::from_str::<History>("{}").unwrap(),
            History::default()
        );
    }

    #[test]
    fn elapsed_time_in_words() {
        assert_eq!(ago(T0, T0), "à l'instant");
        assert_eq!(ago(T0 + 300, T0), "il y a 5 min");
        assert_eq!(ago(T0 + 7_200, T0), "il y a 2 h");
        assert_eq!(ago(T0 + 3 * DAY, T0), "il y a 3 j");
        // Une horloge reculée ne donne pas une durée négative.
        assert_eq!(ago(T0, T0 + 50), "à l'instant");
    }
}
