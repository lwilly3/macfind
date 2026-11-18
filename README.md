# macfind

[![CI](https://github.com/lwilly3/macfind/actions/workflows/ci.yml/badge.svg)](https://github.com/lwilly3/macfind/actions/workflows/ci.yml)

Retrouve l'adresse IP d'un équipement à partir de son adresse MAC (ou l'inverse)
en interrogeant directement les routeurs MikroTik : baux DHCP et table ARP,
par l'API REST de RouterOS v7.

```console
$ macfind abcd
┌───────────┬────────┬───────────────┬───────────────────┬─────────┬──────────────────────────────┬────────────────────────┐
│ Routeur   ┆ Source ┆ IP            ┆ MAC               ┆ Hôte    ┆ Serveur / Interface          ┆ Statut                 │
╞═══════════╪════════╪═══════════════╪═══════════════════╪═════════╪══════════════════════════════╪════════════════════════╡
│ principal ┆ DHCP   ┆ 192.168.1.157 ┆ EC:EB:B8:12:AB:CD ┆ serveur ┆ serveurDHCPLocal (dynamique) ┆ bound (vu il y a 1m7s) │
└───────────┴────────┴───────────────┴───────────────────┴─────────┴──────────────────────────────┴────────────────────────┘
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
macfind leases                # tous les baux DHCP
```

| Option | Effet |
|---|---|
| `-r, --router <nom>` | N'interroger que ce routeur |
| `--no-arp` | Baux DHCP uniquement |
| `--json` | Sortie JSON pour les scripts |
| `--ask` | Redemander les identifiants pour cette fois |

| Commande | Effet |
|---|---|
| `macfind init` | Crée le fichier de configuration |
| `macfind routers` | Liste les routeurs configurés |
| `macfind set-password <nom>` | Enregistre ou change les identifiants |
| `macfind forget <nom>` | Efface les identifiants enregistrés |

Le code de sortie vaut 1 quand une recherche ne trouve rien.
La variable `MACFIND_PASSWORD` remplace le Trousseau pour un test ponctuel.

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
```

- Versions et publication : [docs/RELEASING.md](docs/RELEASING.md)
- Historique : [CHANGELOG.md](CHANGELOG.md)

## Licence

[MIT](LICENSE)
