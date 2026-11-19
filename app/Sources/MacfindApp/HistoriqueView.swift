import SwiftUI

/// Fenêtre « Historique » : adresses IP successives d'un équipement.
struct HistoriqueView: View {
    let equipement: Equipement

    @Environment(\.dismiss) private var fermer
    @State private var fiche: Fiche?
    @State private var message: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text(equipement.nomAffiche.isEmpty ? equipement.mac : equipement.nomAffiche)
                    .font(.title3).bold()
                Text([equipement.mac, equipement.vendor].filter { !$0.isEmpty }
                    .joined(separator: " · "))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            if let fiche {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                    GridRow {
                        Text("Première vue").foregroundStyle(.secondary)
                        Text(date(fiche.firstSeen))
                    }
                    GridRow {
                        Text("Dernière vue").foregroundStyle(.secondary)
                        Text(date(fiche.lastSeen))
                    }
                }
                Table(fiche.ips) {
                    TableColumn("IP") { Text($0.ip).monospaced().textSelection(.enabled) }
                        .width(min: 120, ideal: 135)
                    TableColumn("Première vue") { Text(date($0.firstSeen)) }
                    TableColumn("Dernière vue") { Text(date($0.lastSeen)) }
                }
                .frame(minHeight: 140)
            } else if let message {
                Text(message).foregroundStyle(.secondary)
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
            HStack {
                Text("L'historique est gardé sur ce Mac et s'enrichit à chaque lecture.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Fermer") { fermer() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 560)
        .task {
            do {
                fiche = try await MacfindCLI.historique(mac: equipement.mac)
                if fiche == nil {
                    message = "Cet équipement ne figure pas encore dans l'historique."
                }
            } catch {
                message = error.localizedDescription
            }
        }
    }

    private func date(_ d: Date) -> String {
        d.formatted(date: .abbreviated, time: .shortened)
    }
}
