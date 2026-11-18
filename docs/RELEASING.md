# Publier une version

Le projet suit le [versionnage sémantique](https://semver.org/lang/fr/) :

| Changement | Exemple | Numéro |
|---|---|---|
| Correction sans changement d'usage | bug d'affichage | `0.1.0` → `0.1.1` |
| Nouvelle fonction compatible | recherche du fabricant | `0.1.1` → `0.2.0` |
| Changement incompatible (option renommée, format de config) | | `0.x` → `1.0.0`, puis `1.x` → `2.0.0` |

Tant que la version est en `0.x`, un changement incompatible peut passer
dans une version mineure ; il doit alors être signalé dans le changelog.

## Au quotidien

- `main` reste toujours compilable ; le travail se fait sur des branches
  (`feat/…`, `fix/…`) fusionnées par pull request.
- Chaque changement visible par l'utilisateur ajoute une ligne sous
  `## [Non publié]` dans `CHANGELOG.md` (rubriques Ajouté, Modifié, Corrigé, Supprimé).
- Messages de commit au format [Conventional Commits](https://www.conventionalcommits.org/fr/) :
  `feat: …`, `fix: …`, `docs: …`, `chore: …`.

## Étapes d'une publication

1. Mettre à jour `version` dans `Cargo.toml`, puis `cargo build` pour
   rafraîchir `Cargo.lock`.
2. Dans `CHANGELOG.md`, renommer `## [Non publié]` en `## [X.Y.Z] - AAAA-MM-JJ`,
   recréer une section `## [Non publié]` vide et mettre à jour les liens du bas.
3. Valider et pousser :
   ```bash
   git commit -am "chore: version X.Y.Z"
   git push
   ```
4. Poser l'étiquette et la pousser :
   ```bash
   git tag -a vX.Y.Z -m "macfind X.Y.Z"
   git push origin vX.Y.Z
   ```

L'étiquette déclenche le workflow **Release** : il vérifie qu'elle correspond
à la version de `Cargo.toml`, compile les binaires macOS (Apple Silicon et
Intel) et crée la release GitHub avec les archives et leurs sommes SHA-256.
