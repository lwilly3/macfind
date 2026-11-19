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
    /// Anomalie relevée par macfind ; vide s'il n'y en a pas.
    let anomaly: String
    /// Serveur DHCP du bail ; vide hors DHCP.
    let server: String
    /// Sous-réseau /24 de l'adresse ; vide si elle est inconnue.
    let subnet: String
    /// Port du routeur où l'équipement est branché ; vide s'il est inconnu.
    let port: String

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
        let champs = [router, source, ip, mac, vendor, host, detail, status, anomaly, port]
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
        /// Absent des versions de macfind antérieures à la détection.
        let anomaly: String?
        let server: String?
        let subnet: String?
        let port: String?
    }

    /// Décode la sortie de `macfind --json`.
    static func decoder(_ json: Data) throws -> [Equipement] {
        try JSONDecoder().decode([Ligne].self, from: json).enumerated().map { rang, l in
            Equipement(
                id: rang, router: l.router, source: l.source, ip: l.ip, mac: l.mac,
                vendor: l.vendor, host: l.host, detail: l.detail, status: l.status,
                anomaly: l.anomaly ?? "", server: l.server ?? "", subnet: l.subnet ?? "",
                port: l.port ?? "")
        }
    }
}

/// Regroupement des lignes du tableau.
enum Regroupement: String, CaseIterable, Identifiable {
    case aucun = "Aucun"
    case sousReseau = "Sous-réseau"
    case serveur = "Serveur DHCP"

    var id: Self { self }
}

/// Un groupe de lignes et son titre.
struct Groupe: Identifiable, Equatable {
    let titre: String
    let lignes: [Equipement]

    var id: String { titre }

    /// Répartit des lignes déjà triées, sans changer leur ordre dans chaque
    /// groupe. Sous-réseaux dans l'ordre numérique, serveurs par nom ; les
    /// lignes sans valeur vont dans un dernier groupe.
    static func repartir(_ lignes: [Equipement], par critere: Regroupement) -> [Groupe] {
        let cle: (Equipement) -> String = { critere == .serveur ? $0.server : $0.subnet }
        let vide = critere == .serveur ? "Sans serveur DHCP" : "Adresse inconnue"
        let ordre: (String) -> (UInt32, String) = { valeur in
            if valeur.isEmpty { return (.max, "") }
            if critere == .serveur { return (0, valeur.lowercased()) }
            // « 192.168.1.0/24 » : trier sur l'adresse, pas sur le texte.
            return (Equipement.ipKey(String(valeur.prefix { $0 != "/" })), "")
        }
        return Dictionary(grouping: lignes, by: cle)
            .sorted { ordre($0.key) < ordre($1.key) }
            .map { valeur, membres in
                Groupe(
                    titre: "\(valeur.isEmpty ? vide : valeur) — \(membres.count)",
                    lignes: membres)
            }
    }
}
