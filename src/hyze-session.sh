#!/bin/sh
# Sessão do desktop HyzeVM: papel de parede em degradê + dock estilo Mac (tint2, centralizada, arredondada).
export DISPLAY=:99
mkdir -p "$HOME/.cache" "$HOME/.config/tint2" "$HOME/.local/share/applications"
[ -f "$HOME/.cache/hyze-wall.png" ] || convert -size 1280x800 gradient:'#16213e-#533a7b' "$HOME/.cache/hyze-wall.png"
feh --bg-fill "$HOME/.cache/hyze-wall.png"

# Atalhos da dock. O Chromium abre o MESMO perfil/porta de depuração que o `hyzevm browser` usa.
A="$HOME/.local/share/applications"
cat > "$A/hyze-chromium.desktop" <<'DE'
[Desktop Entry]
Type=Application
Name=Chromium
Icon=chromium
Exec=chromium --user-data-dir=%h/.config/hyze-chromium --remote-debugging-port=9222 --no-first-run --no-default-browser-check --password-store=basic %U
DE
cat > "$A/hyze-terminal.desktop" <<'DE'
[Desktop Entry]
Type=Application
Name=Terminal
Icon=utilities-terminal
Exec=xterm -fa Monospace -fs 11 -bg #14161c -fg #d7dae0 -geometry 110x32
DE
cat > "$A/hyze-files.desktop" <<'DE'
[Desktop Entry]
Type=Application
Name=Arquivos
Icon=system-file-manager
Exec=pcmanfm
DE

cat > "$HOME/.config/tint2/tint2rc" <<RC
rounded = 20
border_width = 1
border_sides = TBLR
background_color = #ffffff 22
border_color = #ffffff 45

panel_items = L
panel_size = 244 84
panel_margin = 0 14
panel_padding = 14 12 14
panel_background_id = 1
panel_position = bottom center horizontal
panel_layer = top
panel_dock = 0
wm_menu = 0
autohide = 0
launcher_padding = 10 0 14
launcher_background_id = 0
launcher_icon_size = 56
launcher_icon_asb = 100 0 0
launcher_icon_theme = Adwaita
launcher_item_app = $A/hyze-chromium.desktop
launcher_item_app = $A/hyze-terminal.desktop
launcher_item_app = $A/hyze-files.desktop
RC
picom --backend xrender --no-fading-openclose -b 2>/dev/null
exec tint2 -c "$HOME/.config/tint2/tint2rc"
