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
    /// Vrai dès le lancement : le premier chargement part aussitôt.
    private(set) var enCours = true
    private(set) var derniereMaj: Date?

    var visibles: [Equipement] {
        equipements.filter { $0.correspond(a: recherche) }
    }

    /// Routeurs configurés, relus à chaque chargement.
    private(set) var routeurs: [Routeur] = []
    /// Rang du dernier chargement lancé : une réponse plus ancienne est ignorée.
    private var rang = 0
    /// Onglet dont viennent les lignes affichées.
    private var sourceAffichee: Source?

    /// Les baux et les anomalies ont besoin d'un routeur configuré ; le
    /// balayage et l'historique, non.
    var configurationManquante: Bool {
        (source == .baux || source == .anomalies) && routeurs.isEmpty
    }

    func charger() async {
        rang += 1
        let monRang = rang
        let demandee = source
        erreur = nil
        avertissements = []
        // Les lignes d'un autre onglet n'ont pas leur place dans celui-ci.
        if sourceAffichee != demandee {
            equipements = []
            sourceAffichee = demandee
        }
        enCours = true
        let liste = (try? await MacfindCLI.routeurs()) ?? []
        // Un chargement plus récent a pris le relais (changement d'onglet,
        // actualisation) : il s'occupe de la suite.
        guard monRang == rang else { return }
        routeurs = liste
        if configurationManquante {
            equipements = []
            enCours = false
            return
        }
        do {
            let reponse = try await MacfindCLI.interroger(demandee.arguments)
            guard monRang == rang else { return }
            equipements = reponse.equipements
            avertissements = reponse.avertissements
            derniereMaj = Date()
        } catch {
            guard monRang == rang else { return }
            equipements = []
            erreur = error.localizedDescription
        }
        enCours = false
    }
}
