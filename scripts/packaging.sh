#!/bin/sh
# Gera a fórmula do Homebrew e os manifestos do winget para uma release já publicada.
# Uso: ./scripts/packaging.sh 0.1.2   → escreve em packaging/out/ (publique no tap/winget-pkgs; veja packaging/README.md)
set -e
cd "$(dirname "$0")/.."
V="$1"; [ -n "$V" ] || { echo "uso: $0 <versão>"; exit 2; }
REPO="Hamertingo/hyzevm"
SUMS=$(curl -fsSL "https://github.com/$REPO/releases/download/v$V/SHA256SUMS.txt")
sum() { echo "$SUMS" | awk -v f="$1" '$2==f {print $1}'; }
OUT=packaging/out; rm -rf "$OUT"; mkdir -p "$OUT/winget"
MAC_ARM=$(sum hyzevm-macos-arm64.tar.gz); MAC_X64=$(sum hyzevm-macos-x64.tar.gz)
LIN_X64=$(sum hyzevm-linux-x64.tar.gz); LIN_ARM=$(sum hyzevm-linux-arm64.tar.gz)
WIN=$(sum HyzeVM-windows-x64-setup.exe)
cat > "$OUT/hyzevm.rb" <<RB
class Hyzevm < Formula
  desc "A Linux computer of its own for every AI agent (CLI + MCP)"
  homepage "https://github.com/$REPO"
  version "$V"
  license "MIT"
  depends_on "qemu"

  on_macos do
    on_arm do
      url "https://github.com/$REPO/releases/download/v$V/hyzevm-macos-arm64.tar.gz"
      sha256 "$MAC_ARM"
    end
    on_intel do
      url "https://github.com/$REPO/releases/download/v$V/hyzevm-macos-x64.tar.gz"
      sha256 "$MAC_X64"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/$REPO/releases/download/v$V/hyzevm-linux-arm64.tar.gz"
      sha256 "$LIN_ARM"
    end
    on_intel do
      url "https://github.com/$REPO/releases/download/v$V/hyzevm-linux-x64.tar.gz"
      sha256 "$LIN_X64"
    end
  end

  def install
    bin.install "hyzevm"
  end

  test do
    assert_match "hyzevm", shell_output("#{bin}/hyzevm --version")
  end
end
RB
cat > "$OUT/winget/Hamertingo.HyzeVM.yaml" <<Y
PackageIdentifier: Hamertingo.HyzeVM
PackageVersion: $V
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
Y
cat > "$OUT/winget/Hamertingo.HyzeVM.installer.yaml" <<Y
PackageIdentifier: Hamertingo.HyzeVM
PackageVersion: $V
InstallerType: nullsoft
Scope: user
InstallerSwitches:
  Silent: /S
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/$REPO/releases/download/v$V/HyzeVM-windows-x64-setup.exe
    InstallerSha256: $(echo "$WIN" | tr a-f A-F)
ManifestType: installer
ManifestVersion: 1.6.0
Y
cat > "$OUT/winget/Hamertingo.HyzeVM.locale.en-US.yaml" <<Y
PackageIdentifier: Hamertingo.HyzeVM
PackageVersion: $V
PackageLocale: en-US
Publisher: Hamertingo
PackageName: HyzeVM
License: MIT
ShortDescription: A Linux computer of its own for every AI agent
PackageUrl: https://github.com/$REPO
ManifestType: defaultLocale
ManifestVersion: 1.6.0
Y
echo "ok → $OUT/ (Homebrew: copie hyzevm.rb para o seu tap; winget: abra um PR em microsoft/winget-pkgs com a pasta winget/)"
