import Foundation

/// Une ligne renvoyée par `macfind --json` (structure `Hit` côté Rust).
struct Equipement: Identifiable, Equatable {
    /// Rang dans la réponse : sert d'identité stable pour le tableau.
    let id: Int
    let router: String
    let source: String
    let ip: String
    let mac: String
    let vendor: String
    let host: String
    let detail: String
    let status: String

    /// Clé de tri numérique : sans elle, « 192.168.1.10 » passerait avant
    /// « 192.168.1.9 ».
    var ipKey: UInt32 { Equipement.ipKey(ip) }

    static func ipKey(_ ip: String) -> UInt32 {
        // Les segments vides sont gardés : « 192.168.1.1. » n'est pas une adresse.
        let segments = ip.split(separator: ".", omittingEmptySubsequences: false)
        let octets = segments.compactMap { UInt32($0) }
        guard segments.count == 4, octets.count == 4, octets.allSatisfy({ $0 <= 255 }) else {
            return .max
        }
        return octets.reduce(0) { $0 << 8 | $1 }
    }

    /// Filtre du champ de recherche : IP complète exacte, texte présent dans
    /// n'importe quelle colonne, ou fragment de MAC saisi dans un autre format
    /// (`aabb.ccdd`, `aa-bb-cc`).
    func correspond(a recherche: String) -> Bool {
        let q = recherche.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return true }
        // Une IP complète désigne une seule adresse : « 192.168.1.12 » ne doit
        // pas ramener « 192.168.1.120 ».
        if Equipement.ipKey(q) != .max { return ip == q }
        let champs = [router, source, ip, mac, vendor, host, detail, status]
        if champs.contains(where: { $0.lowercased().contains(q) }) { return true }
        let hexa = q.filter { !":-. ".contains($0) }
        let estMac = hexa.count >= 4 && hexa.allSatisfy(\.isHexDigit) && hexa.count != q.count
        return estMac && mac.lowercased().filter { $0 != ":" }.contains(hexa)
    }
}

extension Equipement {
    /// Champs du JSON, sans l'identité locale.
    private struct Ligne: Decodable {
        let router, source, ip, mac, vendor, host, detail, status: String
    }

    /// Décode la sortie de `macfind --json`.
    static func decoder(_ json: Data) throws -> [Equipement] {
        try JSONDecoder().decode([Ligne].self, from: json).enumerated().map { rang, l in
            Equipement(
                id: rang, router: l.router, source: l.source, ip: l.ip, mac: l.mac,
                vendor: l.vendor, host: l.host, detail: l.detail, status: l.status)
        }
    }
}
