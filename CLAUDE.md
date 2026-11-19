# macfind

Outil en ligne de commande Rust pour macOS : retrouver l'IP d'un équipement à
partir de son adresse MAC (ou l'inverse) en interrogeant des routeurs MikroTik
par l'API REST de RouterOS v7.

On passe par le routeur parce que, depuis le Mac, l'ARP ne voit que le segment
local ; le routeur connaît tous les sous-réseaux.

- Baux DHCP : `GET /rest/ip/dhcp-server/lease`
- Complétés par la table ARP : `GET /rest/ip/arp` (équipements en IP fixe,
  sans doublon avec les baux)
- Port de branchement : `GET /rest/interface/bridge/host` (facultatif : un
  échec de cette lecture est ignoré)
- RouterOS renvoie toutes les valeurs sous forme de chaînes (`"true"`, `"1m7s"`).

## Code

```
src/
  main.rs      CLI clap, règles de recherche (`matches`), `Hit` (ligne unifiée
               DHCP/ARP), affichage comfy-table, sortie --json
  config.rs    ~/.config/macfind/config.toml (plusieurs [[routers]]),
               identifiants et Trousseau macOS (crate keyring)
  routeros.rs  client REST (reqwest blocking, native-tls), `Lease`, `ArpEntry`
  anomaly.rs   détection des anomalies (baux multiples, IP partagée, adresse
               hors bail) à partir des baux et de l'ARP ; fonctions pures
  mac.rs       normalisation et détection des MAC
  history.rs   historique local par MAC (`~/.config/macfind/history.json`) :
               IP successives, première et dernière vue, « nouveaux »
  names.rs     noms donnés aux équipements (`~/.config/macfind/names.toml`,
               clé = MAC) ; le routeur n'est jamais modifié
  oui.rs       fabricant à partir de l'OUI (`data/oui.tsv` embarqué par
               `include_str!`, recherche dichotomique)
  scan.rs      balayage du segment local : un datagramme UDP par adresse pour
               provoquer la résolution ARP, puis lecture de `arp -an`
data/oui.tsv   registre IEEE MA-L trié, généré par `scripts/maj-oui.sh` ;
               ne pas l'éditer à la main
```

Application graphique (SwiftUI, paquet SwiftPM dans `app/`) : aucune logique
réseau, elle lance le binaire `macfind` embarqué avec `--json --no-prompt`.

```
app/Sources/MacfindApp/
  MacfindApp.swift   point d'entrée, fenêtre
  ContentView.swift  tableau, recherche, barre d'outils
  Inventaire.swift   état de la fenêtre (source, chargement, erreurs)
  MacfindCLI.swift   lancement du binaire, tri résultats / avertissements
  Equipement.swift   décodage de `Hit`, tri des IP, filtre de recherche
  AjoutRouteurView.swift  fenêtre d'ajout d'un routeur (`add-router`)
  RouteursView.swift fenêtre « Routeurs » (liste, retrait)
  Export.swift       export CSV pour Excel des lignes affichées
  RenommerView.swift fenêtre « Renommer » (`macfind name`)
  HistoriqueView.swift fenêtre « Historique » (`macfind history`)
scripts/build-app.sh assemble `dist/macfind.app` (signature ad hoc) ;
                     `--arch … --dmg` produit les images disque des releases
scripts/make-icon.swift dessine l'icône (pas de symbole SF : interdit dans
                     une icône d'application)
```

- Tests de l'app : `swift test --package-path app`.
- Pas de compte développeur Apple pour le moment : ni Developer ID ni
  notarisation.
- Pas de binaire universel : macOS 26 affiche « Fin de la prise en charge
  des apps basées sur Intel » pour une application non notariée contenant du
  code Intel, même lancée en natif (constaté avec la 0.3.0). Une image
  disque par architecture.

- Commandes : `init`, `add-router`, `remove-router`, `set-password`, `forget`, `leases`,
  `anomalies`, `routers`, `scan`, `name`, `names`, `new`, `history`.
- Options globales : `--router`, `--no-arp`, `--json`, `--csv`, `--ask`,
  `--no-prompt`.
- Le format CSV existe en deux exemplaires à garder identiques : `to_csv`
  (main.rs) et `Export.csv` (app).
- Recherche : MAC partielle dans tout format, IP exacte, préfixe terminé par
  un point (`192.168.20.`), nom d'hôte, commentaire, fabricant.
- Une MAC sans fabricant connu dont le bit « administrée localement » est levé
  est affichée « (MAC privée) » (adresse aléatoire).
- `scan [cible] [--filter texte]` ne balaie que les sous-réseaux auxquels le
  Mac est relié (1024 adresses au plus). Les noms viennent des baux du routeur
  si la config existe ; sinon le balayage continue sans eux. Le balayage d'un
  sous-réseau distant passerait par `/tool ip-scan` du routeur et demanderait
  la policy `test` sur le compte : non décidé.
- `fetch` (main.rs) interroge les routeurs ; `collect` et `scan_command`
  s'appuient dessus.
- Un thread par routeur ; les identifiants manquants sont demandés avant le
  lancement des requêtes, puis proposés au Trousseau après une connexion réussie.
- Code de sortie 1 quand une recherche ne trouve rien, 2 sans argument.
- `MACFIND_PASSWORD` remplace le Trousseau pour un test ponctuel.
- La structure `Hit` est sérialisée telle quelle par `--json` : renommer ou
  retirer un champ est un changement incompatible pour les scripts.

Docs : `README.md`, `docs/mikrotik.md` (préparation du routeur, dépannage),
`docs/RELEASING.md`, `CHANGELOG.md`.

## Contraintes

- **Garder native-tls.** rustls échoue à la négociation TLS avec le routeur de
  test. Ne pas activer la fonctionnalité `rustls-tls` de reqwest, et vérifier
  qu'une nouvelle dépendance ne l'amène pas.
- **Tout en français** : messages, aide clap, documentation, commentaires.
- **Jamais de mot de passe ni d'identifiant réel dans le dépôt**, y compris dans
  les tests, les exemples et la documentation. Le mot de passe ne va jamais
  dans le fichier de configuration.
- Avant chaque commit :
  ```bash
  cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
  ```
- Commits au format Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`).
- Chaque changement visible par l'utilisateur ajoute une ligne sous
  `## [Non publié]` dans `CHANGELOG.md` (Ajouté, Modifié, Corrigé, Supprimé).
- Versionnage sémantique. Une étiquette `vX.Y.Z` poussée déclenche
  `.github/workflows/release.yml`, qui compile et publie les binaires.
  Suivre `docs/RELEASING.md` ; ne pas pousser d'étiquette sans demande explicite.
- `main` reste compilable ; le travail se fait sur des branches `feat/…`,
  `fix/…` fusionnées par pull request.

## Environnement

- macOS uniquement (Apple Silicon et Intel).
- Dépôt public : `github.com/lwilly3/macfind`, licence MIT.
- Le dépôt est public : aucune donnée d'un réseau réel (MAC, nom de machine,
  adresse, nom de compte) dans le code, les tests, la doc ou les messages de
  commit. Utiliser des valeurs fictives, par exemple `00:15:5D:00:00:01`.
- Les commits utilisent l'adresse GitHub masquée (`user.email` local du dépôt).
- Les détails du poste et du routeur de test sont dans `CLAUDE.local.md`,
  ignoré par git.
- Aucun routeur n'est joignable depuis la CI : les tests unitaires ne doivent
  jamais en dépendre. Tester la logique sur des structures construites à la
  main, et vérifier le comportement réel avec le binaire.

## Fonctions prévues, dans l'ordre

1. ~~Fabricant de l'équipement à partir de l'OUI de la MAC~~ : fait.
2. ~~Tri et regroupement par sous-réseau ou par serveur DHCP~~ : fait
   (`leases --group subnet|server`). Le sous-réseau est supposé en /24.
3. ~~Détection des anomalies~~ : fait (`macfind anomalies`). Le déplacement
   d'un équipement entre deux serveurs DHCP n'est volontairement pas signalé.
4. ~~Historique local des IP par MAC~~ : fait (`history`, `new`).
