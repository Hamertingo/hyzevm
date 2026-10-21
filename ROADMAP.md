# Roadmap

Ideias para o HyzeVM, em ordem de prioridade dentro de cada grupo. Marque `[x]` quando fizer.

## Ver o que o agente faz (maior impacto)

- [ ] **Gravação da sessão** — vídeo + linha do tempo de tudo que o agente fez, para revisar depois ("o que ele fez às 3h?").
- [ ] **Painel de atividade real** — ligar o painel do app ao registro verdadeiro de comandos, cliques e uploads do agente (hoje só existe no protótipo de design).
- [ ] **Snapshot automático antes de tarefas grandes** + botão "voltar ao ponto anterior" no app.

## Primeira experiência

- [ ] **Imagens prontas na release** — o CI publica as imagens `base` e `desktop`; o `create` baixa em vez de construir (de minutos para segundos).
- [ ] **Instalador assinado e notarizado no Mac** — sem aviso do Gatekeeper.
- [ ] **App para Windows e Linux** na release (hoje só o app de macOS Apple Silicon é publicado).
- [ ] **Testar de verdade em Linux (KVM) e Windows (WHPX)** — hoje só compilam no CI.

## Segurança e confiança

- [ ] **Limites por máquina** — lista de sites permitidos, limite de banda, bloquear ações destrutivas.
- [ ] **Cofre no chaveiro do sistema** (Keychain / Credential Manager) em vez de arquivo de texto.
- [ ] **Avisos no celular** — o pedido de ajuda (CAPTCHA/2FA) chega no celular, não só no Mac.

## Uso no dia a dia

- [ ] **Mais de um perfil de navegador por máquina** — uma conta de rede social por perfil.
- [ ] **Menu de barra com ações rápidas** — pausar tudo, ver quem precisa de ajuda sem abrir o app.
- [ ] **Testes automáticos de ponta a ponta no CI** — criar, pausar, snapshot, colar.
- [ ] **GPU / aceleração** — navegador mais rápido e menos sinais de VM.

## Decidido: fora por enquanto

- **Compartilhar a tela com pessoas de fora** (túnel + link) — não agora.
- **Agendador dentro do app** — não faz sentido: quem decide *o que* fazer e *quando* é o agente (a IA), não o app.
  O HyzeVM é o computador, não o cérebro. Quem quiser rotina ("toda segunda às 9h") agenda no próprio agente
  (cron, o agendador do Claude Code/Codex etc.); a máquina já acorda sozinha quando o agente a usa (`exec`
  retoma uma máquina pausada) e o modo 24 h mantém ela de pé. Só reavaliar se um dia o HyzeVM passar a *rodar* agentes.
