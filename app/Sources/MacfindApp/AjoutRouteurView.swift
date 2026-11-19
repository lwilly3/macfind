import SwiftUI

/// Fenêtre d'ajout d'un routeur : remplace `macfind init` et `set-password`.
struct AjoutRouteurView: View {
    /// Appelé une fois le routeur enregistré.
    let termine: () -> Void

    @Environment(\.dismiss) private var fermer
    @State private var routeur = MacfindCLI.NouveauRouteur()
    @State private var erreur: String?
    @State private var enCours = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Ajouter un routeur").font(.title2).bold()
                Text(
                    "macfind lit les baux DHCP du routeur MikroTik par son API REST (RouterOS 7). Un compte en lecture seule suffit."
                )
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }

            Form {
                TextField("Nom", text: $routeur.nom, prompt: Text("principal"))
                TextField(
                    "Adresse", text: $routeur.adresse, prompt: Text("https://192.168.88.1"))
                TextField("Compte", text: $routeur.utilisateur, prompt: Text("macfind"))
                SecureField("Mot de passe", text: $routeur.motDePasse)
                Toggle("Accepter un certificat auto-signé", isOn: $routeur.certificatAutoSigne)
                    .help("Cas habituel d'un MikroTik sans certificat émis par une autorité.")
            }
            .disabled(enCours)
            .onSubmit(valider)

            if let erreur {
                Label(erreur, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Text("Le mot de passe est rangé dans le Trousseau, jamais dans un fichier.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Spacer()
                if enCours {
                    ProgressView().controlSize(.small)
                }
                Button("Annuler", role: .cancel) { fermer() }
                    .keyboardShortcut(.cancelAction)
                Button("Tester et enregistrer", action: valider)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!routeur.complet || enCours)
            }
        }
        .padding(20)
        .frame(width: 520)
    }

    private func valider() {
        guard routeur.complet, !enCours else { return }
        enCours = true
        erreur = nil
        Task {
            do {
                try await MacfindCLI.ajouter(routeur)
                termine()
                fermer()
            } catch {
                erreur = error.localizedDescription
            }
            enCours = false
        }
    }
}
