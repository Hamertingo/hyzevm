# Pacotes (Homebrew e winget)

Depois de publicar uma release (`./scripts/release.sh X.Y.Z`), gere os arquivos com os hashes reais:

```sh
./scripts/packaging.sh X.Y.Z      # escreve em packaging/out/
```

- **Homebrew**: crie um repositório `homebrew-hyzevm` no GitHub com `Formula/hyzevm.rb` (o arquivo gerado). Instalação: `brew install Hamertingo/hyzevm/hyzevm`.
- **winget**: abra um PR em `microsoft/winget-pkgs` com a pasta `winget/` em `manifests/h/Hamertingo/HyzeVM/X.Y.Z/`. Instalação: `winget install Hamertingo.HyzeVM`.

Só a publicação (criar o tap, abrir o PR, assinar o instalador) precisa de contas e certificados seus; o resto é gerado.
