#[cfg(target_os = "macos")]
mod menubar;
#[cfg(target_os = "macos")]
mod menubar_view;

use hyzevm::{browser, desktop, doctor, mcp, ops, qemu, service, share, ssh, store, takeover, template, ui, vault, watch};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::io::Read;
use store::{parse_dur, Machine};

const HELP: &str = "\
{about}

\x1b[1mUSO\x1b[0m  hyzevm <comando> [opções]

\x1b[1mMÁQUINAS\x1b[0m
  create <nome>        Cria e liga uma máquina (--template minimal|base|desktop, --always-on)
  ls · get <nome>      Lista / detalha
  start · stop · suspend · rm
  always-on · timeout · auto-suspend · up · service

\x1b[1mNO DIA A DIA\x1b[0m
  exec <nome> -- cmd   Roda um comando    ·  ssh · cp · files
  browser …            Browser por snapshot (goto, snapshot, click, type, upload…)
  desktop …            Mouse/teclado/screenshot no desktop
  view · share         Assistir/controlar a tela (link com token)
  takeover …           O agente pede que você assuma (CAPTCHA, 2FA)

\x1b[1mDADOS\x1b[0m
  snapshot · volume · vault · port

\x1b[1mSISTEMA\x1b[0m
  menubar              Ícone na barra de menus do Mac
  doctor · mcp · completions

Use \x1b[1mhyzevm <comando> --help\x1b[0m para detalhes.  Exemplo:
  hyzevm create agente --template desktop --always-on
  hyzevm browser snapshot agente
";

#[derive(Parser)]
#[command(name = "hyzevm", version, about = "Uma máquina Linux própria para cada agente", help_template = HELP, disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Templates disponíveis
    Templates,
    /// Cria e liga uma máquina
    Create {
        name: String,
        #[arg(long, default_value = "minimal")] template: String,
        #[arg(long)] snapshot: Option<String>,
        #[arg(long, default_value_t = 4)] cpus: u32,
        #[arg(long, default_value_t = 4096)] memory_mb: u32,
        #[arg(long, default_value_t = 32)] disk_gb: u32,
        /// Publica porta: HOST:GUEST (repetível)
        #[arg(long = "port")] ports: Vec<String>,
        /// Metadados K=V (repetível)
        #[arg(long = "meta")] meta: Vec<String>,
        /// Prazo da máquina (ex.: 2h); ao vencer aplica --on-timeout
        #[arg(long)] timeout: Option<String>,
        #[arg(long, default_value = "pause")] on_timeout: String,
        /// Pausa após ociosidade (padrão 30m; 0 desliga)
        #[arg(long)] auto_suspend: Option<String>,
        /// Anexa volume persistente (monta em /data/<nome>; repetível)
        #[arg(long = "volume")] volumes: Vec<String>,
        /// Não escreve o vault nesta máquina
        #[arg(long)] no_vault: bool,
        /// 24/7: sobe com o host, reinicia se cair e nunca pausa por ociosidade
        #[arg(long)] always_on: bool,
        /// Não espera o boot terminar
        #[arg(long)] no_wait: bool,
    },
    /// Lista máquinas
    #[command(visible_alias = "list")]
    Ls { #[arg(long = "meta")] meta: Vec<String>, #[arg(long)] json: bool },
    /// Detalhes de uma máquina
    Get { name: String, #[arg(long)] json: bool },
    /// Liga (ou retoma, se estava pausada)
    #[command(alias = "resume")]
    Start { name: String },
    /// Desliga (ACPI); --force mata
    Stop { name: String, #[arg(long)] force: bool },
    /// Pausa ao vivo (instantânea; retoma ao usar)
    Pause { name: String },
    /// Suspende para disco (libera a RAM do host; retomada mais lenta)
    Suspend { name: String },
    /// Apaga a máquina
    #[command(visible_alias = "delete")]
    Rm { name: String },
    /// Roda um comando; sai com o código dele
    Exec { name: String, #[arg(last = true, required = true)] cmd: Vec<String> },
    /// Shell interativo
    Ssh { name: String },
    /// Copia arquivos: use <máquina>:caminho num dos lados
    Cp { from: String, to: String },
    /// Lista um diretório ou imprime um arquivo da máquina
    Files { name: String, path: Option<String> },
    /// Define/limpa o prazo da máquina (0 limpa)
    Timeout { name: String, duration: String, #[arg(long, default_value = "pause")] on_timeout: String },
    /// Pausa após ociosidade (0 desliga)
    AutoSuspend { name: String, duration: String },
    /// Snapshots (máquina parada)
    #[command(subcommand)]
    Snapshot(Snap),
    /// Volumes persistentes
    #[command(subcommand)]
    Volume(Vol),
    /// Portas publicadas
    #[command(subcommand)]
    Port(PortCmd),
    /// Segredos escritos em toda máquina
    #[command(subcommand)]
    Vault(VaultCmd),
    /// Computer use no desktop da máquina (template desktop)
    #[command(subcommand)]
    Desktop(Desk),
    /// Link de tela com token, expiração e somente-leitura (local)
    #[command(subcommand)]
    Share(ShareCmd),
    /// Browser por snapshot: o agente enxerga a página como elementos com referência (e1, e2…)
    #[command(subcommand)]
    Browser(Br),
    /// O agente pede que você assuma a tela (CAPTCHA, 2FA…)
    #[command(subcommand)]
    Takeover(Tk),
    /// Abre a tela da máquina no navegador (template desktop)
    View { name: String, #[arg(long)] no_open: bool },
    /// Liga/desliga o modo 24/7 de uma máquina
    AlwaysOn { name: String, #[arg(value_parser = ["on", "off"])] mode: String },
    /// Sobe as máquinas always-on (--wait: fica supervisionando)
    Up { #[arg(long)] wait: bool },
    /// Roda `hyzevm up` sozinho no login do host (launchd/systemd/Agendador)
    #[command(subcommand)]
    Service(Svc),
    /// Skill que ensina agentes a usar o hyzevm (Claude Code, Codex…)
    #[command(subcommand)]
    Skill(SkillCmd),
    /// Diagnóstico do ambiente
    Doctor { #[arg(long)] json: bool },
    /// Ícone na barra de menus do Mac (preview das telas ao passar o mouse)
    Menubar {
        #[arg(long, hide = true)] foreground: bool,
        /// Abre sozinho no login
        #[arg(long)] install: bool,
        #[arg(long)] uninstall: bool,
    },
    /// Servidor MCP (stdio)
    Mcp,
    /// Gera autocompletar para o shell (zsh, bash, fish, powershell)
    Completions { #[arg(value_parser = ["zsh", "bash", "fish", "powershell"])] shell: String },
    #[command(name = "_gateway", hide = true)]
    Gateway { port: u16 },
    #[command(name = "_watch", hide = true)]
    Watch { name: String },
}

#[derive(Subcommand)]
enum Svc {
    Install { #[arg(long)] print: bool },
    Uninstall,
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Instala em ~/.claude/skills/hyzevm (ou em --dir <pasta>)
    Install { #[arg(long)] dir: Option<String> },
    /// Imprime o SKILL.md
    Print,
}

#[derive(Subcommand)]
enum Snap {
    Save { name: String, #[arg(long = "name")] snap: String },
    Ls,
    Rm { snap: String },
}

#[derive(Subcommand)]
enum Vol {
    Create { name: String, #[arg(long, default_value_t = 10)] size_gb: u32 },
    Ls,
    Rm { name: String },
}

#[derive(Subcommand)]
enum PortCmd {
    /// Publica a porta do guest num endereço local
    Publish { name: String, port: u16, #[arg(long)] host: Option<u16> },
    Ls { name: Option<String> },
    Unpublish { name: String, port: u16 },
}

#[derive(Subcommand)]
enum VaultCmd {
    /// Valor via --value ou stdin
    Set { name: String, #[arg(long)] value: Option<String> },
    /// Arquivo escrito na home do guest
    SetFile { path: String, #[arg(long)] from: String },
    /// Importa linhas KEY=value
    Import { file: String },
    Ls,
    Rm { name: String },
    /// O que a máquina recebe (só nomes)
    Show { name: String },
    /// Escreve o vault na máquina agora
    Push { name: String },
}

#[derive(Subcommand)]
enum ShareCmd {
    /// Cria um link (somente-leitura por padrão)
    Create {
        name: String,
        /// Permite mouse e teclado
        #[arg(long)] control: bool,
        /// Validade (60s a 24h)
        #[arg(long, default_value = "1h")] ttl: String,
    },
    Ls { name: Option<String> },
    /// Revoga todos os links da máquina (ou um --token)
    Revoke { name: String, #[arg(long)] token: Option<String> },
}

#[derive(Subcommand)]
enum Br {
    /// Abre uma URL
    Goto { name: String, url: String },
    /// Lista elementos interativos + texto visível
    Snapshot { name: String },
    Click { name: String, #[arg(name = "ref")] r: String },
    Type { name: String, #[arg(name = "ref")] r: String, text: String, #[arg(long)] submit: bool },
    Press { name: String, keys: String },
    Scroll { name: String, direction: String, #[arg(default_value_t = 5)] amount: u32 },
    /// Envia arquivo (caminho NO GUEST) para um <input type=file>
    Upload { name: String, #[arg(name = "ref")] r: String, files: Vec<String> },
    /// Espera X ms ou um texto aparecer
    Wait { name: String, what: String },
    Tabs { name: String },
}

#[derive(Subcommand)]
enum Tk {
    /// Pede takeover: cria link de controle, notifica e registra o motivo
    Request { name: String, #[arg(long, default_value = "o agente precisa de você")] reason: String },
    /// Espera você devolver o controle
    Wait { name: String, #[arg(long, default_value_t = 600)] timeout: u64 },
    /// Devolve o controle ao agente (revoga o link)
    Done { name: String },
    Status { name: String },
}

#[derive(Subcommand)]
enum Desk {
    Screenshot { name: String, #[arg(short, long, default_value = "screenshot.png")] output: String },
    Click { name: String, x: i32, y: i32, #[arg(long, default_value = "left")] button: String, #[arg(long)] double: bool },
    Move { name: String, x: i32, y: i32 },
    Drag { name: String, x1: i32, y1: i32, x2: i32, y2: i32 },
    Type { name: String, text: String },
    Press { name: String, keys: String },
    Scroll { name: String, direction: String, #[arg(long, default_value_t = 5)] amount: u32 },
    Open { name: String, url: String },
    Cursor { name: String },
}

const SKILL_MD: &str = include_str!("../skills/hyzevm/SKILL.md");

pub fn parse_port(s: &str) -> Result<(u16, u16)> {
    let Some((h, g)) = s.split_once(':') else { bail!("porta “{s}”: use HOST:GUEST") };
    Ok((h.parse()?, g.parse()?))
}

pub fn parse_meta(v: &[String]) -> Result<BTreeMap<String, String>> {
    v.iter().map(|s| s.split_once('=').map(|(k, v)| (k.to_string(), v.to_string()))
        .with_context(|| format!("meta “{s}”: use K=V"))).collect()
}

fn open_browser(url: &str) {
    let r = if cfg!(target_os = "macos") { std::process::Command::new("open").arg(url).status() }
        else if cfg!(windows) { std::process::Command::new("cmd").args(["/c", "start", "", url]).status() }
        else { std::process::Command::new("xdg-open").arg(url).status() };
    let _ = r;
}

/// Carrega e já religa/retoma (auto-resume) e marca atividade.
fn live(name: &str) -> Result<Machine> {
    let m = Machine::load(name)?;
    let was_up = qemu::state(&m) == "running";
    ops::start(&m)?;
    if !was_up { ssh::wait_ssh(&m, 120)?; }
    Ok(m)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            let msg = format!("{e:#}");
            let hint = if msg.contains("não existe") && msg.contains("máquina") { Some("veja as máquinas com `hyzevm ls`") }
                else if msg.contains("qemu") && msg.contains("não encontrado") { Some("rode `hyzevm doctor` para ver o que falta") }
                else if msg.contains("sem tela") || msg.contains("template desktop") { Some("crie com: hyzevm create <nome> --template desktop") }
                else if msg.contains("SSH") { Some("a máquina pode estar iniciando — tente de novo em instantes") } else { None };
            ui::fail(&msg, hint);
            std::process::exit(1);
        }
    }
}

fn overview() -> Result<()> {
    println!("{}  {}", ui::bold("hyzevm"), ui::dim(&format!("v{} · uma máquina Linux própria para cada agente", env!("CARGO_PKG_VERSION"))));
    println!();
    let ms = store::list()?;
    if ms.is_empty() {
        println!("  Nenhuma máquina ainda.\n");
        println!("  {}  {}", ui::cyan("hyzevm create agente"), ui::dim("Debian 12 pronto em ~10 s"));
        println!("  {}  {}", ui::cyan("hyzevm create agente --template desktop --always-on"), ui::dim("com tela, 24 h"));
    } else { print_machines(&ms); println!(); println!("  {}", ui::dim("hyzevm --help para ver todos os comandos")); }
    Ok(())
}

fn print_machines(ms: &[Machine]) {
    let rows = ms.iter().map(|m| {
        let mut flags = vec![];
        if m.always_on { flags.push("24/7".to_string()); }
        if m.template == "desktop" { flags.push("tela".to_string()); }
        if takeover::pending(m).is_some() { flags.push(ui::yellow("● precisa de você")); }
        if !m.ports.is_empty() { flags.push(format!("{} porta(s)", m.ports.len())); }
        vec![ui::bold(&m.name), ui::state(qemu::state(m)), m.template.clone(),
             format!("{} CPU · {} GB", m.cpus, m.memory_mb / 1024), ui::dim(&flags.join(" · "))]
    }).collect::<Vec<_>>();
    ui::table(&["NOME", "ESTADO", "TEMPLATE", "RECURSOS", ""], &rows);
}

fn run() -> Result<i32> {
    let Some(cmd) = Cli::parse().cmd else { overview()?; return Ok(0); };
    match cmd {
        Cmd::Templates => for (n, d) in template::TEMPLATES { println!("{n}\t{d}") },
        Cmd::Create { name, template, snapshot, cpus, memory_mb, disk_gb, ports, meta, timeout, on_timeout,
                      auto_suspend, volumes, no_vault, always_on, no_wait } => {
            if !matches!(on_timeout.as_str(), "pause" | "kill") { bail!("--on-timeout: pause|kill"); }
            let o = ops::CreateOpts {
                name, template, snapshot, cpus, memory_mb, disk_gb,
                ports: ports.iter().map(|p| parse_port(p)).collect::<Result<_>>()?,
                wait: !no_wait, meta: parse_meta(&meta)?,
                auto_suspend: auto_suspend.as_deref().map(parse_dur).transpose()?,
                timeout: timeout.as_deref().map(parse_dur).transpose()?, on_timeout, volumes, vault: !no_vault, always_on,
            };
            let m = ops::create(o)?;
            ui::ok(&format!("{} pronta", ui::bold(&m.name)));
            ui::hint(&format!("hyzevm ssh {0}  ·  hyzevm exec {0} -- <cmd>", m.name));
            if m.template == "desktop" { ui::hint(&format!("hyzevm view {}  ·  hyzevm browser snapshot {}", m.name, m.name)); }
        }
        Cmd::Ls { meta, json } => {
            let want = parse_meta(&meta)?;
            let ms: Vec<Machine> = store::list()?.into_iter()
                .filter(|m| want.iter().all(|(k, v)| m.meta.get(k) == Some(v))).collect();
            if json {
                let v: Vec<_> = ms.iter().map(|m| serde_json::json!({"name": m.name, "state": qemu::state(m),
                    "template": m.template, "cpus": m.cpus, "memory_mb": m.memory_mb, "always_on": m.always_on, "meta": m.meta})).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if ms.is_empty() { ui::hint("nenhuma máquina — crie com `hyzevm create <nome>`"); }
            else { print_machines(&ms); }
        }
        Cmd::Get { name, json } => {
            let m = Machine::load(&name)?;
            if json { println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "name": m.name, "state": qemu::state(&m), "template": m.template, "cpus": m.cpus,
                "memory_mb": m.memory_mb, "disk_gb": m.disk_gb, "ssh_port": m.ssh_port, "ports": m.ports,
                "view": desktop::view_url(&m).ok(), "auto_suspend_secs": m.auto_suspend_secs,
                "always_on": m.always_on, "expires_at": m.expires_at, "on_timeout": m.on_timeout, "meta": m.meta, "volumes": m.volumes}))?); }
            else {
                println!("{}  {}", ui::bold(&m.name), ui::state(qemu::state(&m)));
                ui::kv("template", &m.template);
                ui::kv("recursos", &format!("{} CPU · {} MB RAM · {} GB disco", m.cpus, m.memory_mb, m.disk_gb));
                ui::kv("ssh", &format!("hyzevm ssh {}  {}", m.name, ui::dim(&format!("(127.0.0.1:{})", m.ssh_port))));
                if let Ok(u) = desktop::view_url(&m) { ui::kv("tela", &u); }
                for (h, g) in &m.ports { ui::kv("porta", &format!("http://127.0.0.1:{h} → {g}")); }
                ui::kv("24/7", if m.always_on { "sim (reinicia se cair)" } else { "não" });
                if m.auto_suspend_secs > 0 { ui::kv("ociosa", &format!("pausa após {}", ui::humanize_secs(m.auto_suspend_secs))); }
                if let Some(t) = takeover::pending(&m) { ui::kv("takeover", &ui::yellow(&format!("pendente: {}", t.reason))); }
                if !m.meta.is_empty() { ui::kv("meta", &m.meta.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")); }
            }
        }
        Cmd::Start { name } => { live(&name)?; ui::ok(&format!("{name} ligada")); }
        Cmd::Stop { name, force } => { qemu::stop(&Machine::load(&name)?, force)?; ui::ok(&format!("{name} desligada")); }
        Cmd::Pause { name } => { qemu::pause(&Machine::load(&name)?)?; ui::ok(&format!("{name} pausada (retoma na hora ao usar)")); }
        Cmd::Suspend { name } => { qemu::suspend(&Machine::load(&name)?)?; ui::ok(&format!("{name} suspensa em disco (memória e processos guardados)")); }
        Cmd::Rm { name } => { ops::remove(&Machine::load(&name)?)?; ui::ok(&format!("{name} removida")); }
        Cmd::Exec { name, cmd } => return ssh::exec_inherit(&live(&name)?, Some(&cmd.join(" ")), false),
        Cmd::Ssh { name } => return ssh::exec_inherit(&live(&name)?, None, true),
        Cmd::Cp { from, to } => {
            let name = [&from, &to].iter().find_map(|s| s.split_once(':').map(|x| x.0.to_string()));
            let Some(name) = name else { bail!("um lado precisa ser <máquina>:caminho") };
            ssh::copy(&live(&name)?, &from, &to)?;
        }
        Cmd::Files { name, path } => {
            let p = ssh::sh_quote(path.as_deref().unwrap_or("."));
            return ssh::exec_inherit(&live(&name)?, Some(&format!("if [ -d {p} ]; then ls -la {p}; else cat {p}; fi")), false);
        }
        Cmd::Timeout { name, duration, on_timeout } => {
            if !matches!(on_timeout.as_str(), "pause" | "kill") { bail!("--on-timeout: pause|kill"); }
            let mut m = Machine::load(&name)?;
            let d = parse_dur(&duration)?;
            m.expires_at = if d == 0 { None } else { Some(store::now() + d) };
            m.on_timeout = on_timeout;
            m.save()?;
            if qemu::is_running(&m) { watch::ensure(&m)?; }
            println!("{name}: prazo {}", if d == 0 { "removido".into() } else { format!("em {duration}") });
        }
        Cmd::AutoSuspend { name, duration } => {
            let mut m = Machine::load(&name)?;
            m.auto_suspend_secs = parse_dur(&duration)?;
            m.save()?;
            println!("{name}: auto-suspend {}", if m.auto_suspend_secs == 0 { "desligado".into() } else { duration });
        }
        Cmd::Snapshot(Snap::Save { name, snap }) => { ops::snapshot_save(&Machine::load(&name)?, &snap)?; ui::ok(&format!("snapshot {snap} salvo")); }
        Cmd::Snapshot(Snap::Ls) => for s in ops::snapshot_list() { println!("{s}") },
        Cmd::Snapshot(Snap::Rm { snap }) => ops::snapshot_remove(&snap)?,
        Cmd::Volume(Vol::Create { name, size_gb }) => { ops::volume_create(&name, size_gb)?; println!("volume {name} ({size_gb} GB) — use `create --volume {name}`"); }
        Cmd::Volume(Vol::Ls) => for v in ops::volume_list() { println!("{v}") },
        Cmd::Volume(Vol::Rm { name }) => ops::volume_remove(&name)?,
        Cmd::Port(PortCmd::Publish { name, port, host }) => {
            let mut m = Machine::load(&name)?;
            let h = ops::port_publish(&mut m, port, host)?;
            println!("http://127.0.0.1:{h} → {name}:{port}");
        }
        Cmd::Port(PortCmd::Ls { name }) => {
            let ms = match name { Some(n) => vec![Machine::load(&n)?], None => store::list()? };
            for m in ms { for (h, g) in &m.ports { println!("{}\t{g}\thttp://127.0.0.1:{h}", m.name); } }
        }
        Cmd::Port(PortCmd::Unpublish { name, port }) => ops::port_unpublish(&mut Machine::load(&name)?, port)?,
        Cmd::Vault(v) => match v {
            VaultCmd::Set { name, value } => {
                let val = match value { Some(v) => v, None => { let mut s = String::new(); std::io::stdin().read_to_string(&mut s)?; s.trim_end_matches(['\n', '\r']).to_string() } };
                vault::Vault::load().set_env(&name, &val)?;
                ui::ok(&format!("{name} guardado no vault"));
            }
            VaultCmd::SetFile { path, from } => { vault::Vault::load().set_file(&path, &std::fs::read(&from)?)?; println!("{path} guardado"); }
            VaultCmd::Import { file } => {
                let mut v = vault::Vault::load();
                let mut n = 0;
                for l in std::fs::read_to_string(&file)?.lines() {
                    let l = l.trim();
                    if l.is_empty() || l.starts_with('#') { continue; }
                    if let Some((k, val)) = l.split_once('=') { v.set_env(k.trim(), val.trim().trim_matches('"'))?; n += 1; }
                }
                println!("{n} itens importados");
            }
            VaultCmd::Ls => { let v = vault::Vault::load(); for k in v.env.keys() { println!("env\t{k}") } for k in v.files.keys() { println!("file\t~/{k}") } }
            VaultCmd::Rm { name } => vault::Vault::load().remove(&name)?,
            VaultCmd::Show { name } => {
                let m = Machine::load(&name)?;
                println!("vault {} nesta máquina", if m.vault { "ATIVO" } else { "desligado (--no-vault)" });
                let v = vault::Vault::load();
                for k in v.env.keys() { println!("env\t{k}") } for k in v.files.keys() { println!("file\t~/{k}") }
            }
            VaultCmd::Push { name } => { let n = vault::push(&live(&name)?)?; println!("{n} itens escritos"); }
        },
        Cmd::Desktop(d) => match d {
            Desk::Screenshot { name, output } => { let png = desktop::screenshot(&live(&name)?)?; std::fs::write(&output, &png)?; println!("{output} ({} bytes)", png.len()); }
            Desk::Click { name, x, y, button, double } => desktop::click(&live(&name)?, x, y, &button, double)?,
            Desk::Move { name, x, y } => desktop::mv(&live(&name)?, x, y)?,
            Desk::Drag { name, x1, y1, x2, y2 } => desktop::drag(&live(&name)?, (x1, y1), (x2, y2))?,
            Desk::Type { name, text } => desktop::type_text(&live(&name)?, &text)?,
            Desk::Press { name, keys } => desktop::press(&live(&name)?, &keys)?,
            Desk::Scroll { name, direction, amount } => desktop::scroll(&live(&name)?, &direction, amount)?,
            Desk::Open { name, url } => desktop::open(&live(&name)?, &url)?,
            Desk::Cursor { name } => println!("{}", desktop::cursor(&live(&name)?)?),
        },
        Cmd::View { name, no_open } => {
            let m = live(&name)?;
            let url = desktop::view_url(&m)?;
            println!("{url}");
            if !no_open { open_browser(&url); }
        }
        Cmd::Share(sc) => match sc {
            ShareCmd::Create { name, control, ttl } => {
                let m = live(&name)?;
                let (_, url, sh) = share::create(&m, !control, parse_dur(&ttl)?)?;
                println!("{url}");
                eprintln!("{} · expira em {}s", if sh.view_only { "somente-leitura" } else { "controle total" }, sh.expires_at - store::now());
            }
            ShareCmd::Ls { name } => for (t, s) in share::list(name.as_deref()) {
                println!("{}…\t{}\t{}\texpira em {}s", &t[..8], s.machine, if s.view_only { "view-only" } else { "controle" }, s.expires_at - store::now());
            },
            ShareCmd::Revoke { name, token } => {
                match token { Some(t) => share::revoke_token(&t)?, None => share::revoke_machine(&name) }
                println!("revogado");
            }
        },
        Cmd::Gateway { port } => share::run_gateway(port)?,
        Cmd::AlwaysOn { name, mode } => {
            let mut m = Machine::load(&name)?;
            m.always_on = mode == "on";
            if m.always_on { m.auto_suspend_secs = 0; }
            m.save()?;
            if m.always_on { ops::start(&m)?; }
            ui::ok(&format!("{name}: modo 24/7 {}", if mode == "on" { "ligado" } else { "desligado" }));
        }
        Cmd::Up { wait } => service::up(wait)?,
        Cmd::Service(Svc::Install { print }) => service::install(print)?,
        Cmd::Service(Svc::Uninstall) => service::uninstall()?,
        Cmd::Browser(b) => {
            let (name, args): (String, Vec<String>) = match b {
                Br::Goto { name, url } => (name, vec!["goto".into(), url]),
                Br::Snapshot { name } => (name, vec!["snapshot".into()]),
                Br::Click { name, r } => (name, vec!["click".into(), r]),
                Br::Type { name, r, text, submit } => (name, [vec!["type".into(), r, text], if submit { vec!["--submit".into()] } else { vec![] }].concat()),
                Br::Press { name, keys } => (name, vec!["press".into(), keys]),
                Br::Scroll { name, direction, amount } => (name, vec!["scroll".into(), direction, amount.to_string()]),
                Br::Upload { name, r, files } => (name, [vec!["upload".into(), r], files].concat()),
                Br::Wait { name, what } => (name, vec!["wait".into(), what]),
                Br::Tabs { name } => (name, vec!["tabs".into()]),
            };
            let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            println!("{}", browser::run(&live(&name)?, &refs)?);
        }
        Cmd::Takeover(t) => match t {
            Tk::Request { name, reason } => {
                let tk = takeover::request(&live(&name)?, &reason)?;
                println!("{}", tk.url);
                eprintln!("pedido enviado: {reason}\ndevolva o controle com: hyzevm takeover done {name}");
            }
            Tk::Wait { name, timeout } => {
                let done = takeover::wait(&Machine::load(&name)?, timeout)?;
                println!("{}", if done { "devolvido" } else { "esperando" });
                return Ok(if done { 0 } else { 2 });
            }
            Tk::Done { name } => { takeover::done(&Machine::load(&name)?)?; ui::ok("controle devolvido ao agente"); }
            Tk::Status { name } => match takeover::status(&Machine::load(&name)?) {
                Some(t) => println!("{}\t{}", if t.done { "devolvido" } else { "pendente" }, t.reason),
                None => println!("nenhum"),
            },
        },
        Cmd::Skill(SkillCmd::Print) => print!("{}", SKILL_MD),
        Cmd::Skill(SkillCmd::Install { dir }) => {
            let base = match dir { Some(d) => std::path::PathBuf::from(d), None => dirs::home_dir().context("sem home")?.join(".claude/skills") };
            let d = base.join("hyzevm");
            std::fs::create_dir_all(&d)?;
            std::fs::write(d.join("SKILL.md"), SKILL_MD)?;
            ui::ok(&format!("skill instalada em {}", d.display()));
            ui::hint("o agente passa a conhecer o hyzevm na próxima sessão");
        }
        Cmd::Doctor { json } => return Ok(doctor::run(json)),
        Cmd::Menubar { foreground, install, uninstall } => {
            #[cfg(target_os = "macos")]
            {
                if install { service::install_menubar()?; }
                else if uninstall { service::uninstall_menubar()?; }
                else if foreground { menubar::run()?; }
                else { menubar::launch()?; ui::ok("menu bar ativo — o ícone está na barra de menus"); ui::hint("para abrir sempre no login: hyzevm menubar --install"); }
            }
            #[cfg(not(target_os = "macos"))]
            { let _ = (foreground, install, uninstall); bail!("o menu bar existe só no macOS por enquanto"); }
        }
        Cmd::Mcp => return mcp::run(),
        Cmd::Completions { shell } => {
            use clap::CommandFactory;
            let sh = match shell.as_str() { "bash" => clap_complete::Shell::Bash, "fish" => clap_complete::Shell::Fish, "powershell" => clap_complete::Shell::PowerShell, _ => clap_complete::Shell::Zsh };
            clap_complete::generate(sh, &mut Cli::command(), "hyzevm", &mut std::io::stdout());
        }
        Cmd::Watch { name } => watch::run(&name)?,
    }
    Ok(0)
}
