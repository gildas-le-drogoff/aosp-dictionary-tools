#!/usr/bin/env bash
# Construit fr/build-rs/main_fr.dict à partir des corpus bruts.
# Utilisable depuis n'importe quel dossier : bash dictfr/build.sh
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=fr/sources
OUT=fr/build-rs
mkdir -p "$SRC" "$OUT" fr/listes

fetch() { [ -s "$SRC/$2" ] || wget -q --show-progress -O "$SRC/$2" "$1"; }
fetch https://object.pouta.csc.fi/OPUS-OpenSubtitles/v2024/mono/fr.txt.gz opensubtitles2024_fr.txt.gz
fetch https://kaikki.org/dictionary/downloads/fr/fr-extract.jsonl.gz wiktionnaire_fr.jsonl.gz
for c in fra_news_2022_1M fra_news_2023_1M fra_news_2024_1M fra_wikipedia_2021_1M; do
	fetch "https://downloads.wortschatz-leipzig.de/corpora/$c.tar.gz" "$c.tar.gz"
	[ -s "$SRC/$c/$c-sentences.txt" ] || tar -xzf "$SRC/$c.tar.gz" -C "$SRC" "$c/$c-sentences.txt"
done

cargo build --release --quiet --manifest-path dictfr/Cargo.toml
dictfr/target/release/dictfr "$SRC" fr/listes "$OUT"
