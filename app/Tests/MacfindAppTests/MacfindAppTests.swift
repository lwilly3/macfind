import XCTest

@testable import MacfindApp

final class MacfindAppTests: XCTestCase {
    // Valeurs fictives : aucun équipement réel.
    private let json = Data(
        """
        [
          {"router": "principal", "source": "DHCP", "ip": "192.168.20.15",
           "mac": "00:15:5D:00:00:01", "vendor": "Microsoft Corporation",
           "host": "serveur-test", "detail": "dhcp20 (dynamique)",
           "status": "bound (vu il y a 1m7s)"},
          {"router": "principal", "source": "DHCP", "ip": "192.168.20.9",
           "mac": "DA:A1:19:00:00:01", "vendor": "(MAC privée)",
           "host": "", "detail": "dhcp20 (statique) — Caméra", "status": "waiting",
           "anomaly": "2 baux actifs sur dhcp20", "port": "ether5",
           "name": "Caméra entrée", "new": true, "first_seen": 86400, "last_seen": 172800}
        ]
        """.utf8)

    func testDecodeLaSortieJson() throws {
        let equipements = try Equipement.decoder(json)
        XCTAssertEqual(equipements.map(\.id), [0, 1])
        XCTAssertEqual(equipements[0].host, "serveur-test")
        XCTAssertEqual(equipements[1].vendor, "(MAC privée)")
        // Le champ `anomaly` est facultatif (anciennes versions de macfind).
        XCTAssertEqual(equipements[0].anomaly, "")
        XCTAssertEqual(equipements[1].anomaly, "2 baux actifs sur dhcp20")
        XCTAssertTrue(equipements[1].correspond(a: "baux actifs"))
        XCTAssertEqual(equipements[0].port, "")
        XCTAssertEqual(equipements[1].port, "ether5")
        XCTAssertTrue(equipements[1].correspond(a: "ether5"))
        // Le nom donné remplace le nom d'hôte à l'affichage et se cherche.
        XCTAssertEqual(equipements[0].nomAffiche, "serveur-test")
        XCTAssertEqual(equipements[1].nomAffiche, "Caméra entrée")
        XCTAssertTrue(equipements[1].correspond(a: "caméra"))
        XCTAssertFalse(equipements[0].new)
        XCTAssertTrue(equipements[1].new)
        XCTAssertEqual(equipements[0].firstSeen, 0)
        XCTAssertEqual(equipements[1].firstSeen, 86400)
        XCTAssertEqual(equipements[1].lastSeen, 172800)
    }

    func testTriNumeriqueDesIp() {
        XCTAssertLessThan(Equipement.ipKey("192.168.1.9"), Equipement.ipKey("192.168.1.10"))
        XCTAssertLessThan(Equipement.ipKey("10.0.0.1"), Equipement.ipKey("192.168.1.1"))
        // Une adresse vide ou invalide est classée en dernier.
        XCTAssertEqual(Equipement.ipKey(""), .max)
        XCTAssertEqual(Equipement.ipKey("192.168.1.300"), .max)
    }

    func testRecherche() throws {
        let e = try Equipement.decoder(json)[0]
        XCTAssertTrue(e.correspond(a: ""))
        XCTAssertTrue(e.correspond(a: "SERVEUR"))
        XCTAssertTrue(e.correspond(a: "192.168.20."))
        XCTAssertTrue(e.correspond(a: "microsoft"))
        XCTAssertTrue(e.correspond(a: "00:15:5d"))
        XCTAssertTrue(e.correspond(a: "0015.5d00"))
        XCTAssertTrue(e.correspond(a: "5d-00-00"))
        XCTAssertFalse(e.correspond(a: "apple"))
        XCTAssertFalse(e.correspond(a: "aa-bb-cc"))
    }

    func testIpCompleteExacte() throws {
        let e = try Equipement.decoder(json)[0]  // 192.168.20.15
        XCTAssertTrue(e.correspond(a: "192.168.20.15"))
        XCTAssertFalse(e.correspond(a: "192.168.20.1"))
        // Une saisie incomplète reste un filtre « contient ».
        XCTAssertTrue(e.correspond(a: "192.168.20."))
        XCTAssertFalse(e.correspond(a: "192.168.20.1."))
        XCTAssertEqual(Equipement.ipKey("192.168.20.1."), .max)
        XCTAssertTrue(e.correspond(a: "168.20"))
    }

    func testAvertissementsSeparesDesResultats() throws {
        let reponse = try MacfindCLI.interpreter(
            sortie: json,
            erreurs: "🔎 Balayage de 254 adresse(s) sur en0 (environ 2 s)…\n⚠️  site2 : délai dépassé\n")
        XCTAssertEqual(reponse.equipements.count, 2)
        XCTAssertEqual(reponse.avertissements, ["site2 : délai dépassé"])
    }

    func testListeVideSansErreur() throws {
        let reponse = try MacfindCLI.interpreter(sortie: Data("[]\n".utf8), erreurs: "")
        XCTAssertTrue(reponse.equipements.isEmpty)
    }

    func testEchecSansJson() {
        XCTAssertThrowsError(
            try MacfindCLI.interpreter(
                sortie: Data(),
                erreurs: "Error: Identifiants manquants pour « principal »\n")
        ) { erreur in
            XCTAssertEqual(
                erreur.localizedDescription, "Identifiants manquants pour « principal »")
        }
    }

    func testArgumentsDAjoutDeRouteur() {
        var r = MacfindCLI.NouveauRouteur()
        XCTAssertFalse(r.complet)
        r.nom = " site2 "
        r.adresse = "https://192.0.2.1"
        r.utilisateur = "lecture"
        r.motDePasse = "exemple"
        XCTAssertTrue(r.complet)
        // Le mot de passe ne figure jamais dans les arguments.
        XCTAssertEqual(
            r.arguments,
            [
                "add-router", "site2", "--url", "https://192.0.2.1", "--user", "lecture",
                "--insecure", "--password-stdin",
            ])
        r.certificatAutoSigne = false
        XCTAssertFalse(r.arguments.contains("--insecure"))
        r.adresse = "https://"
        XCTAssertFalse(r.complet)
    }

    func testMessageDErreurSansChaineAnyhow() {
        XCTAssertEqual(
            MacfindCLI.messageDErreur(
                "Error: [essai] connexion impossible à https://192.0.2.1\n\nCaused by:\n    délai dépassé\n"),
            "[essai] connexion impossible à https://192.0.2.1")
    }

    func testRegroupement() throws {
        let lignes = try Equipement.decoder(
            Data(
                """
                [
                  {"router": "r", "source": "DHCP", "ip": "198.51.100.7", "mac": "00:15:5D:00:00:01",
                   "vendor": "", "host": "", "detail": "", "status": "", "anomaly": "",
                   "server": "invites", "subnet": "198.51.100.0/24"},
                  {"router": "r", "source": "DHCP", "ip": "192.0.2.10", "mac": "00:15:5D:00:00:02",
                   "vendor": "", "host": "", "detail": "", "status": "", "anomaly": "",
                   "server": "bureau", "subnet": "192.0.2.0/24"},
                  {"router": "r", "source": "ARP", "ip": "192.0.2.9", "mac": "00:15:5D:00:00:03",
                   "vendor": "", "host": "", "detail": "", "status": "", "anomaly": "",
                   "server": "", "subnet": "192.0.2.0/24"}
                ]
                """.utf8))
        // Sous-réseaux dans l'ordre numérique, ordre des lignes conservé.
        let parReseau = Groupe.repartir(lignes, par: .sousReseau)
        XCTAssertEqual(parReseau.map(\.titre), ["192.0.2.0/24 — 2", "198.51.100.0/24 — 1"])
        XCTAssertEqual(parReseau[0].lignes.map(\.ip), ["192.0.2.10", "192.0.2.9"])
        // Les lignes sans serveur ferment la liste.
        let parServeur = Groupe.repartir(lignes, par: .serveur)
        XCTAssertEqual(
            parServeur.map(\.titre), ["bureau — 1", "invites — 1", "Sans serveur DHCP — 1"])
    }

    func testExportCsvPourExcel() throws {
        XCTAssertEqual(Export.champ("imprimante"), "imprimante")
        XCTAssertEqual(Export.champ("a;b"), "\"a;b\"")
        XCTAssertEqual(Export.champ("dit \"x\""), "\"dit \"\"x\"\"\"")
        // Un nom d'hôte ne doit pas devenir une formule.
        XCTAssertEqual(Export.champ("=1+1"), "'=1+1")

        let lignes = Export.csv(try Equipement.decoder(json)).components(separatedBy: "\r\n")
        XCTAssertEqual(
            lignes[0],
            "\u{feff}Routeur;Source;IP;MAC;Fabricant;Nom;Hôte;Port;Serveur / Interface;Statut;Anomalie"
        )
        XCTAssertEqual(
            lignes[2],
            "principal;DHCP;192.168.20.9;DA:A1:19:00:00:01;(MAC privée);Caméra entrée;;ether5;dhcp20 (statique) — Caméra;waiting;2 baux actifs sur dhcp20"
        )
        XCTAssertEqual(lignes.count, 4)  // en-tête, deux lignes, fin de fichier
        XCTAssertEqual(
            Export.nomDeFichier(source: .baux, date: Date(timeIntervalSince1970: 0)),
            "macfind-baux-dhcp-1970-01-01.csv")
    }

    func testArgumentsDeNom() {
        let mac = "00:15:5D:00:00:01"
        XCTAssertEqual(
            MacfindCLI.argumentsDeNom(mac: mac, nom: " Caméra entrée "),
            ["name", "--", mac, "Caméra entrée"])
        // Un nom vide efface le nom existant.
        XCTAssertEqual(MacfindCLI.argumentsDeNom(mac: mac, nom: "  "), ["name", "--clear", mac])
    }

    func testFicheDHistorique() throws {
        let fiches = try Fiche.decoder(
            Data(
                """
                [{"mac": "00:15:5D:00:00:01", "vendor": "Microsoft Corporation", "name": "",
                  "host": "serveur-test", "first_seen": 86400, "last_seen": 172800, "new": false,
                  "ips": [{"ip": "192.0.2.25", "first_seen": 90000, "last_seen": 172800},
                          {"ip": "192.0.2.10", "first_seen": 86400, "last_seen": 90000}]}]
                """.utf8))
        XCTAssertEqual(fiches.count, 1)
        XCTAssertEqual(fiches[0].firstSeen, Date(timeIntervalSince1970: 86400))
        XCTAssertEqual(fiches[0].ips.map(\.ip), ["192.0.2.25", "192.0.2.10"])
        XCTAssertEqual(fiches[0].ips[0].lastSeen, Date(timeIntervalSince1970: 172800))
        XCTAssertTrue(try Fiche.decoder(Data("[]".utf8)).isEmpty)
    }

    func testListeDesRouteurs() throws {
        let routeurs = try JSONDecoder().decode(
            [Routeur].self,
            from: Data(
                """
                [{"name": "principal", "url": "https://192.0.2.1", "user": "lecture",
                  "insecure": true}]
                """.utf8))
        XCTAssertEqual(routeurs.map(\.name), ["principal"])
        XCTAssertTrue(routeurs[0].insecure)
    }
}
