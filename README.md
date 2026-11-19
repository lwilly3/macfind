# macfind

[![CI](https://github.com/lwilly3/macfind/actions/workflows/ci.yml/badge.svg)](https://github.com/lwilly3/macfind/actions/workflows/ci.yml)
[![Version](https://img.shields.io/github/v/release/lwilly3/macfind?label=version)](https://github.com/lwilly3/macfind/releases/latest)
[![Licence MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)

Retrouve l'adresse IP d'un équipement à partir de son adresse MAC (ou l'inverse)
en interrogeant directement les routeurs MikroTik : baux DHCP et table ARP,
par l'API REST de RouterOS v7.

```console
$ macfind abcd
┌───────────┬────────┬───────────────┬───────────────────┬────────────────────────────┬─────────┬──────────────────────────────┬────────────────────────┐
│ Routeur   ┆ Source ┆ IP            ┆ MAC               ┆ Fabricant                  ┆ Hôte    ┆ Serveur / Interface          ┆ Statut                 │
╞═══════════╪════════╪═══════════════╪═══════════════════╪════════════════════════════╪═════════╪══════════════════════════════╪════════════════════════╡
│ principal ┆ DHCP   ┆ 192.168.1.157 ┆ EC:EB:B8:12:AB:CD ┆ Hewlett Packard Enterprise ┆ serveur ┆ serveurDHCPLocal (dynamique) ┆ bound (vu il y a 1m7s) │
└───────────┴────────┴───────────────┴───────────────────┴────────────────────────────┴─────────┴──────────────────────────────┴────────────────────────┘
```

**Pourquoi passer par le routeur ?** Depuis un poste, la table ARP ne montre
que le segment local. Les équipements des autres sous-réseaux et VLAN ne sont
visibles que du routeur, qui connaît tous les couples MAC ↔ IP.

## Prérequis

- macOS (Apple Silicon ou Intel)
- Un routeur MikroTik en RouterOS v7.1 ou plus récent

## Installation

### Binaire prêt à l'emploi

Télécharge l'archive de ton Mac depuis la page
[Releases](https://github.com/lwilly3/macfind/releases)
(`aarch64` pour Apple Silicon, `x86_64` pour Intel), puis :

```bash
tar -xzf macfind-v*-apple-darwin.tar.gz
cd macfind-v*-apple-darwin
xattr -d com.apple.quarantine macfind 2>/dev/null
sudo install -m 755 macfind /usr/local/bin/macfind
macfind --version
```

### Depuis les sources

Avec [Rust](https://rustup.rs) installé :

```bash
git clone https://github.com/lwilly3/macfind.git
cd macfind
cargo install --path .
```

## Mise en place

1. **Préparer le routeur** : compte en lecture seule, certificat, service
   HTTPS. Les commandes à coller sont dans [docs/mikrotik.md](docs/mikrotik.md).

2. **Créer la configuration** :
   ```bash
   macfind init
   open -e ~/.config/macfind/config.toml
   ```
   ```toml
   [[routers]]
   name = "principal"
   url = "https://192.168.88.1"   # adresse de ton routeur
   user = "macfind"               # facultatif : demandé au lancement s'il manque
   insecure = true                # certificat auto-signé
   timeout_secs = 5
   ```
   Ajoute un bloc `[[routers]]` par routeur.

   En une commande, sans éditer le fichier (le mot de passe est demandé,
   la connexion testée, puis le tout enregistré) :
   ```bash
   macfind add-router principal --url https://192.168.88.1 --user macfind --insecure
   ```

3. **Premier lancement** :
   ```bash
   macfind leases
   ```
   macfind demande les identifiants manquants puis propose de les enregistrer
   dans le Trousseau macOS. Le mot de passe n'est jamais écrit dans le fichier
   de configuration.

## Utilisation

```bash
macfind aa:bb:cc:dd:ee:ff     # MAC complète, tout format
macfind ddeeff                # MAC partielle
macfind 192.168.20.15         # IP exacte → MAC
macfind 192.168.20.           # tout un sous-réseau (préfixe terminé par un point)
macfind imprimante            # nom d'hôte ou commentaire
macfind hikvision             # fabricant (utile quand l'appareil n'a pas de nom)
macfind leases                # tous les baux DHCP
macfind scan                  # balaie le sous-réseau du Mac
```

| Option | Effet |
|---|---|
| `-r, --router <nom>` | N'interroger que ce routeur |
| `--no-arp` | Baux DHCP uniquement |
| `--json` | Sortie JSON pour les scripts |
| `--ask` | Redemander les identifiants pour cette fois |
| `--no-prompt` | Ne poser aucune question : échouer si des identifiants manquent (scripts, application graphique) |

| Commande | Effet |
|---|---|
| `macfind init` | Crée le fichier de configuration |
| `macfind add-router <nom> --url <adresse> --user <compte> [--insecure]` | Teste la connexion, puis ajoute le routeur à la configuration et range le mot de passe dans le Trousseau |
| `macfind leases --group subnet` | Baux regroupés par sous-réseau (`server` : par serveur DHCP) |
| `macfind anomalies` | Liste les anomalies : plusieurs baux actifs pour une MAC, IP utilisée par deux MAC, équipement qui répond hors de ses baux |
| `macfind routers` | Liste les routeurs configurés |
| `macfind set-password <nom>` | Enregistre ou change les identifiants |
| `macfind forget <nom>` | Efface les identifiants enregistrés |

### Balayage du réseau local

`macfind scan` interroge toutes les adresses du sous-réseau auquel le Mac est
relié et liste celles qui répondent : IP, MAC, fabricant, et le nom d'hôte
quand le routeur le connaît par un bail. Il trouve aussi les équipements en
IP fixe qui n'ont jamais demandé de bail (statut « sans bail »), même s'ils
bloquent le ping. Compte environ deux secondes pour 254 adresses.

```bash
macfind scan                      # le sous-réseau du Mac
macfind scan 192.168.1.           # un sous-réseau précis (ou 192.168.1.0/24)
macfind scan -f dahua             # filtrer par fabricant, nom, MAC ou IP
macfind scan --json               # pour un script
```

Le balayage ne porte que sur le segment local : pour un autre sous-réseau,
`macfind 192.168.20.` liste ce que le routeur connaît. Sans routeur configuré
ou joignable, le balayage fonctionne quand même, sans les noms d'hôte.

### Fabricant

La colonne « Fabricant » vient de l'OUI, les trois premiers octets de la MAC,
comparé au registre public de l'IEEE embarqué dans le binaire : aucune requête
réseau, aucune adresse envoyée à un tiers. Avec `--json`, le champ s'appelle
`vendor`.

« (MAC privée) » désigne une adresse aléatoire, comme en utilisent par défaut
les iPhone, les Android et les Windows récents sur le Wi-Fi : le fabricant
n'est alors pas identifiable. `macfind privée` les liste toutes.

Le code de sortie vaut 1 quand une recherche ne trouve rien.
La variable `MACFIND_PASSWORD` remplace le Trousseau pour un test ponctuel.

## Application graphique (en préparation)

Une application macOS native (SwiftUI) affiche les baux DHCP et le balayage
local dans un tableau triable, avec recherche, et permet d'ajouter un
routeur sans passer par le Terminal. Elle embarque le binaire
`macfind` et lit sa sortie `--json` ; la configuration et le Trousseau sont
ceux de la ligne de commande.

```bash
scripts/build-app.sh             # construit dist/macfind.app
scripts/build-app.sh --install   # puis l'installe dans /Applications
```

Il faut Xcode (Swift 6) et Rust. L'application est signée « ad hoc », sans
compte développeur Apple : elle fonctionne sur le Mac qui l'a construite ;
aucune version téléchargeable n'est encore publiée.

## Dépannage

Les erreurs courantes (`timed out`, `handshake failure`, 401, 403) et leurs
solutions sont dans [docs/mikrotik.md](docs/mikrotik.md#dépannage).

## Développement

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

```
src/
  main.rs      ligne de commande, règles de recherche, affichage
  config.rs    configuration et Trousseau macOS
  routeros.rs  client de l'API REST RouterOS
  mac.rs       normalisation et détection des adresses MAC
  oui.rs       fabricant à partir de l'OUI
  scan.rs      balayage du sous-réseau local
data/oui.tsv   registre IEEE, régénéré par scripts/maj-oui.sh
```

- Contribuer : [CONTRIBUTING.md](CONTRIBUTING.md)
- Sécurité et signalement d'une faille : [SECURITY.md](SECURITY.md)
- Versions et publication : [docs/RELEASING.md](docs/RELEASING.md)
- Historique : [CHANGELOG.md](CHANGELOG.md)

## Licence

[MIT](LICENSE). La base des fabricants (`data/oui.tsv`) est dérivée du
[registre public de l'IEEE](https://standards-oui.ieee.org/).
