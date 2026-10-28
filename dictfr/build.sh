#!/usr/bin/env bash
# Construit <dossier>/build-rs/main_<langue>.dict pour chaque langue demandée, à partir
# des corpus bruts téléchargés au besoin dans <dossier>/sources/.
# Utilisable depuis n'importe quel dossier : bash dictfr/build.sh fr en_US en_GB
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --quiet --manifest-path dictfr/Cargo.toml
BIN=dictfr/target/release/dictfr
if [ $# -eq 0 ]; then
	"$BIN"
fi

for lang; do
	list=$("$BIN" sources "$lang")
	while IFS=$'\t' read -r url dest extra; do
		src=$(dirname "$dest")
		mkdir -p "$src"
		if [ "$url" = - ]; then
			if [ ! -s "$dest" ]; then
				echo "Source manquante : $dest" >&2
				echo "$extra" >&2
				exit 1
			fi
			continue
		fi
		[ -s "$dest" ] || wget -q --show-progress -O "$dest" "$url"
		if [ "$extra" != - ] && [ ! -s "$src/$extra" ]; then
			tar -xzf "$dest" -C "$src" "$extra"
		fi
	done <<<"$list"
	"$BIN" build "$lang"
done
