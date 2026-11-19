# Journal des modifications

Toutes les modifications notables de ce projet sont consignées ici.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/)
et le projet respecte le [versionnage sémantique](https://semver.org/lang/fr/).

## [Non publié]

## [0.3.1] - 2026-11-19

### Modifié
- Application : deux images disque, `macfind-X.Y.Z-apple-silicon.dmg` et
  `macfind-X.Y.Z-intel.dmg`, au lieu d'une image universelle. Sur un Mac
  Apple Silicon, macOS 26 affichait « Fin de la prise en charge des apps
  basées sur Intel » pour l'image universelle de la 0.3.0.

### Corrigé
- `macfind scan` échouait avec « Ce Mac est relié à plusieurs réseaux » dès
  qu'un VPN était actif (WireGuard en /24, par exemple). Les interfaces de
  tunnel sont maintenant ignorées : elles n'ont pas d'ARP.
- Application : sur un écran de message, les onglets restaient au milieu de
  la fenêtre au lieu d'en haut.

## [0.3.0] - 2026-11-19

### Modifié
- `macfind leases` lit aussi la table ARP, nécessaire à la détection des
  anomalies (une requête de plus par routeur ; `--no-arp` l'évite).

### Ajouté
- Détection des anomalies : commande `macfind anomalies` et champ JSON
  `anomaly`. Sont signalés une MAC avec plusieurs baux actifs sur le même
  serveur DHCP, une IP utilisée par deux MAC, et un équipement qui répond à
  une adresse hors de ses baux (IP fixe en plus de baux dynamiques). Une
  colonne « Anomalie » s'ajoute aux tableaux quand il y a lieu.
- Application : vue « Anomalies » et repère sur les lignes signalées.
- `macfind leases --group subnet|server` : un tableau par sous-réseau (/24)
  ou par serveur DHCP, trié par adresse. Champs JSON `server` et `subnet`.
- Application : menu « Regrouper par » (sous-réseau ou serveur DHCP).
- Port du routeur où chaque équipement est branché (table des hôtes du
  bridge) : colonne « Port » et champ JSON `port`. Derrière un switch ou une
  borne Wi-Fi, c'est le port qui y mène.
- Export pour Excel : option `--csv` (séparateur « ; », UTF-8 avec marque
  d'ordre d'octets) et bouton « Exporter pour Excel » dans l'application,
  qui enregistre les lignes affichées. Les valeurs qui ressemblent à une
  formule sont neutralisées.
- Noms personnalisés : `macfind name <MAC> <nom>` (ou `--clear`) et
  `macfind names`. Le nom est gardé dans `~/.config/macfind/names.toml`,
  affiché dans la colonne « Hôte », pris en compte par la recherche et
  exposé dans le champ JSON `name`. Le routeur n'est pas modifié.
- Application : « Renommer… » par clic droit sur une ligne.
- Historique local (`~/.config/macfind/history.json`) : chaque lecture des
  routeurs ou balayage note les équipements vus, leurs adresses IP
  successives, la première et la dernière vue. `macfind history <requête>`
  affiche la fiche d'un équipement sans contacter de routeur.
- `macfind new [--days N]` : équipements apparus depuis N jours (7 par
  défaut). Ceux présents lors du premier relevé ne comptent pas.
- Champs JSON `first_seen`, `last_seen` (secondes Unix) et `new`.
- Application : onglet « Nouveaux », pastille « Nouveau » sur les lignes et
  « Historique… » par clic droit.
- `macfind remove-router <nom>` retire un routeur de la configuration et
  efface ses identifiants du Trousseau ; `macfind routers --json` liste les
  routeurs pour les scripts.
- Application : fenêtre « Routeurs » pour ajouter ou retirer un routeur.
- Application : icône, binaire universel (Apple Silicon et Intel) et image
  disque `macfind-X.Y.Z.dmg` jointe aux releases.
- Option `--no-prompt` : ne pose aucune question et échoue si les identifiants
  d'un routeur manquent (pour les scripts et l'application graphique).
- Commande `macfind add-router <nom> --url … --user …` : teste la connexion,
  puis écrit le routeur dans la configuration et range le mot de passe dans
  le Trousseau. Rien n'est écrit si le test échoue. `--password-stdin` lit le
  mot de passe sur l'entrée standard.
- Première version de l'application graphique macOS (`app/`, construite par
  `scripts/build-app.sh`) : tableau triable des baux DHCP et du balayage
  local, recherche, copie de l'IP ou de la MAC, ajout d'un routeur sans
  passer par le Terminal.

## [0.2.0] - 2026-11-18

### Ajouté
- Colonne « Fabricant » et champ JSON `vendor`, déduits de l'OUI de l'adresse
  MAC grâce au registre IEEE embarqué dans le binaire (aucune requête réseau).
- Les adresses MAC aléatoires (iPhone, Android, Windows récents) sont
  signalées par « (MAC privée) ».
- Recherche par fabricant : `macfind hikvision`, `macfind privée`.
- Commande `macfind scan` : balayage actif du sous-réseau local du Mac, qui
  trouve aussi les équipements en IP fixe absents des baux. Les noms d'hôte
  sont repris des baux du routeur ; `--filter` restreint l'affichage.

## [0.1.0] - 2026-11-18

### Ajouté
- Recherche par adresse MAC complète ou partielle, dans n'importe quel format
  (`aa:bb:cc…`, `aa-bb-cc…`, `aabb.ccdd.eeff`, `ddeeff`).
- Recherche par IP exacte, par préfixe de sous-réseau (`192.168.20.`),
  par nom d'hôte ou par commentaire.
- Lecture des baux DHCP et de la table ARP via l'API REST de RouterOS v7.
- Plusieurs routeurs interrogés en parallèle (`~/.config/macfind/config.toml`).
- Identifiants demandés au lancement s'ils manquent, puis enregistrement
  facultatif dans le Trousseau macOS.
- Commandes `init`, `set-password`, `forget`, `leases`, `routers`.
- Options `--router`, `--no-arp`, `--json`, `--ask`.

[Non publié]: https://github.com/lwilly3/macfind/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/lwilly3/macfind/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/lwilly3/macfind/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/lwilly3/macfind/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lwilly3/macfind/releases/tag/v0.1.0
