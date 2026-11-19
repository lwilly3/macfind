import AppKit
import UniformTypeIdentifiers

/// Export des lignes affichées vers un fichier CSV lisible par Excel.
/// Même format que `macfind --csv` : séparateur « ; », fins de ligne CRLF,
/// marque d'ordre d'octets pour les accents.
enum Export {
    static let enTete = [
        "Routeur", "Source", "IP", "MAC", "Fabricant", "Nom", "Hôte", "Port",
        "Serveur / Interface", "Statut", "Anomalie",
    ]

    /// Un champ CSV. Les noms d'hôte viennent des équipements du réseau : un
    /// champ commençant par `=`, `+`, `-` ou `@` serait exécuté comme formule
    /// par Excel, il est donc neutralisé par une apostrophe.
    static func champ(_ valeur: String) -> String {
        var v = valeur
        if let premier = v.first, "=+-@\t\r".contains(premier) {
            v = "'" + v
        }
        if v.contains(where: { ";\"\n\r".contains($0) }) {
            v = "\"" + v.replacingOccurrences(of: "\"", with: "\"\"") + "\""
        }
        return v
    }

    static func csv(_ lignes: [Equipement]) -> String {
        var sortie = "\u{feff}" + enTete.joined(separator: ";") + "\r\n"
        for e in lignes {
            let champs = [
                e.router, e.source, e.ip, e.mac, e.vendor, e.name, e.host, e.port, e.detail,
                e.status, e.anomaly,
            ]
            sortie += champs.map(champ).joined(separator: ";") + "\r\n"
        }
        return sortie
    }

    /// Nom proposé : `macfind-baux-dhcp-2026-11-19.csv`.
    static func nomDeFichier(source: Source, date: Date = Date()) -> String {
        let jour = date.formatted(.iso8601.year().month().day())
        let sujet = source.rawValue.lowercased().replacingOccurrences(of: " ", with: "-")
        return "macfind-\(sujet)-\(jour).csv"
    }

    /// Demande où enregistrer, puis écrit le fichier. Renvoie un message
    /// d'erreur si l'écriture échoue, `nil` sinon (y compris en cas d'annulation).
    @MainActor
    static func enregistrer(_ lignes: [Equipement], source: Source) -> String? {
        let panneau = NSSavePanel()
        panneau.allowedContentTypes = [.commaSeparatedText]
        panneau.nameFieldStringValue = nomDeFichier(source: source)
        panneau.title = "Exporter pour Excel"
        guard panneau.runModal() == .OK, let url = panneau.url else { return nil }
        do {
            try Data(csv(lignes).utf8).write(to: url, options: .atomic)
            NSWorkspace.shared.activateFileViewerSelecting([url])
            return nil
        } catch {
            return "Export impossible : \(error.localizedDescription)"
        }
    }
}
