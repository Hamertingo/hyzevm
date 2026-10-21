<div align="center">

# HyzeVM

**A Linux computer of its own for every AI agent.**

Spin up a clean Debian machine in ~10 seconds. Your agent runs code, browses the web,
uploads files and posts content — you watch the screen and take over whenever it gets stuck.

[![ci](https://github.com/Hamertingo/hyzevm/actions/workflows/ci.yml/badge.svg)](https://github.com/Hamertingo/hyzevm/actions/workflows/ci.yml) ![license](https://img.shields.io/badge/license-MIT-black)

[Install](#install) · [Quick start](#quick-start) · [For agents](#for-agents) · [App](#desktop-app) · [Português](README.pt-BR.md)

<img src="docs/grid.jpg" width="760" alt="HyzeVM app: every machine live in a grid">

</div>

> **Status: alpha.** Built and verified on **macOS (Apple Silicon)**. Linux and Windows hosts are
> implemented but not yet tested — see [Platform support](#platform-support). Expect rough edges.

## Why

Agents need a real computer, not just a shell: a browser that stays logged in, a desktop to look at,
files, secrets, a way to ask a human for help. Cloud sandboxes do this, but they cost money, add latency
and put your logged-in sessions on someone else's machine.

HyzeVM does it **locally**. Every machine is a real VM (own kernel, root, Docker) powered by QEMU with
native acceleration, controlled by a single CLI, an MCP server and a small desktop app.

## Features

- **Fast** — `hyzevm create` takes ~10 s; pausing is instant (0% CPU) and resuming takes < 1 s.
- **Real desktop** for agents — Chromium, a dock, `xdotool`, ffmpeg. Persistent browser profile.
- **Browser by snapshot** — agents read the page as a list of elements (`[e3] button "Publish"`)
  instead of guessing pixels. Fewer tokens, fewer mistakes.
- **Human in the loop** — an agent that hits a CAPTCHA/2FA calls `takeover request`; you get a
  notification, take the screen, and hand it back.
- **MCP server** (`hyzevm mcp`) with 30+ tools, plus a ready-made **agent skill**.
- **Always-on mode** — comes up with your computer, restarts if it dies, never auto-pauses.
- **Snapshots, volumes, vault, port publishing**, view-only share links with tokens and expiry.
- **Desktop app** — all machines live in a grid, focus on one, `⌘K` for everything, right-click menus.
- **Single binary**, cross-platform core written in Rust.

## Install

Requirements: [QEMU](https://www.qemu.org/) and the OpenSSH client.

```bash
# macOS
brew install qemu
# Debian/Ubuntu
sudo apt install qemu-system qemu-utils qemu-efi-aarch64 openssh-client
# Windows
winget install --id SoftwareFreedomConservancy.QEMU -e
```

```bash
cargo install --git https://github.com/Hamertingo/hyzevm hyzevm
hyzevm doctor        # checks QEMU, firmware, acceleration, ssh
```

## Quick start

```bash
hyzevm create work                              # Debian 12, ready in ~10 s
hyzevm exec work -- 'uname -a && python3 --version'
hyzevm ssh work

hyzevm create web --template desktop --always-on   # with a screen and a browser
hyzevm view web                                    # watch it in your browser
hyzevm browser goto web https://example.com
hyzevm browser snapshot web
```

| Template | What you get | First build |
|---|---|---|
| `minimal` *(default)* | Debian 12, nothing else | none (~10 s) |
| `base` | + Docker, Node, Python, git, build tools | ~1 min, once |
| `desktop` | + Xvfb, dock, Chromium, ffmpeg, noVNC | ~1 min, once |

### Commands

```
create · ls · get · start · stop · pause · suspend · rm
exec · ssh · cp · files
browser (goto snapshot click type press scroll upload wait tabs)
desktop (screenshot click move drag type press scroll open cursor)
takeover (request wait done status) · view · share (create ls revoke)
snapshot · volume · vault · port · always-on · timeout · auto-suspend
up · service · menubar · skill · locale · update · doctor · mcp · completions
```

`hyzevm <command> --help` for details. `hyzevm completions zsh` for shell completion.

## For agents

**MCP** — add to your client (Claude Code, Cursor, Codex…):

```json
{ "mcpServers": { "hyzevm": { "command": "hyzevm", "args": ["mcp"] } } }
```

**Skill** — teaches an agent the whole workflow (create/reuse a machine, exec, browser loop,
when to ask a human, secrets, posting content):

```bash
hyzevm skill install        # → ~/.claude/skills/hyzevm/SKILL.md
```

Agents should call `request_takeover` on CAPTCHAs/2FA instead of trying to bypass them.

## Desktop app

A small app (Tauri) over the same core: a live grid of machines, focus on one, assume/return control,
`⌘K` palette (create machines, pause, capture, type…), right-click menus, `⌘I` info panel.

<div align="center"><img src="docs/context-menu.jpg" width="640" alt="Right-click menu on a machine"></div>

```bash
./scripts/bundle-macos.sh     # builds HyzeVM.app
```

## Updates

`hyzevm update` checks the latest GitHub Release, verifies its SHA-256 and replaces the binary. The macOS app
checks on its own (at most every 6 h): a small `v0.x.y` pill appears at the top — click it, **Update & restart**, done.

Machines also inherit your computer's **timezone and language** (so the browser doesn't say `UTC / en-US` on a
Brazilian IP); change it any time with `hyzevm locale <machine> [--tz …] [--lang …]`.

## 24/7

```bash
hyzevm create agent --template desktop --always-on
hyzevm service install        # launchd / systemd --user / Task Scheduler
```

If QEMU dies it comes back on its own; the macOS service also keeps the Mac from idle-sleeping.

## Platform support

| Host | Accelerator | Status |
|---|---|---|
| macOS (Apple Silicon) | HVF | ✅ built and tested |
| Linux | KVM | ⚠️ compiles in CI, not run end-to-end yet |
| Windows | WHPX | ⚠️ compiles in CI, not run end-to-end yet |

Guests are Linux only (Debian 12, arm64 on Apple Silicon, amd64 elsewhere).

## How it works

```
 hyzevm CLI ─┐
 MCP server ─┼─►  core (Rust) ──► QEMU (QMP) ──► Debian VM ◄─ ssh / cloud-init
 desktop app ┘        │                                └─ Xvfb · Chromium · x11vnc · noVNC
                      └─ vault · snapshots · watcher (auto-pause, 24/7) · screen gateway
```

Machines live in `~/.hyzevm`. Access is plain SSH over a per-machine loopback port with a per-machine key.
Screen sharing goes through a local gateway that checks tokens and enforces view-only on the guest side.
See [AGENTS.md](AGENTS.md) for internals and known pitfalls.

## Security notes

- Everything is local: no accounts, no telemetry, nothing listens beyond `127.0.0.1`.
- The vault stores secrets in plain text at `~/.hyzevm/vault.json` (mode `0600`) and writes them to
  `/etc/environment` inside the machine. Treat machines like any host that holds your credentials.
- Automating logins/posting on third-party sites may violate their terms; keep a human in the loop.

## Roadmap

Prebuilt images from CI · Linux/Windows validation · installers and signing · shareable screen links over a tunnel ·
more templates and machine sizes.

## License

[MIT](LICENSE)

## Agent control & safety / Controle e segurança

```sh
hyzevm ask send web "Post it?" --options yes,no --wait 600   # the agent asks, you answer in the app
hyzevm guard set web --require publish,delete,pay,upload     # the agent must get your OK first (approve)
hyzevm activity web                                          # everything it did, with timestamps
hyzevm record auto web on --keep 3d                          # optional screen recording, 3-day retention
hyzevm checkpoint save web before-bulk-upload                # live restore point; checkpoint restore to go back
hyzevm net set web --allow instagram.com,facebook.com        # outbound allow-list (applies on next boot)
hyzevm clone web web-2 · hyzevm summary · hyzevm notify set <topic>
```
