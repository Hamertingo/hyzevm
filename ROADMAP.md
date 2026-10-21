# Roadmap

Ideias para o HyzeVM, em ordem de prioridade dentro de cada grupo. Marque `[x]` quando fizer.

## Primeiro: confiabilidade 24 h

- [ ] **Detector de VM travada** — o vigia (`_watch`) checa o SSH de tempos em tempos; se não responder, tenta `system_reset`, grava o motivo e avisa no app. Hoje ele só reinicia quando o QEMU cai.
- [ ] **Cofre no chaveiro do sistema** (Keychain / Credential Manager) em vez de `vault.json` em texto.
- [ ] **Testes de ponta a ponta no CI** — o runner Linux tem KVM: subir uma VM mínima, `exec`, pausar/retomar, snapshot, colar, a cada release.
- [ ] **Suspender em disco** — consertar a retomada travada (HVF/arm64, relógio virtual) ou esconder da CLI e deixar só o pausar, que é confiável.

## Agente com rédea curta

- [x] **Pergunte ao humano** (`hyzevm ask`, MCP `ask_human`, cartões no app) — feito.
- [ ] **Trava de ações irreversíveis** — `hyzevm approve` / `guard set --require-approval publish,delete,pay` (o agente declara a ação e espera o sim) + regras que o HyzeVM enforça por fora (sites permitidos, `rm -rf`, uploads).
- [ ] **Resumo diário** por máquina (o que fez, quanto ficou ligada, o que falhou) e alerta de comportamento estranho (CPU alta, erros seguidos, tela parada há horas).

## Ver o que o agente faz (maior impacto)

- [ ] **Registro de ações** — cada comando, clique, digitação, upload e navegação do agente, com horário.
- [ ] **Linha do tempo** — registro: clicar numa ação pula para o momento no vídeo ("o que ele fez às 3h?").
- [ ] **Painel de atividade real** no app, ligado a esse registro (hoje só existe no protótipo de design).

## Desempenho

- [ ] **Imagens prontas na release** — o CI publica `base` e `desktop`; o `create` baixa em vez de construir (de ~1 min para ~15 s).
- [ ] **Miniaturas pelo VNC** em vez de screenshot por SSH a cada 4 s: menos CPU na VM e no Mac com várias máquinas abertas.
- [ ] **Liberar espaço do disco** — `discard` + `fstrim` para o qcow2 não só crescer; mostrar uso real de disco no app.
- [ ] **Limites de CPU/RAM por máquina** (tamanhos) para uma VM não deixar o Mac lento.
- [ ] **Boot mais rápido** (microvm / kernel direto).
- [ ] **GPU / aceleração** — navegador mais rápido (WebGL por software como paliativo).

## Primeira experiência / distribuição

- [ ] **Instalador assinado e notarizado no Mac** — sem aviso do Gatekeeper.
- [ ] **Windows**: instalador assinado, `winget`, atualização automática do app (o instalador `HyzeVM-windows-x64-setup.exe` já sai no CI, mas nunca rodou lá).
- [ ] **Linux**: app (AppImage/deb) e `brew` / pacote nativo.
- [ ] **Testar de verdade em Linux (KVM) e Windows (WHPX)** — hoje só compilam no CI.

## Segurança e confiança

- [ ] **Lista de sites permitidos** (firewall de saída) por máquina: o agente só acessa os domínios liberados.
- [ ] **Limite de banda** e bloqueio de ações destrutivas.
- [ ] **Avisos no celular** — o pedido de ajuda (CAPTCHA/2FA) chega no celular (ex.: ntfy), não só no Mac.

## Uso no dia a dia

- [ ] **Clonar máquina, exportar e importar** — uma máquina "modelo" já configurada e cópias instantâneas dela.
- [ ] **Arrastar arquivo para dentro da VM** no app (enviar o vídeo a postar sem usar `cp`).
- [ ] **Mais de um perfil de navegador por máquina** — uma conta de rede social por perfil.
- [ ] **Menu de barra completo e bonito, no Mac (menu bar) e no Windows (bandeja do sistema)** — hoje só existe um menu simples no macOS. Meta: um painel de verdade, com o mesmo visual do app, que dá para usar sem abrir a janela principal:
  - lista de máquinas com estado, miniatura ao vivo e uso de CPU/RAM; clicar abre a máquina no app;
  - ações rápidas por máquina (ligar, pausar, desligar, terminal, assumir/devolver) e globais (pausar tudo, ligar tudo);
  - perguntas do agente (`ask`) e pedidos de ajuda (takeover/CAPTCHA/2FA) aparecendo e respondíveis direto no painel, com badge no ícone;
  - ícone que muda de estado (tudo ok, agente precisa de você, erro, atualização disponível);
  - atalho global para abrir o painel, iniciar com o sistema, e claro/escuro seguindo o sistema;
  - Windows: ícone na bandeja com o mesmo painel (janela flutuante sem borda, ancorada perto do relógio), notificações nativas do Windows, o app continua rodando na bandeja ao fechar a janela;
  - mesma base de código para os dois (painel web do Tauri em janela pequena), em vez de menus nativos diferentes.

## Decidido: fora por enquanto

- **Compartilhar a tela com pessoas de fora** (túnel + link) — não agora.
- **Agendador dentro do app** — não faz sentido: quem decide *o que* fazer e *quando* é o agente (a IA), não o app.
  O HyzeVM é o computador, não o cérebro. Quem quiser rotina ("toda segunda às 9h") agenda no próprio agente
  (cron, o agendador do Claude Code/Codex etc.); a máquina já acorda sozinha quando o agente a usa (`exec`
  retoma uma máquina pausada) e o modo 24 h mantém ela de pé. Só reavaliar se um dia o HyzeVM passar a *rodar* agentes.
