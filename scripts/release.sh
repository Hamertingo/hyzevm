#!/bin/sh
# Uso: ./scripts/release.sh 0.1.1  — sobe a versão, commita, cria a tag e dispara a release (CI publica os binários e o app).
set -e
cd "$(dirname "$0")/.."
V="$1"; [ -n "$V" ] || { echo "uso: $0 <versão, ex.: 0.1.1>"; exit 2; }
[ -z "$(git status --porcelain)" ] || { echo "há mudanças não commitadas — commite antes"; exit 1; }
perl -0pi -e "s/^version = \"[0-9.]+\"/version = \"$V\"/m" Cargo.toml app/Cargo.toml
grep -q "^version = \"$V\"" Cargo.toml && grep -q "^version = \"$V\"" app/Cargo.toml || { echo "não consegui atualizar a versão"; exit 1; }
cargo update -w -q
git add -A && git commit -q -m "v$V"
git tag "v$V"
git push -q && git push -q origin "v$V"
echo "v$V enviada — acompanhe em: gh run watch"
