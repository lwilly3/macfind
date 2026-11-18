# Contribuer à macfind

Merci de l'intérêt que tu portes au projet. Les rapports de bogue, les idées
et les pull requests sont les bienvenus.

## Signaler un problème

Ouvre une [issue](https://github.com/lwilly3/macfind/issues) en précisant :

- la version (`macfind --version`) et celle de macOS ;
- le modèle du routeur et sa version de RouterOS ;
- la commande lancée et le message obtenu.

**Avant de coller une sortie, retire ce qui identifie ton réseau** : adresses
MAC, noms de machines, adresses publiques, noms de comptes. Ne publie jamais
de mot de passe.

Pour une faille de sécurité, n'ouvre pas d'issue publique : suis
[SECURITY.md](SECURITY.md).

## Proposer une modification

1. Crée une branche depuis `main` : `feat/…` pour une fonction, `fix/…` pour
   une correction.
2. Vérifie ton code avant chaque commit :
   ```bash
   cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
   ```
   La CI rejoue ces trois commandes sur chaque pull request.
3. Ajoute une ligne sous `## [Non publié]` dans [CHANGELOG.md](CHANGELOG.md)
   pour tout changement visible par l'utilisateur.
4. Rédige tes messages au format
   [Conventional Commits](https://www.conventionalcommits.org/fr/) :
   `feat: …`, `fix: …`, `docs: …`, `chore: …`.
5. Ouvre la pull request vers `main`.

## Règles du projet

- **Français** pour les messages, l'aide en ligne de commande et la
  documentation.
- **Aucune donnée réelle** dans le code, les tests ou la documentation : ni
  mot de passe, ni adresse MAC, ni nom de machine d'un vrai réseau. Utilise
  des valeurs fictives comme `00:15:5D:00:00:01`.
- **Tests sans réseau** : la CI n'a accès à aucun routeur. Les tests portent
  sur des structures construites à la main.
- **TLS** : le projet utilise `native-tls`. Ne le remplace pas par `rustls`,
  qui échoue à négocier avec certains routeurs MikroTik.
- `data/oui.tsv` est généré par `scripts/maj-oui.sh` : ne le modifie pas à la
  main.

## Versions

Le projet suit le versionnage sémantique. La marche à suivre pour publier est
dans [docs/RELEASING.md](docs/RELEASING.md).

## Licence

En contribuant, tu acceptes que ton travail soit publié sous la
[licence MIT](LICENSE) du projet.
