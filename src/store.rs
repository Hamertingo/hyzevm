//! Biblioteca de máquinas: ~/.hyzevm/machines/<nome>/ (ou $HYZEVM_HOME).
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Machine {
    pub name: String,
    pub template: String,
    pub cpus: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub ssh_port: u16,
    pub qmp_port: u16,
    /// (host, guest) — portas publicadas via redirecionamento.
    pub ports: Vec<(u16, u16)>,
    pub user: String,
    /// Porta local → noVNC (6080) do guest; 0 = máquina antiga sem visualização.
    #[serde(default)]
    pub view_port: u16,
    /// Porta local → noVNC somente-leitura (6081); x11vnc -viewonly garante no servidor.
    #[serde(default)]
    pub view_ro_port: u16,
    /// Pausa após esse tempo ocioso (0 = nunca). Padrão 30 min, como o InstaBox.
    #[serde(default = "default_auto_suspend")]
    pub auto_suspend_secs: u64,
    /// Instante (unix) em que `on_timeout` age; None = sem prazo.
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default = "default_on_timeout")]
    pub on_timeout: String,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    /// 24/7: sobe sozinha com o host (`hyzevm up`), reinicia se cair e nunca auto-suspende.
    #[serde(default)]
    pub always_on: bool,
    /// Fuso e idioma do guest (copiados do computador do usuário na criação).
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    /// Gravação automática da tela: guarda os últimos N segundos de vídeo (0 = desligada).
    #[serde(default)]
    pub record_keep_secs: u64,
    /// Ações que exigem sua aprovação antes (publish, delete, pay, upload, send… ou `*`).
    #[serde(default)]
    pub guard: Vec<String>,
    /// Ensaio: toda ação declarada pára antes de acontecer (o agente mostra o que faria).
    #[serde(default)]
    pub dry_run: bool,
    /// Firewall de saída: a VM só alcança estes sites (via proxy no host). Vazio = rede livre.
    #[serde(default)]
    pub net_allow: Vec<String>,
    /// Limite de banda da VM em KB/s (0 = sem limite; só vale com net_allow).
    #[serde(default)]
    pub net_limit_kbps: u64,
    #[serde(default)]
    pub net_proxy_port: u16,
    /// Ponto de restauração automático diário (guarda 7). Desligado por padrão.
    #[serde(default)]
    pub auto_checkpoint: bool,
    /// Escreve o vault nesta máquina no boot.
    #[serde(default = "yes")]
    pub vault: bool,
}

fn default_auto_suspend() -> u64 { 1800 }
fn default_on_timeout() -> String { "pause".into() }
fn yes() -> bool { true }

pub fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

/// "30m", "2h", "45s", "1d", "90" (segundos) ou "0".
pub fn parse_dur(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last() {
        Some('s') => (&s[..s.len() - 1], 1), Some('m') => (&s[..s.len() - 1], 60),
        Some('h') => (&s[..s.len() - 1], 3600), Some('d') => (&s[..s.len() - 1], 86400),
        _ => (s, 1),
    };
    Ok(num.parse::<u64>().with_context(|| format!("duração inválida “{s}” (ex.: 30m, 2h, 0)"))? * mult)
}

pub fn home() -> PathBuf {
    std::env::var_os("HYZEVM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("sem home").join(".hyzevm"))
}

pub fn machines_dir() -> PathBuf { home().join("machines") }
pub fn images_dir() -> PathBuf { home().join("images") }
pub fn snapshots_dir() -> PathBuf { home().join("snapshots") }
pub fn volumes_dir() -> PathBuf { home().join("volumes") }
pub fn vault_file() -> PathBuf { home().join("vault.json") }
/// Cofre cifrado (AES-256-GCM); a chave fica no chaveiro do sistema.
pub fn vault_enc_file() -> PathBuf { home().join("vault.enc") }
pub fn shares_file() -> PathBuf { home().join("shares.json") }
pub fn gateway_file() -> PathBuf { home().join("gateway.port") }

impl Machine {
    pub fn dir(&self) -> PathBuf { machines_dir().join(&self.name) }
    pub fn disk(&self) -> PathBuf { self.dir().join("disk.qcow2") }
    pub fn key(&self) -> PathBuf { self.dir().join("id_ed25519") }
    pub fn seed_dir(&self) -> PathBuf { self.dir().join("seed") }
    pub fn activity_file(&self) -> PathBuf { self.dir().join("activity") }
    pub fn intent_file(&self) -> PathBuf { self.dir().join("intent") }
    /// O que o usuário quer: "up" ou "down" (distingue queda de desligamento de propósito).
    pub fn set_intent(&self, up: bool) { let _ = std::fs::write(self.intent_file(), if up { "up" } else { "down" }); }
    pub fn wants_up(&self) -> bool { std::fs::read_to_string(self.intent_file()).map(|s| s.trim() == "up").unwrap_or(false) }
    pub fn watch_file(&self) -> PathBuf { self.dir().join("watch.alive") }
    /// Marca atividade (adia o auto-suspend).
    pub fn touch(&self) { let _ = std::fs::write(self.activity_file(), now().to_string()); }
    pub fn idle_secs(&self) -> u64 {
        let last = std::fs::read_to_string(self.activity_file()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        now().saturating_sub(last)
    }
    pub fn serial_log(&self) -> PathBuf { self.dir().join("serial.log") }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(self.dir())?;
        let tmp = self.dir().join("machine.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, self.dir().join("machine.json"))?; // atômico
        Ok(())
    }

    pub fn load(name: &str) -> Result<Self> {
        let p = machines_dir().join(name).join("machine.json");
        let Ok(b) = std::fs::read(&p) else { bail!("máquina “{name}” não existe") };
        Ok(serde_json::from_slice(&b)?)
    }

    pub fn remove(&self) -> Result<()> { Ok(std::fs::remove_dir_all(self.dir())?) }
}

pub fn list() -> Result<Vec<Machine>> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(machines_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with('_') { continue; } // máquinas internas (ex.: _build-desktop, o construtor de imagens)
            if let Ok(m) = Machine::load(&n) { v.push(m); }
        }
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(v)
}

pub fn valid_name(n: &str) -> Result<()> {
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("nome inválido “{n}” (use letras, números, - e _)");
    }
    Ok(())
}

/// Porta TCP livre no loopback.
pub fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// Fuso horário do computador (ex.: America/Sao_Paulo). None se não der para descobrir.
pub fn host_timezone() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") { if tz.contains('/') { return Some(tz.trim_start_matches(':').to_string()); } }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let s = link.to_string_lossy();
    s.split("zoneinfo/").nth(1).map(|z| z.to_string()).filter(|z| z.contains('/') || z == "UTC")
}

/// Idioma do computador no formato BCP-47 (ex.: pt-BR). None se não der para descobrir.
pub fn host_lang() -> Option<String> {
    let raw = if cfg!(target_os = "macos") {
        std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else { ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX")) }?;
    let base = raw.split(['.', '@']).next()?.replace('_', "-");
    let ok = base.len() >= 2 && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    ok.then_some(base)
}
