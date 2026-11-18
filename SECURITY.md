# Sécurité

## Signaler une faille

N'ouvre pas d'issue publique. Utilise le signalement privé de GitHub :
onglet **Security** du dépôt, puis **Report a vulnerability**
([lien direct](https://github.com/lwilly3/macfind/security/advisories/new)).

Indique la version concernée, les étapes pour reproduire le problème et ses
conséquences possibles. Seule la dernière version publiée reçoit des
correctifs.

## Ce que macfind fait de tes identifiants

- Le mot de passe du routeur n'est jamais écrit dans le fichier de
  configuration. Il est demandé au lancement, puis enregistré dans le
  Trousseau macOS seulement si tu l'acceptes.
- La variable `MACFIND_PASSWORD` sert aux essais ponctuels. Évite-la dans un
  script partagé ou un historique de shell.
- macfind n'envoie de requêtes qu'aux routeurs listés dans ta configuration.
  La recherche du fabricant utilise une base embarquée : aucune adresse MAC
  ne quitte ton poste.
- La commande `scan` envoie un datagramme UDP vide à chaque adresse du
  sous-réseau local et lit la table ARP du Mac. Ne l'utilise que sur un
  réseau que tu administres ou avec l'accord de son responsable.

## Recommandations

- Crée sur le routeur un compte dédié en lecture seule
  (voir [docs/mikrotik.md](docs/mikrotik.md)) plutôt que d'utiliser `admin`.
- Restreins le service `www-ssl` du routeur à ton réseau d'administration.
- `insecure = true` accepte n'importe quel certificat : la connexion est
  chiffrée, mais l'identité du routeur n'est pas vérifiée. Réserve ce réglage
  à un réseau de confiance, ou installe un certificat reconnu par ton Mac.
