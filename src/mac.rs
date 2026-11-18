/// Ne garde que les chiffres hexadécimaux, en majuscules.
/// "aa-bb-cc-dd-ee-ff", "aabb.ccdd.eeff", "AA:BB:CC:DD:EE:FF" -> "AABBCCDDEEFF"
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Vrai si la requête ressemble à un (morceau de) MAC :
/// uniquement des chiffres hexa et des séparateurs, au moins 4 chiffres hexa.
pub fn looks_like_mac(query: &str) -> bool {
    let only_mac_chars = query
        .chars()
        .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '-' | '.'));
    let hex_count = query.chars().filter(|c| c.is_ascii_hexdigit()).count();
    // Exclut les IP du type 10.0.1.25 (que des chiffres décimaux + points)
    let is_ip_like = query.chars().all(|c| c.is_ascii_digit() || c == '.') && query.contains('.');
    only_mac_chars && hex_count >= 4 && !is_ip_like
}

/// Formate une MAC normalisée en AA:BB:CC:DD:EE:FF.
pub fn pretty(s: &str) -> String {
    let n = normalize(s);
    if n.len() != 12 {
        return s.to_string();
    }
    n.as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_formats() {
        assert_eq!(normalize("aa-bb-cc-dd-ee-ff"), "AABBCCDDEEFF");
        assert_eq!(normalize("aabb.ccdd.eeff"), "AABBCCDDEEFF");
        assert_eq!(normalize("AA:BB:CC:DD:EE:FF"), "AABBCCDDEEFF");
    }

    #[test]
    fn detects_mac_queries() {
        assert!(looks_like_mac("dd:ee:ff"));
        assert!(looks_like_mac("aabbcc"));
        assert!(looks_like_mac("AA-BB"));
        assert!(!looks_like_mac("192.168.1.10"));
        assert!(!looks_like_mac("imprimante"));
        assert!(!looks_like_mac("ab"));
    }

    #[test]
    fn pretty_prints() {
        assert_eq!(pretty("aabb.ccdd.eeff"), "AA:BB:CC:DD:EE:FF");
    }
}
