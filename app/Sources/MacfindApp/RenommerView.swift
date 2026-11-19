import SwiftUI

/// Fenêtre « Renommer » : donne un nom à un équipement sans toucher au routeur.
struct RenommerView: View {
    let equipement: Equipement
    /// Appelé une fois le nom enregistré.
    let termine: () -> Void

    @Environment(\.dismiss) private var fermer
    @State private var nom: String
    @State private var erreur: String?
    @State private var enCours = false

    init(equipement: Equipement, termine: @escaping () -> Void) {
        self.equipement = equipement
        self.termine = termine
        _nom = State(initialValue: equipement.name)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Renommer l'équipement").font(.title3).bold()
                Text(description)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            TextField("Nom", text: $nom, prompt: Text("Caméra entrée"))
                .textFieldStyle(.roundedBorder)
                .onSubmit(valider)
            Text("Le nom est gardé sur ce Mac, d'après l'adresse MAC. Laisse le champ vide pour l'effacer.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let erreur {
                Label(erreur, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button("Annuler", role: .cancel) { fermer() }
                    .keyboardShortcut(.cancelAction)
                Button("Enregistrer", action: valider)
                    .keyboardShortcut(.defaultAction)
                    .disabled(enCours)
            }
        }
        .padding(20)
        .frame(width: 420)
    }

    private var description: String {
        [equipement.mac, equipement.ip, equipement.host]
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

    private func valider() {
        guard !enCours else { return }
        enCours = true
        erreur = nil
        Task {
            do {
                try await MacfindCLI.nommer(mac: equipement.mac, nom: nom)
                termine()
                fermer()
            } catch {
                erreur = error.localizedDescription
            }
            enCours = false
        }
    }
}
