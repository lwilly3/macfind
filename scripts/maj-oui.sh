#!/bin/sh
# Régénère data/oui.tsv à partir du registre public de l'IEEE (MA-L).
# À lancer avant chaque publication : scripts/maj-oui.sh
#
# Format produit, trié par préfixe : 6 chiffres hexa, tabulation, fabricant.
set -eu

URL="https://standards-oui.ieee.org/oui/oui.csv"
racine=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Téléchargement de ${URL}…" >&2
curl --fail --silent --show-error --location --retry 3 \
    --user-agent "Mozilla/5.0 (macfind; maj-oui)" \
    --output "$tmp/oui.csv" "$URL"

python3 -I - "$tmp/oui.csv" "$tmp/oui.tsv" <<'PY'
import csv
import re
import sys

source, cible = sys.argv[1], sys.argv[2]

# Formes juridiques retirées en fin de nom pour garder des noms courts.
SUFFIXES = re.compile(
    r"[\s,]+("
    r"co\.?\s*,?\s*ltd\.?|ltd\.?|limited|inc\.?|incorporated|llc\.?|l\.l\.c\.|"
    r"corp\.?|corporation|gmbh|s\.?a\.?s\.?|s\.?p\.?a\.?|b\.?v\.?|pte\.?|pty\.?|"
    r"a/s|oy|ab|ag|kg|sia|\(pvt\)|pvt\.?|private|co\.?"
    r")$",
    re.IGNORECASE,
)


def nettoyer(nom):
    nom = " ".join(nom.split())
    while True:
        court = SUFFIXES.sub("", nom).rstrip(" ,.&-")
        if court == nom or len(court) < 2:
            break
        nom = court
    return nom


fabricants = {}
with open(source, newline="", encoding="utf-8") as f:
    for ligne in csv.DictReader(f):
        prefixe = ligne["Assignment"].strip().upper()
        nom = ligne["Organization Name"].strip()
        if not re.fullmatch(r"[0-9A-F]{6}", prefixe):
            continue
        # « Private » : l'attributaire a demandé à ne pas être publié.
        if not nom or nom.lower() == "private":
            continue
        fabricants.setdefault(prefixe, nettoyer(nom))

if len(fabricants) < 30000:
    sys.exit(f"registre incomplet : {len(fabricants)} préfixes seulement")

with open(cible, "w", encoding="utf-8", newline="\n") as f:
    for prefixe in sorted(fabricants):
        f.write(f"{prefixe}\t{fabricants[prefixe]}\n")

print(f"{len(fabricants)} préfixes", file=sys.stderr)
PY

mv "$tmp/oui.tsv" "$racine/data/oui.tsv"
echo "data/oui.tsv mis à jour." >&2
