import Foundation

/// Lance le binaire `macfind` et interprète sa sortie.
enum MacfindCLI {
    /// Ce qu'une exécution a produit.
    struct Reponse: Equatable {
        var equipements: [Equipement]
        /// Messages non bloquants (routeur injoignable, noms indisponibles…).
        var avertissements: [String]
    }

    struct Echec: LocalizedError, Equatable {
        let message: String
        var errorDescription: String? { message }
    }

    /// Fichier de configuration lu par `macfind`.
    static var configuration: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".config/macfind/config.toml")
    }

    /// Emplacement du binaire : `MACFIND_BIN` (développement), puis celui du
    /// bundle, puis une installation en ligne de commande.
    static func binaire() -> URL? {
        let fm = FileManager.default
        if let force = ProcessInfo.processInfo.environment["MACFIND_BIN"] {
            return URL(fileURLWithPath: force)
        }
        if let embarque = Bundle.main.url(forAuxiliaryExecutable: "macfind"),
            fm.isExecutableFile(atPath: embarque.path)
        {
            return embarque
        }
        return ["/usr/local/bin/macfind", "/opt/homebrew/bin/macfind"]
            .first(where: fm.isExecutableFile(atPath:))
            .map(URL.init(fileURLWithPath:))
    }

    /// Exécute `macfind <arguments> --json --no-prompt`.
    static func interroger(_ arguments: [String]) async throws -> Reponse {
        guard let binaire = binaire() else {
            throw Echec(message: "Le binaire macfind est introuvable dans l'application.")
        }
        let resultat = try await lancer(binaire, arguments + ["--json", "--no-prompt"])
        return try interpreter(sortie: resultat.sortie, erreurs: resultat.erreurs)
    }

    /// Routeur saisi dans la fenêtre d'ajout.
    struct NouveauRouteur: Equatable {
        var nom = "principal"
        var adresse = "https://"
        var utilisateur = ""
        var motDePasse = ""
        var certificatAutoSigne = true

        var complet: Bool {
            let vide: (String) -> Bool = { $0.trimmingCharacters(in: .whitespaces).isEmpty }
            return !vide(nom) && !vide(utilisateur) && !motDePasse.isEmpty
                && URL(string: adresse.trimmingCharacters(in: .whitespaces))?.host != nil
        }

        var arguments: [String] {
            let net: (String) -> String = { $0.trimmingCharacters(in: .whitespaces) }
            var args = ["add-router", net(nom), "--url", net(adresse), "--user", net(utilisateur)]
            if certificatAutoSigne { args.append("--insecure") }
            return args + ["--password-stdin"]
        }
    }

    /// Teste la connexion puis enregistre le routeur (config et Trousseau).
    /// `macfind` n'écrit rien si le test échoue.
    static func ajouter(_ routeur: NouveauRouteur) async throws {
        guard let binaire = binaire() else {
            throw Echec(message: "Le binaire macfind est introuvable dans l'application.")
        }
        let resultat = try await lancer(binaire, routeur.arguments, entree: routeur.motDePasse)
        if resultat.code != 0 {
            throw Echec(message: messageDErreur(resultat.erreurs))
        }
    }

    /// Première ligne utile de la sortie d'erreur, sans le préfixe ni la
    /// chaîne « Caused by » d'anyhow.
    static func messageDErreur(_ erreurs: String) -> String {
        let ligne = erreurs.split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty }
        return ligne?.replacingOccurrences(of: "Error: ", with: "")
            ?? "macfind n'a renvoyé aucune réponse."
    }

    /// Sépare résultats, avertissements et erreur. `macfind` écrit le JSON sur
    /// la sortie standard même sans résultat ; son absence signale un échec.
    static func interpreter(sortie: Data, erreurs: String) throws -> Reponse {
        let lignes = erreurs.split(separator: "\n").map {
            $0.trimmingCharacters(in: .whitespaces)
        }
        guard !sortie.isEmpty, let equipements = try? Equipement.decoder(sortie) else {
            let utiles = lignes.filter { !$0.isEmpty && !$0.hasPrefix("🔎") }
                .map { $0.replacingOccurrences(of: "Error: ", with: "") }
            throw Echec(
                message: utiles.isEmpty
                    ? "macfind n'a renvoyé aucune réponse." : utiles.joined(separator: "\n"))
        }
        let avertissements = lignes.filter { $0.hasPrefix("⚠️") }.map {
            $0.dropFirst(2).trimmingCharacters(in: .whitespaces)
        }
        return Reponse(equipements: equipements, avertissements: avertissements)
    }

    /// Lance le binaire. `entree`, si elle est fournie, est écrite sur son
    /// entrée standard (mot de passe : jamais en argument, visible par `ps`).
    private static func lancer(
        _ binaire: URL, _ arguments: [String], entree: String? = nil
    ) async throws -> (sortie: Data, erreurs: String, code: Int32) {
        try await withCheckedThrowingContinuation { suite in
            DispatchQueue.global(qos: .userInitiated).async {
                let processus = Process()
                processus.executableURL = binaire
                processus.arguments = arguments
                let sortie = Pipe()
                let erreurs = Pipe()
                let saisie = Pipe()
                processus.standardOutput = sortie
                processus.standardError = erreurs
                // Sans entrée fournie, aucune saisie possible : macfind ne
                // doit jamais attendre.
                processus.standardInput = entree == nil ? FileHandle.nullDevice : saisie
                do {
                    try processus.run()
                } catch {
                    suite.resume(
                        throwing: Echec(
                            message: "Lancement de macfind impossible : "
                                + error.localizedDescription))
                    return
                }
                if let entree {
                    saisie.fileHandleForWriting.write(Data((entree + "\n").utf8))
                    try? saisie.fileHandleForWriting.close()
                }
                // Les deux tubes sont vidés en parallèle : un tube plein
                // bloquerait macfind.
                var texteErreurs = Data()
                let groupe = DispatchGroup()
                groupe.enter()
                DispatchQueue.global().async {
                    texteErreurs = erreurs.fileHandleForReading.readDataToEndOfFile()
                    groupe.leave()
                }
                let donnees = sortie.fileHandleForReading.readDataToEndOfFile()
                groupe.wait()
                processus.waitUntilExit()
                suite.resume(
                    returning: (
                        donnees, String(decoding: texteErreurs, as: UTF8.self),
                        processus.terminationStatus
                    ))
            }
        }
    }
}
