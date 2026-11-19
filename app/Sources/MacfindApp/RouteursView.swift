import SwiftUI

/// Fenêtre « Routeurs » : liste, ajout et retrait.
struct RouteursView: View {
    /// Appelé quand la liste a changé.
    let modifie: () -> Void

    @Environment(\.dismiss) private var fermer
    @State private var routeurs: [Routeur] = []
    @State private var erreur: String?
    @State private var ajout = false
    @State private var aRetirer: Routeur?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Routeurs").font(.title2).bold()
            if routeurs.isEmpty {
                Text("Aucun routeur configuré.")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 80)
            } else {
                List(routeurs) { routeur in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(routeur.name).bold()
                            Text(
                                [routeur.url, routeur.user.isEmpty ? nil : "compte \(routeur.user)"]
                                    .compactMap { $0 }.joined(separator: " · ")
                            )
                            .font(.callout)
                            .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(role: .destructive) {
                            aRetirer = routeur
                        } label: {
                            Label("Retirer", systemImage: "trash")
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .help("Retirer ce routeur")
                    }
                    .padding(.vertical, 2)
                }
                .frame(minHeight: 120)
            }
            if let erreur {
                Label(erreur, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Button("Ajouter un routeur…") { ajout = true }
                Spacer()
                Button("Fermer") { fermer() }
                    .keyboardShortcut(.defaultAction)
            }
            Text("Pour changer l'adresse ou le mot de passe d'un routeur, retire-le puis ajoute-le de nouveau.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(20)
        .frame(width: 520)
        .task { await relire() }
        .sheet(isPresented: $ajout) {
            AjoutRouteurView {
                modifie()
                Task { await relire() }
            }
        }
        .confirmationDialog(
            "Retirer le routeur « \(aRetirer?.name ?? "") » ?",
            isPresented: Binding(get: { aRetirer != nil }, set: { if !$0 { aRetirer = nil } }),
            presenting: aRetirer
        ) { routeur in
            Button("Retirer", role: .destructive) { retirer(routeur) }
            Button("Annuler", role: .cancel) {}
        } message: { _ in
            Text("Il est retiré de la configuration et son mot de passe est effacé du Trousseau. L'historique et les noms donnés sont conservés.")
        }
    }

    private func relire() async {
        do {
            routeurs = try await MacfindCLI.routeurs()
        } catch {
            erreur = error.localizedDescription
        }
    }

    private func retirer(_ routeur: Routeur) {
        erreur = nil
        Task {
            do {
                try await MacfindCLI.retirer(routeur: routeur.name)
                modifie()
            } catch {
                erreur = error.localizedDescription
            }
            await relire()
        }
    }
}
