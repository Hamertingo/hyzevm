# HyzeVM — uma máquina Linux própria para cada agente

**Visão:** um computador leve, rápido e fácil que fica ligado 24 h para o agente (logado nas redes do usuário, posta vídeos etc.).
Estilo InstaBox (https://instabox.sh/docs), mas **local**: cada agente ganha uma VM Linux com kernel e
root próprios, Docker e (template `desktop`) um desktop que o agente dirige e você assiste. Interface =
**CLI + MCP**, sem GUI. Rust + QEMU, para **macOS, Linux e Windows**. (O projeto Swift antigo foi removido.)

## Workspace

- `src/` — **lib `hyzevm`** (tudo: QEMU, SSH, MCP, browser…) + **CLI** `hyzevm` (`src/main.rs`; menubar só macOS fica no bin)
- `app/` — **app desktop (Tauri 2)**: `app/src/main.rs` = comandos Rust sobre a lib; `app/ui/` = HTML/CSS/JS sem build (sem node) com design estilo Apple (sidebar com vibrancy no macOS / mica no Windows, barra de título overlay, claro/escuro)
- `scripts/bundle-macos.sh` — monta `HyzeVM.app` (CLI em `Contents/SharedSupport/hyzevm`; NUNCA em `MacOS/`: FS case-insensitive colide com `HyzeVM`)
- Auxiliares (`_watch`, `_gateway`) são a mesma binária chamada com argumento; o app despacha isso no início do `main()`.
- Preview da UI sem o app: `python3 -m http.server --directory app/ui` + `mock.js` (dados falsos quando não há `window.__TAURI__`).

## App (design aprovado: grade + foco + ⌘K — `design/app.html`)

Minimalista, monocromático; cor só com significado (verde ligada, âmbar precisa de você, azul você no controle).
- **Grade**: todas as máquinas com miniatura ao vivo (JPEG 640×400 via `machine_thumb`, ~4–30 KB, a cada 4 s só com a janela visível); máquinas sem tela mostram um cartão de terminal com uso. Ações no hover (pausar/ligar, abrir).
- **Foco**: clique abre a máquina. Desktop = visor RFB (`screen.js`) em somente-leitura; **Assumir** (⌘↩) libera teclado/mouse, **Devolver** (⌘⎋) devolve e fecha o takeover. Sem tela = console por SSH. Pausada/desligada: retoma/liga ao abrir.
- **⌘K**: tudo (ações da máquina em foco, busca, abrir máquina ⌘1–9, pausar/remover qualquer uma). **Nova máquina** dentro do ⌘K: nome → ↑↓ modelo → ⇥ tamanho, chips de disco/24 h, ↵ cria; progresso no cartão + aviso.
- **Botão direito** em qualquer máquina (grade ou tela fora do controle): abrir, assumir/devolver, capturar, digitar, pausar/ligar, desligar, terminal, informações, 24 h, copiar SSH, remover. Com o controle assumido, o botão direito vai para a VM. ↑↓/↵/esc no menu.
- **⌘I**: informações (uso ao vivo a cada 3 s, recursos, 24 h, pausar se ociosa, ssh, terminal). ⌘⇧S captura. Esc volta.
- Janela sólida (sem vibrancy), luz/escuro pelo sistema. Preview sem app: `python3 -m http.server --directory app/ui` + `mock.js`.
**Verificado:** UI completa no navegador com mock (grade, foco, ⌘I, ⌘K, criação); app empacota, abre e faz as chamadas reais (miniatura/uso da tela1 só com a janela visível). **Não verificado visualmente:** a janela real do Tauri (sem permissão de captura) e Windows.

## Pausar × suspender (importante)

- **Pausar** (`pause`, app, MCP `machine_suspend`, ociosidade, timeout): QMP `stop` — QEMU continua aberto, CPU 0%, retoma em ~0,3 s com `cont`; o relógio do guest é acertado pelo host ao retomar (`sudo date -s @epoch`). Testado: 90 s pausada → relógio idêntico.
- **Suspender para disco** (`suspend`, só CLI): `savevm` + fecha o QEMU. **Bug medido**: após horas suspensa, a retomada (HVF/arm64) deixa o guest congelado (0% CPU, SSH sem banner) — provável relógio virtual. `ops::start` agora espera 25 s pelo SSH e, se não vier, faz `system_reset` (perde a RAM, preserva o disco) e grava `restore-error.txt`. Uma máquina (tela1) ficou com ext4 corrompido nesse cenário + desligamento forçado e foi recriada.

## Estrutura da lib (`src/`)

- `store.rs` — biblioteca `~/.hyzevm` (`$HYZEVM_HOME`): `machines/<nome>/`, `images/`, `snapshots/`, `volumes/`, `vault.json`
- `qemu.rs` — motor: HVF (mac) / KVM (linux) / WHPX (windows); QMP em TCP loopback; start/stop/suspend/hostfwd
- `template.rs` — base **Debian 12 genericcloud** (~340 MB, leve); templates `minimal`/`base`/`desktop` via cloud-init (seed NoCloud como vvfat label `cidata`)
- `ops.rs` — create (imagem de template cacheada), snapshots, volumes, portas — usado por CLI e MCP
- `ssh.rs` — exec/cp via SSH numa porta local por máquina (igual nos 3 SOs; sem vsock)
- `desktop.rs` — computer use: xdotool + ImageMagick no Xvfb `:99`; `view_url` → noVNC
- `service.rs` — `up [--wait]` supervisiona máquinas always-on; `service install` (launchd+caffeinate / systemd user / schtasks)
- `share.rs` — link de tela local: gateway em 127.0.0.1 (`hyzevm _gateway`) valida token/expiração e repassa ao noVNC; revogação derruba viewers abertos em ~2 s
- `browser.rs` + `cdp.py` — browser por snapshot: Chromium com CDP só no loopback do guest, script Python (stdlib) enviado por ssh; elementos viram refs `e1…`
- `takeover.rs` — `request_takeover`: link de controle + notificação; agente espera `takeover wait`
- `ui.rs` — cores (TTY/NO_COLOR), tabelas, ✓/▸/erro com dica · `menubar.rs` + `menubar_view.rs` (só macOS) — ícone na barra de menus; preview = NSImageView nativo (objc2) no 1º item do submenu (muda limita ícones a 18 pt)
- `vault.rs` · `watch.rs` (processo `hyzevm _watch`: vault no boot, timeout, auto-suspend) · `doctor.rs` · `mcp.rs` · `main.rs`

## Comandos

`templates` · `create <nome> [--template minimal|base|desktop] [--snapshot S] [--volume V] [--port H:G] [--meta K=V] [--timeout 2h --on-timeout pause|kill] [--auto-suspend 30m|0] [--no-vault] [--always-on]`
· `ls [--meta K=V] [--json]` · `get` · `start|resume` · `stop [--force]` · `suspend|pause` · `rm` · `exec <m> -- cmd` · `ssh` · `cp m:path` · `files`
· `timeout` · `auto-suspend` · `snapshot save|ls|rm` · `volume create|ls|rm` · `port publish|ls|unpublish`
· `vault set|set-file|import|ls|rm|show|push` · `desktop screenshot|click|move|drag|type|press|scroll|open|cursor`
· `browser goto|snapshot|click|type|press|scroll|upload|wait|tabs` · `takeover request|wait|done|status` · `menubar [--install|--uninstall]` · `completions <shell>` · `view <m>` (abre noVNC no navegador) · `share create <m> [--control] [--ttl 1h]|ls|revoke` (link com token, view-only por padrão) · `always-on <m> on|off` · `up [--wait]` · `service install [--print]|uninstall` · `doctor [--json]` · `mcp` (32 tools: machine_*, snapshot_*, desktop_*, browser_*, request_takeover/takeover_wait)

Uso de uma máquina (`exec/ssh/cp/desktop/view`) religa/retoma sozinha e adia o auto-suspend (padrão 30 min, como o InstaBox).

## 24/7

`create --always-on`: nunca auto-suspende, o vigia (`_watch`) reinicia o QEMU se cair (intent=up) e `hyzevm service install` sobe tudo no login do host.
Testado: matar o QEMU com -9 → volta em ≤15 s. `service install` só testado com `--print` (não instalado). Mac em tampa fechada dorme: use na tomada/sem sleep.
Chromium (perfil/logins ficam em `~/.config/chromium` no disco da VM e sobrevivem a stop/suspend). Redes sociais: automatizar login/postagem pode violar ToS e gerar bloqueio — usar APIs oficiais quando existirem.

## Desktop (template `desktop`)

Xvfb :99 1280x800 + Openbox + **dock estilo Mac** (tint2: pílula translúcida arredondada, centralizada embaixo; ícones Chromium/Terminal/Arquivos, tema Adwaita) + papel de parede em degradê (ImageMagick+feh) + picom. Tudo em `src/hyze-session.sh` (vai pro guest em `/usr/local/bin/hyze-session`, serviço `hyze-session`). O atalho do Chromium da dock usa o MESMO perfil/porta CDP do `hyzevm browser`. Plank foi tentado e descartado (desenha o painel fora da tela no Xvfb e usa dconf). Papirus descartado (209 MB); Adwaita 36 MB.

## Browser + takeover

`browser snapshot` devolve `[e1] button "Publicar" …` + texto visível; `click/type/upload` usam a ref (stale ref → erro pedindo novo snapshot). `upload` aceita caminho NO GUEST (envie o vídeo antes com `cp`). Perfil em `~/.config/hyze-chromium` (logins persistem). Testado: type + upload de arquivo + click num form → título da aba refletiu os 3.
Takeover testado: request (link de controle + notificação osascript) → wait → done (revoga o link).

## Verificado (macOS arm64, QEMU 11.1.2, HVF)

create **minimal (Debian puro): ~10 s**, RAM ociosa ~214 MB, retomar de suspend + exec 0,8 s · desktop (1ª vez ~1 min monta a imagem; Chromium 155 + ffmpeg + noVNC testados) · exec · suspend/resume com RAM e processos
· stop · snapshot + `--snapshot` · volume persistente entre máquinas · vault (env + arquivo) · port publish a quente
· auto-suspend · desktop (Firefox, screenshot, type/press/click) · `view` no navegador · MCP · doctor · `share`: token inválido 403, view-only IMPOSTO NO SERVIDOR (x11vnc -viewonly na 5901: ponteiro via link RO não move, via link de controle move), revogação fecha o WebSocket aberto em ~4 s.

**Não verificado:** Linux x86_64 (KVM) e Windows (WHPX) como host — código existe, nunca rodou lá; `timeout --on-timeout kill`;
`--timeout` com vencimento real. CI de release em `.github/workflows/release.yml` (não executado).

## Armadilhas

0. Gateway: fechar com `Shutdown::Both` deixando dados do cliente por ler vira RST e o Chrome perde a resposta (ERR_EMPTY_RESPONSE) — fecha só a escrita e drena. websockify derruba conexões em rajada: o gateway reconecta e reenvia até o 1º byte.
1. Disco novo nunca pode ser menor que o virtual size do backing (corta a partição raiz → initramfs) — `create_disk` ajusta.
2. QMP atende um cliente por vez: `qmp()` tenta de novo em caso de reset.
3. Imagem de template é achatada (`qemu-img convert`); máquinas usam seed "light" (só identidade/chave/volumes).
4. `vault set-file ~/x`: o shell expande `~` — o código devolve para caminho relativo à home do guest.
5. Segredos ficam em texto em `vault.json` (0600) e em `/etc/environment` do guest.

## Skill para agentes

`skills/hyzevm/SKILL.md` ensina o agente (Claude Code, Codex…) a usar a máquina: criar/reusar, exec/cp, browser por snapshot, takeover (nunca contornar CAPTCHA/2FA), vault, snapshots, fluxo de postagem. `hyzevm skill install [--dir]` copia para `~/.claude/skills/hyzevm`; `hyzevm skill print` imprime. Embutida no binário (`include_str!`).

## Atualização e fuso/idioma

- `src/update.rs`: consulta `GET /repos/Hamertingo/hyzevm/releases` (inclui pré-lançamentos), cache de 6 h em `~/.hyzevm/update.json`, baixa o asset, **confere SHA-256 contra `SHA256SUMS.txt` (sem isso não instala)**. CLI: `hyzevm update [--check]` troca o binário; aviso discreto em `ls`/visão geral (cache; se velho, `_checkupdate` em segundo plano). App: pílula `vX.Y.Z` no topo → folha → `update_install` baixa `HyzeVM-macos-arm64.zip`, agenda `swap.sh` (espera o PID sair, troca o .app, `xattr -cr`, `open`) e fecha. Só macOS arm64 troca o app sozinho; nos demais abre a página de download. `HYZEVM_UPDATE_API` aponta para uma release falsa (teste).
- Release: `./scripts/release.sh 0.1.1` (bump em Cargo.toml e app/Cargo.toml, tag, push) → CI publica 5 binários, `HyzeVM-macos-arm64.zip` e `SHA256SUMS.txt`.
- Fuso/idioma: copiados do computador na criação (`store::host_timezone/host_lang`) → cloud-init `timezone:` + `/etc/hyze-lang`; Chromium usa `--lang`/`--accept-lang` (helper `/usr/local/bin/hyze-chromium`, mesmo perfil/porta do `hyzevm browser`). `hyzevm locale <m>` ajusta uma existente. Bug corrigido: o atalho do Chromium na dock usava `%h` (não existe em .desktop → perfil em `/.config`).

## Roadmap

1. Imagens `base`/`desktop` prontas nos releases do CI (baixar em vez de construir)
2. Validar Linux x86_64/KVM e Windows/WHPX; empacotar (brew/winget/instalador) e assinar
3. (Decisão: **local-first** — sem túnel/URL pública; tudo em 127.0.0.1)
3. Templates novos (orca/herdr-like), tamanhos de máquina, equipes
4. Boot mais rápido (microvm), GPU

## Menu bar (macOS)

`hyzevm menubar` (detacha) / `--install` (LaunchAgent). Lista máquinas com estado, ligar/pausar/desligar/terminal, takeover pendente; preview (screenshot 5 s, só desktops RODANDO — não acorda pausadas) ao passar o mouse; clicar abre a tela.
Linha do preview só entra depois do 1º frame (antes aparecia um item vazio acima de "Abrir tela").
**Verificado:** compila, roda sem crash e instala a view. **Não verificado visualmente** (sem permissão de captura/acessibilidade no ambiente de teste) — conferir o ícone, o tamanho do preview e o clique.
