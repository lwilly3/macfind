import Foundation
import Observation

/// D'où viennent les lignes affichées.
enum Source: String, CaseIterable, Identifiable {
    case baux = "Baux DHCP"
    case balayage = "Balayage local"
    case anomalies = "Anomalies"
    case nouveaux = "Nouveaux"

    var id: Self { self }

    var arguments: [String] {
        switch self {
        case .baux: ["leases"]
        case .balayage: ["scan"]
        case .anomalies: ["anomalies"]
        case .nouveaux: ["new"]
        }
    }
}

/// État de la fenêtre principale.
@MainActor
@Observable
final class Inventaire {
    var source: Source = .baux
    var recherche = ""
    private(set) var equipements: [Equipement] = []
    private(set) var avertissements: [String] = []
    private(set) var erreur: String?
    private(set) var enCours = false
    private(set) var derniereMaj: Date?

    var visibles: [Equipement] {
        equipements.filter { $0.correspond(a: recherche) }
    }

    /// Les baux et les anomalies ont besoin d'un routeur configuré ; le
    /// balayage et l'historique, non.
    var configurationManquante: Bool {
        (source == .baux || source == .anomalies)
            && !FileManager.default.fileExists(atPath: MacfindCLI.configuration.path)
    }

    func charger() async {
        guard !enCours else { return }
        erreur = nil
        avertissements = []
        if configurationManquante {
            equipements = []
            return
        }
        enCours = true
        defer { enCours = false }
        let demandee = source
        do {
            let reponse = try await MacfindCLI.interroger(demandee.arguments)
            // La source a pu changer pendant l'attente : la réponse est périmée.
            guard demandee == source else { return }
            equipements = reponse.equipements
            avertissements = reponse.avertissements
            derniereMaj = Date()
        } catch {
            guard demandee == source else { return }
            equipements = []
            erreur = error.localizedDescription
        }
    }
}
