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

## Persistência dos dados

- Tudo da máquina vive em `~/.hyzevm/machines/<nome>/disk.qcow2` (+ `state` de pausa em disco). Sobrevive a pausar, desligar, reiniciar o app/Mac e a queda do QEMU. Some só com `hyzevm rm`/"Remover…".
- O disco é uma **cópia independente** da imagem (`clone_file`: `cp -c` no APFS = instantâneo e sem gastar espaço; `--reflink=auto` no Linux; cópia comum no Windows). Antes (≤ v0.1.1) era uma camada sobre `images/tpl-*.qcow2`/`debian-12-*.qcow2`: apagar ou refazer a imagem quebrava/corrompia as máquinas. `ensure_standalone` achata máquinas antigas no próximo boot (testado: dados preservados; imagem-base removida e a máquina continua ligando).
- Perfil do Chromium em `~/.config/hyze-chromium` (dentro do disco). Atenção ao bug antigo do atalho da dock (`%h`): máquinas criadas antes da correção abriam o Chromium com perfil em `/.config` (inexistente) = sem persistência; corrigir com os arquivos `hyze-chromium`/`hyze-session` e reabrir o navegador.
- Cache de disco `writeback` respeita flush do guest; `stop --force`/`quit` equivale a puxar o cabo (ext4 recupera pelo journal). Volumes (`/data/<nome>`) e snapshots são arquivos independentes.

## Teclado e área de transferência na VM (controle assumido)

- **Caps Lock não é encaminhado**: o case já vem em `e.key` e x11vnc aplica Shift; encaminhar a tecla invertia duas vezes (letras saíam minúsculas). `screen_ws` desliga um Caps preso na VM ao conectar.
- **Mac → Linux**: ⌘ vira Ctrl (⌘A/C/X/Z/F/T/L…), ⌘←/→ = Home/End, ⌘↑/↓ = Ctrl+Home/End, ⌘⌫ apaga a linha, ⌥←/→ e ⌥⌫ = por palavra; ⌘ sozinho não é enviado. Com o controle, só ⌘K, ⌘⎋ e ⌘⇧S continuam sendo do app; o resto vai para a VM.
- **Colar/copiar**: a ponte de seleção do x11vnc não funciona (cut text não vira seleção). ⌘V → `clipboard_to_vm` (arboard no Mac → `xclip -i` na VM via SSH) + Ctrl+V; ⌘C/⌘X → Ctrl+C/X + `clipboard_from_vm` (`xclip -o` → arboard). Testado na VM real: acentos, japonês, aspas e `$HOME` chegam intactos.
- **App sem menu Editar** (só HyzeVM → sobre/ocultar/sair; macOS), senão ⌘V/⌘C/⌘A são consumidos pelo menu nativo e nunca chegam ao JS. Campos do próprio app (⌘K, console) tratam ⌘V/C/X/A/Z em JS via `clipboard_read/write`.
- Não verificado na janela real: acentos por tecla morta (⌥E+A) e IME; o noVNC manda o keysym Unicode e o x11vnc remapeia.

## Atualização e fuso/idioma

- `src/update.rs`: consulta `GET /repos/Hamertingo/hyzevm/releases` (inclui pré-lançamentos), cache de 6 h em `~/.hyzevm/update.json`, baixa o asset, **confere SHA-256 contra `SHA256SUMS.txt` (sem isso não instala)**. CLI: `hyzevm update [--check]` troca o binário; aviso discreto em `ls`/visão geral (cache; se velho, `_checkupdate` em segundo plano). App: pílula `vX.Y.Z` no topo → folha → `update_install` baixa `HyzeVM-macos-arm64.zip`, agenda `swap.sh` (espera o PID sair, troca o .app, `xattr -cr`, `open`) e fecha. Só macOS arm64 troca o app sozinho; nos demais abre a página de download. `HYZEVM_UPDATE_API` aponta para uma release falsa (teste).
- Release: `./scripts/release.sh 0.1.1` (bump em Cargo.toml e app/Cargo.toml, tag, push) → CI publica 5 binários, `HyzeVM-macos-arm64.zip` e `SHA256SUMS.txt`.
- Fuso/idioma: copiados do computador na criação (`store::host_timezone/host_lang`) → cloud-init `timezone:` + `/etc/hyze-lang`; Chromium usa `--lang`/`--accept-lang` (helper `/usr/local/bin/hyze-chromium`, mesmo perfil/porta do `hyzevm browser`). `hyzevm locale <m>` ajusta uma existente. Bug corrigido: o atalho do Chromium na dock usava `%h` (não existe em .desktop → perfil em `/.config`).

## App no Windows

CI (`app-windows`) gera `HyzeVM-windows-x64-setup.exe` (NSIS, `installer/windows.nsi`, por usuário em `%LOCALAPPDATA%\Programs\HyzeVM`, atalhos no Menu Iniciar/Desktop) com o app (`hyzevm-desktop.exe`) + a CLI (`hyzevm.exe`). NUNCA chamar o app de `hyzevm.exe`/`HyzeVM.exe`: FS case-insensitive (mesma colisão do macOS). `hide_child_consoles` (app/src/main.rs) cria console oculto para ssh/qemu-img filhos não piscarem janelas. **Não verificado:** build e execução no Windows (nada rodou lá); sem auto-update do app no Windows.

## Ask (pergunte ao humano)

`src/ask.rs`: `hyzevm ask send <m> "pergunta" [--options a,b] [--wait SEG]` · `ask wait <m> <id>` · `ask ls [m]` · `ask answer <m> <id> <texto>` (exit 2 = sem resposta ainda). MCP: `ask_human` + `ask_wait`. Cada pergunta é `machines/<m>/asks/<id>.json` (resposta gravada no próprio arquivo; com `--options` só aceita as opções). Notifica como o takeover. App: `Dto.asks` + comando `ask_answer`; cartões fixos no canto (`#asks`, `renderAsks` só redesenha quando a lista muda para não perder o texto digitado). **Verificado:** CLI (enviar, opção inválida, esperar, responder, timeout exit 2) e cartões no preview com mock. **Não verificado:** a janela real do Tauri e o MCP em uso. Próximo: trava de ações (`guard`), ver ROADMAP.

## Controle do agente, registro e segurança (v0.1.3)

- `activity.rs`: `machines/<m>/activity.jsonl` (rotaciona em 4 MB). `activity::log(m, kind, detail)` é chamado nas funções da lib (desktop, browser, power, snapshot, ask, takeover, guard…) e em `exec/cp` da CLI/MCP; `set_actor("cli"|"mcp"|"app"|"watch"|"net")` no início de cada processo. **Nunca registre texto digitado** (pode ser senha): só o tamanho. CLI `activity`, MCP `activity_list`.
- `record.rs`: ffmpeg x11grab no guest, 5 fps, mp4 fragmentado em trechos de 5 min `~/.hyze/rec/<epoch>.mp4` (o nome liga a ação ao vídeo). `record start|stop|ls|get|auto on|off [--keep 3d]`; o vigia (`maintain`, a cada ~5 min) religa a gravação e apaga o que passou da retenção. `clip()` recorta ±segundos sem recodificar (o app toca em base64). Desligada por padrão.
- `guard.rs`: `approve` (exit 0 aprovado, 3 recusado, 2 sem resposta, 4 ensaio), `guard set --require publish,delete,pay,upload [--dry-run]`. `enforce` barra por fora: `exec` destrutivo (`guard::destructive`) e `browser upload`. Usa o mesmo mecanismo do `ask`.
- `checkpoint.rs`: snapshots INTERNOS do qcow2 (`ck-<nome>`), criados com a máquina ligada via QMP `blockdev-snapshot-internal-sync` (device `virtio0`); `restore` desliga → `qemu-img snapshot -a` → liga. `checkpoint auto on` = 1/dia (guarda 7). Testado: escrever → ponto → alterar → voltar.
- `keychain.rs` + `vault.rs`: `vault.enc` (AES-256-GCM); chave no Keychain (`security`), DPAPI (PowerShell, `vault.key`) ou `secret-tool`; sem chaveiro cai para `vault.json` com aviso; `Vault::migrate()` converte o texto antigo. Protege o arquivo em disco, não de um programa rodando como você.
- `net.rs`: firewall de saída. `net set --allow a.com,b.com [--limit-kbps N]` → o QEMU sobe com `restrict=on` + `guestfwd` 10.0.2.100:3128 → proxy do host (`hyzevm _netproxy`, iniciado ANTES do QEMU em `qemu::start`) que só libera os domínios da lista; a VM recebe `http(s)_proxy`, `/etc/hyze-proxy` (Chromium) e apt. Bloqueios viram `net-blocked` no registro. Vale no próximo boot.
- `watch.rs`: detector de VM travada (SSH a cada ~1 min, 4 falhas → `system_reset`), manutenção da gravação, alerta de tela parada 3 h, resumo diário (`summary.rs`), ponto automático opcional.
- `clone`, `resize`, `snapshot export|import`, `trim` (+ `discard=unmap` no disco), `--profile` do navegador (pasta `hyze-chromium-<perfil>`, porta CDP 9300+), `notify set <tópico>` (ntfy; só avisa).
- App: ⌘J = atividade (Ações/Vídeo/Pontos/Trava); bandeja (`tray.html`, janela "tray" sem borda; fechar a janela principal só esconde; `set_focused`/`take_drop_message` para arrastar arquivos → `~/Uploads`).
- Testes: `tests/e2e.sh` (CI job `e2e-linux` com KVM). **Não verificado:** bandeja/arrastar na janela real, Windows/Linux de ponta a ponta, medição do limite de banda, travamento real do detector.

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
