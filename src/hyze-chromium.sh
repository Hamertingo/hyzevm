#!/bin/sh
# Chromium do desktop do HyzeVM: mesmo perfil e porta de depuração que o `hyzevm browser` usa,
# no idioma do computador do usuário (/etc/hyze-lang).
L=$(cat /etc/hyze-lang 2>/dev/null || echo en-US)
P=$(cat /etc/hyze-proxy 2>/dev/null) # firewall de saída (hyzevm net)
exec chromium --user-data-dir="$HOME/.config/hyze-chromium" --remote-debugging-port=9222 \
  --no-first-run --no-default-browser-check --password-store=basic \
  ${P:+--proxy-server="$P"} --lang="$L" --accept-lang="$L,${L%%-*},en-US,en" "$@"
