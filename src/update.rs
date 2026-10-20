//! Atualização: consulta as Releases do GitHub, verifica o SHA-256 e troca o binário (CLI) ou o .app (macOS).
use crate::store::{home, now};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "Hamertingo/hyzevm";
const CACHE_SECS: u64 = 6 * 3600;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub notes: String,
    pub page: String,
    pub assets: Vec<(String, String)>, // (nome, url)
    pub prerelease: bool,
}

#[derive(Serialize, Deserialize)]
struct Cache { checked_at: u64, latest: Option<Release> }

/// "v0.1.2" / "0.1.2-beta" → (0,1,2)
fn parse_ver(v: &str) -> (u64, u64, u64) {
    let core = v.trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
    let mut it = core.split('.').map(|x| x.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}
pub fn is_newer(candidate: &str, current: &str) -> bool { parse_ver(candidate) > parse_ver(current) }

fn api_url() -> String {
    // HYZEVM_UPDATE_API existe para testar com uma "release" local.
    std::env::var("HYZEVM_UPDATE_API").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases?per_page=5"))
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    let o = Command::new("curl").args(["-fsSL", "--max-time", "20", "-H", "User-Agent: hyzevm-updater", "-H", "Accept: application/vnd.github+json", url]).output().context("curl não encontrado")?;
    if !o.status.success() { bail!("não consegui falar com o GitHub ({})", String::from_utf8_lossy(&o.stderr).trim()); }
    Ok(o.stdout)
}

/// Release mais nova (inclui pré-lançamentos, que é o caso do alfa) — mesmo que não seja mais nova que a instalada.
pub fn fetch_latest() -> Result<Option<Release>> {
    let body = http_get(&api_url())?;
    let v: serde_json::Value = serde_json::from_slice(&body)?;
    let list = if v.is_array() { v.as_array().cloned().unwrap_or_default() } else { vec![v] };
    let mut best: Option<Release> = None;
    for r in list.iter().filter(|r| !r["draft"].as_bool().unwrap_or(false)) {
        let tag = r["tag_name"].as_str().unwrap_or("").to_string();
        if tag.is_empty() { continue; }
        let rel = Release {
            version: tag.trim_start_matches('v').to_string(), tag,
            notes: r["body"].as_str().unwrap_or("").to_string(),
            page: r["html_url"].as_str().unwrap_or("").to_string(),
            prerelease: r["prerelease"].as_bool().unwrap_or(false),
            assets: r["assets"].as_array().map(|a| a.iter().filter_map(|x| Some((x["name"].as_str()?.to_string(), x["browser_download_url"].as_str()?.to_string()))).collect()).unwrap_or_default(),
        };
        if best.as_ref().is_none_or(|b| is_newer(&rel.version, &b.version)) { best = Some(rel); }
    }
    Ok(best)
}

fn cache_file() -> PathBuf { home().join("update.json") }
fn read_cache() -> Option<Cache> { serde_json::from_slice(&std::fs::read(cache_file()).ok()?).ok() }

/// Consulta o GitHub (no máximo a cada 6 h, a menos que `force`) e devolve a release SE for mais nova que a instalada.
pub fn check(force: bool) -> Result<Option<Release>> {
    let cached = read_cache().filter(|c| !force && now().saturating_sub(c.checked_at) < CACHE_SECS);
    let latest = match cached { Some(c) => c.latest, None => {
        let l = fetch_latest()?;
        let _ = std::fs::create_dir_all(home());
        let _ = std::fs::write(cache_file(), serde_json::to_vec(&Cache { checked_at: now(), latest: l.clone() })?);
        l
    } };
    Ok(latest.filter(|r| is_newer(&r.version, CURRENT)))
}

/// Só lê o cache (nunca vai à rede): para avisos baratos no terminal.
pub fn cached_newer() -> Option<Release> { read_cache()?.latest.filter(|r| is_newer(&r.version, CURRENT)) }
pub fn cache_stale() -> bool { read_cache().is_none_or(|c| now().saturating_sub(c.checked_at) >= CACHE_SECS) }

fn arch_tag() -> &'static str { if std::env::consts::ARCH == "aarch64" { "arm64" } else { "x64" } }

/// Nome do arquivo do CLI para este sistema.
pub fn cli_asset_name() -> String {
    match std::env::consts::OS {
        "macos" => format!("hyzevm-macos-{}.tar.gz", arch_tag()),
        "windows" => "hyzevm-windows-x64.zip".to_string(),
        _ => format!("hyzevm-linux-{}.tar.gz", arch_tag()),
    }
}
/// Nome do arquivo do app (só macOS Apple Silicon por enquanto).
pub fn app_asset_name() -> Option<String> {
    (std::env::consts::OS == "macos" && std::env::consts::ARCH == "aarch64").then(|| "HyzeVM-macos-arm64.zip".to_string())
}

fn sha256_file(p: &Path) -> Result<String> {
    let mut h = Sha256::new();
    h.update(std::fs::read(p)?);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Baixa um asset da release para `dir` e confere o SHA-256 contra o SHA256SUMS.txt publicado.
fn download_verified(rel: &Release, name: &str, dir: &Path) -> Result<PathBuf> {
    let url = rel.assets.iter().find(|(n, _)| n == name).map(|(_, u)| u.clone()).with_context(|| format!("a release {} não tem o arquivo {name}", rel.tag))?;
    let dest = dir.join(name);
    let st = Command::new("curl").args(["-fsSL", "--max-time", "600", "-o"]).arg(&dest).arg(&url).status().context("curl não encontrado")?;
    if !st.success() { bail!("falha ao baixar {name}"); }
    let sums_url = rel.assets.iter().find(|(n, _)| n == "SHA256SUMS.txt").map(|(_, u)| u.clone()).context("a release não publicou SHA256SUMS.txt — não instalo sem verificar")?;
    let sums = String::from_utf8_lossy(&http_get(&sums_url)?).to_string();
    let want = sums.lines().find_map(|l| { let mut p = l.split_whitespace(); let h = p.next()?; let n = p.next()?.trim_start_matches('*'); (n == name).then(|| h.to_lowercase()) })
        .with_context(|| format!("{name} não está no SHA256SUMS.txt"))?;
    let got = sha256_file(&dest)?;
    if got != want { let _ = std::fs::remove_file(&dest); bail!("checksum de {name} não confere (esperado {want}, veio {got}) — abortei"); }
    Ok(dest)
}

fn temp_dir() -> Result<PathBuf> {
    let d = home().join("tmp-update");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn extract(archive: &Path, to: &Path) -> Result<()> {
    // tar (bsdtar no macOS/Windows 10+, GNU no Linux) abre .tar.gz e, no bsdtar, .zip
    let st = Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(to).status().context("tar não encontrado")?;
    if !st.success() { bail!("não consegui extrair {}", archive.display()); }
    Ok(())
}

/// Troca o binário em execução pelo da release (verificado). Funciona no macOS/Linux; no Windows renomeia o antigo.
pub fn install_cli(rel: &Release) -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let dir = temp_dir()?;
    let arc = download_verified(rel, &cli_asset_name(), &dir)?;
    extract(&arc, &dir)?;
    let new = dir.join(if cfg!(windows) { "hyzevm.exe" } else { "hyzevm" });
    if !new.exists() { bail!("o pacote não contém o binário esperado"); }
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?; }
    let staged = exe.with_extension("new");
    std::fs::copy(&new, &staged).with_context(|| format!("sem permissão de escrita em {} (tente com sudo)", exe.parent().unwrap().display()))?;
    #[cfg(windows)] { let old = exe.with_extension("old.exe"); let _ = std::fs::remove_file(&old); std::fs::rename(&exe, &old)?; }
    std::fs::rename(&staged, &exe)?;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(cache_file()); // próxima checagem reconsulta
    Ok(exe)
}

/// macOS: baixa e extrai o novo .app; devolve o caminho do .app novo (a troca é feita por `swap_app_and_relaunch`).
pub fn download_app(rel: &Release) -> Result<PathBuf> {
    let name = app_asset_name().context("o app só tem atualização automática no macOS (Apple Silicon) por enquanto")?;
    let dir = temp_dir()?;
    let zip = download_verified(rel, &name, &dir)?;
    let st = Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&dir).status().context("ditto não encontrado")?;
    if !st.success() { bail!("não consegui extrair o app"); }
    let app = dir.join("HyzeVM.app");
    if !app.exists() { bail!("o pacote não contém HyzeVM.app"); }
    Ok(app)
}

/// Caminho do .app em execução (…/HyzeVM.app) a partir do executável.
pub fn running_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(|p| p.to_path_buf())
}

/// Agenda a troca do .app depois que este processo sair e reabre o app novo.
pub fn swap_app_and_relaunch(new_app: &Path, pid: u32) -> Result<()> {
    let target = running_bundle().context("não estou rodando de dentro de um .app")?;
    let parent = target.parent().unwrap();
    // Precisa poder escrever na pasta que contém o app (ex.: /Applications ou a pasta do usuário).
    let probe = parent.join(".hyzevm-write-test");
    std::fs::write(&probe, b"").with_context(|| format!("sem permissão para atualizar em {} — baixe a versão nova em {}", parent.display(), format!("https://github.com/{REPO}/releases/latest")))?;
    let _ = std::fs::remove_file(probe);
    let script = home().join("tmp-update/swap.sh");
    std::fs::write(&script, "#!/bin/sh\nwhile kill -0 \"$1\" 2>/dev/null; do sleep 0.3; done\nrm -rf \"$3.old\"\nmv \"$3\" \"$3.old\" && mv \"$2\" \"$3\" && rm -rf \"$3.old\"\nxattr -cr \"$3\" 2>/dev/null\nopen \"$3\"\n")?;
    let mut c = Command::new("sh");
    c.arg(&script).arg(pid.to_string()).arg(new_app).arg(&target).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)] { use std::os::unix::process::CommandExt; c.process_group(0); }
    c.spawn()?;
    Ok(())
}
