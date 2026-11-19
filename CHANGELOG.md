# Journal des modifications

Toutes les modifications notables de ce projet sont consignées ici.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/)
et le projet respecte le [versionnage sémantique](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
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

[Non publié]: https://github.com/lwilly3/macfind/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/lwilly3/macfind/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lwilly3/macfind/releases/tag/v0.1.0
