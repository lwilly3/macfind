//! Noms donnés aux équipements par l'utilisateur, rangés par adresse MAC
//! dans `~/.config/macfind/names.toml`. Le routeur n'est jamais modifié.

use crate::mac;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Noms personnalisés ; la clé est la MAC normalisée (`AABBCCDDEEFF`).
#[derive(Debug, Default, PartialEq)]
pub struct Names(BTreeMap<String, String>);

pub fn path() -> PathBuf {
    crate::config::config_path().with_file_name("names.toml")
}

/// MAC complète, quel que soit son format ; erreur sinon.
fn key(mac_addr: &str) -> Result<String> {
    let k = mac::normalize(mac_addr);
    let only_mac_chars = mac_addr
        .chars()
        .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '-' | '.'));
    if k.len() != 12 || !only_mac_chars {
        bail!("Adresse MAC complète attendue (ex : 00:15:5D:00:00:01) : « {mac_addr} »");
    }
    Ok(k)
}

impl Names {
    /// Lit le contenu du fichier. Les clés sont acceptées dans tout format.
    pub fn from_toml(raw: &str) -> Result<Self> {
        let table: BTreeMap<String, String> = toml::from_str(raw)?;
        Ok(Self(
            table
                .into_iter()
                .map(|(k, v)| (mac::normalize(&k), v))
                .collect(),
        ))
    }

    /// Contenu du fichier : une ligne `"AA:BB:CC:DD:EE:FF" = "nom"` par équipement.
    pub fn to_toml(&self) -> String {
        let quoted = |v: &str| toml::Value::String(v.to_string()).to_string();
        let mut out = String::from("# Noms donnés aux équipements (macfind name <MAC> <nom>).\n");
        for (k, v) in &self.0 {
            out.push_str(&format!("{} = {}\n", quoted(&mac::pretty(k)), quoted(v)));
        }
        out
    }

    /// Nom donné à cette MAC ; vide s'il n'y en a pas.
    pub fn get(&self, mac_addr: &str) -> &str {
        self.0
            .get(&mac::normalize(mac_addr))
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn set(&mut self, mac_addr: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            bail!("Le nom est vide (pour l'effacer : macfind name <MAC> --clear)");
        }
        self.0.insert(key(mac_addr)?, name.to_string());
        Ok(())
    }

    /// Efface le nom ; renvoie faux s'il n'y en avait pas.
    pub fn remove(&mut self, mac_addr: &str) -> Result<bool> {
        Ok(self.0.remove(&key(mac_addr)?).is_some())
    }

    /// Paires (MAC affichable, nom), triées par MAC.
    pub fn entries(&self) -> impl Iterator<Item = (String, &str)> {
        self.0.iter().map(|(k, v)| (mac::pretty(k), v.as_str()))
    }
}

/// Noms enregistrés ; aucun si le fichier n'existe pas encore.
pub fn load() -> Result<Names> {
    let path = path();
    if !path.exists() {
        return Ok(Names::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Lecture impossible de {}", path.display()))?;
    Names::from_toml(&raw).with_context(|| format!("Fichier de noms invalide : {}", path.display()))
}

pub fn save(names: &Names) -> Result<()> {
    let path = path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, names.to_toml())
        .with_context(|| format!("Écriture impossible de {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove_in_any_mac_format() {
        let mut n = Names::default();
        n.set("00-15-5d-00-00-01", "  Caméra entrée ").unwrap();
        assert_eq!(n.get("00:15:5D:00:00:01"), "Caméra entrée");
        assert_eq!(n.get("0015.5d00.0001"), "Caméra entrée");
        assert_eq!(n.get("00:15:5D:00:00:02"), "");
        assert!(n.remove("00:15:5D:00:00:01").unwrap());
        assert!(!n.remove("00:15:5D:00:00:01").unwrap());
    }

    #[test]
    fn rejects_partial_mac_and_empty_name() {
        let mut n = Names::default();
        assert!(n.set("00:15:5D", "x").is_err());
        assert!(n.set("imprimante01", "x").is_err());
        assert!(n.set("00:15:5D:00:00:01", "  ").is_err());
    }

    #[test]
    fn file_round_trip() {
        let mut n = Names::default();
        n.set("00:15:5D:00:00:02", "Salle \"B\"").unwrap();
        n.set("00:15:5D:00:00:01", "Caméra entrée").unwrap();
        let raw = n.to_toml();
        assert!(raw.contains("\"00:15:5D:00:00:01\" = \"Caméra entrée\"\n"));
        assert_eq!(Names::from_toml(&raw).unwrap(), n);
        let listed: Vec<_> = n.entries().map(|(m, _)| m).collect();
        assert_eq!(listed, ["00:15:5D:00:00:01", "00:15:5D:00:00:02"]);
    }
}
