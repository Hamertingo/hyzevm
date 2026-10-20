#!/bin/sh
# Uso: ./scripts/release.sh 0.1.1  — sobe a versão, commita, cria a tag e dispara a release (CI publica os binários e o app).
set -e
cd "$(dirname "$0")/.."
V="$1"; [ -n "$V" ] || { echo "uso: $0 <versão, ex.: 0.1.1>"; exit 2; }
[ -z "$(git status --porcelain)" ] || { echo "há mudanças não commitadas — commite antes"; exit 1; }
sed -i.bak -E "0,/^version = \"[0-9.]+\"/s//version = \"$V\"/" Cargo.toml && rm Cargo.toml.bak
sed -i.bak -E "0,/^version = \"[0-9.]+\"/s//version = \"$V\"/" app/Cargo.toml && rm app/Cargo.toml.bak
cargo update -w -q
git add -A && git commit -q -m "v$V"
git tag "v$V"
git push -q && git push -q origin "v$V"
echo "v$V enviada — acompanhe em: gh run watch"
