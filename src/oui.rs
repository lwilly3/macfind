//! Fabricant d'un équipement à partir de l'OUI (les 3 premiers octets de la MAC).
//!
//! La base vient du registre public de l'IEEE et elle est embarquée dans le
//! binaire : aucune requête réseau, aucune MAC envoyée à un tiers.
//! `scripts/maj-oui.sh` la régénère.

use crate::mac;
use std::sync::OnceLock;

/// Une ligne par préfixe, triée : 6 chiffres hexa en majuscules, tabulation, nom.
const DATA: &str = include_str!("../data/oui.tsv");

/// Libellé des adresses aléatoires (iPhone, Android, Windows récents).
pub const PRIVATE: &str = "(MAC privée)";

fn table() -> &'static [(&'static str, &'static str)] {
    static TABLE: OnceLock<Vec<(&str, &str)>> = OnceLock::new();
    TABLE.get_or_init(|| DATA.lines().filter_map(|l| l.split_once('\t')).collect())
}

/// Vrai si le bit « administrée localement » est levé : l'adresse n'a pas été
/// attribuée par un fabricant, elle a été tirée au hasard ou fixée à la main.
fn locally_administered(normalized: &str) -> bool {
    matches!(
        normalized.as_bytes().get(1),
        Some(b'2' | b'6' | b'A' | b'E')
    )
}

/// Fabricant de la MAC (tout format), ou chaîne vide s'il est inconnu.
pub fn vendor(mac_addr: &str) -> &'static str {
    let n = mac::normalize(mac_addr);
    let Some(prefix) = n.get(..6) else {
        return "";
    };
    let table = table();
    match table.binary_search_by(|(p, _)| (*p).cmp(prefix)) {
        Ok(i) => table[i].1,
        Err(_) if locally_administered(&n) => PRIVATE,
        Err(_) => "",
    }
}

/// Raccourcit un nom pour l'affichage en tableau.
pub fn short(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let cut: String = name.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_prefix_any_format() {
        assert_eq!(vendor("00:15:5D:00:00:01"), "Microsoft");
        assert_eq!(vendor("0015.5d00.0001"), "Microsoft");
        assert_eq!(vendor("00-15-5d"), "Microsoft");
    }

    #[test]
    fn unknown_or_incomplete() {
        assert_eq!(vendor(""), "");
        assert_eq!(vendor("00:15"), "");
        // FC:FF:FF n'est pas attribué et n'est pas une adresse locale.
        assert_eq!(vendor("FC:FF:FF:00:00:01"), "");
    }

    #[test]
    fn random_mac_is_flagged() {
        assert_eq!(vendor("DA:A1:19:00:00:01"), PRIVATE);
        assert_eq!(vendor("f2:00:00:00:00:01"), PRIVATE);
    }

    #[test]
    fn table_is_sorted_and_well_formed() {
        let t = table();
        assert_eq!(t.len(), DATA.lines().count(), "ligne sans tabulation");
        assert!(t.len() > 30_000);
        assert!(t.windows(2).all(|w| w[0].0 < w[1].0), "préfixes non triés");
        assert!(t.iter().all(|(p, name)| p.len() == 6
            && p.bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
            && !name.is_empty()));
    }

    #[test]
    fn shortens_long_names() {
        assert_eq!(short("Apple", 10), "Apple");
        assert_eq!(short("Hewlett Packard Enterprise", 10), "Hewlett P…");
    }
}
