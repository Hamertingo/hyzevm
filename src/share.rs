//! Link de tela compartilhável (local): token + expiração + somente-leitura.
//! Um gateway em 127.0.0.1 valida o token e repassa ao noVNC do guest.
//! O somente-leitura é imposto no guest (x11vnc -viewonly na 5901), não só no navegador.
use crate::{ops, store::*};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone)]
pub struct Share {
    pub machine: String,
    pub view_only: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

type Shares = BTreeMap<String, Share>;

fn load() -> Shares {
    std::fs::read(shares_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(s: &Shares) -> Result<()> {
    std::fs::create_dir_all(home())?;
    let tmp = shares_file().with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?; }
    Ok(std::fs::rename(tmp, shares_file())?)
}

fn token() -> Result<String> {
    let mut b = [0u8; 24];
    getrandom::getrandom(&mut b).map_err(|e| anyhow::anyhow!("sem aleatoriedade: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

pub fn valid(token: &str) -> Option<Share> {
    load().get(token).filter(|s| s.expires_at > now()).cloned()
}

pub fn list(machine: Option<&str>) -> Vec<(String, Share)> {
    load().into_iter().filter(|(_, s)| s.expires_at > now() && machine.is_none_or(|m| m == s.machine)).collect()
}

pub fn revoke_machine(name: &str) {
    let mut s = load();
    let n = s.len();
    s.retain(|_, v| v.machine != name);
    if s.len() != n { let _ = save(&s); }
}

pub fn revoke_token(t: &str) -> Result<()> {
    let mut s = load();
    if s.remove(t).is_none() { bail!("token não encontrado"); }
    save(&s)
}

/// Cria um link. `ttl_secs` fica entre 60 s e 24 h (como o InstaBox).
pub fn create(m: &Machine, view_only: bool, ttl_secs: u64) -> Result<(String, String, Share)> {
    if m.template != "desktop" || m.view_port == 0 || m.view_ro_port == 0 {
        bail!("{} não tem tela compartilhável (crie de novo com --template desktop)", m.name);
    }
    let ttl = ttl_secs.clamp(60, 86400);
    let mut s = load();
    s.retain(|_, v| v.expires_at > now()); // limpa vencidos
    let t = token()?;
    let sh = Share { machine: m.name.clone(), view_only, created_at: now(), expires_at: now() + ttl };
    s.insert(t.clone(), sh.clone());
    save(&s)?;
    let port = ensure_gateway()?;
    let url = format!("http://127.0.0.1:{port}/s/{t}/vnc.html?autoconnect=1&resize=scale&reconnect=1&path=s/{t}/websockify{}",
        if view_only { "&view_only=1" } else { "" });
    Ok((t, url, sh))
}

// ---------- gateway ----------

fn gateway_alive(port: u16) -> bool {
    TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(), Duration::from_millis(300)).is_ok()
}

/// Porta do gateway; sobe `hyzevm _gateway` (destacado) se ainda não existir.
pub fn ensure_gateway() -> Result<u16> {
    if let Some(p) = std::fs::read_to_string(gateway_file()).ok().and_then(|s| s.trim().parse::<u16>().ok()) {
        if gateway_alive(p) { return Ok(p); }
    }
    let port = free_port()?;
    std::fs::create_dir_all(home())?;
    std::fs::write(gateway_file(), port.to_string())?;
    let mut c = Command::new(std::env::current_exe()?);
    c.args(["_gateway", &port.to_string()]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)] { use std::os::unix::process::CommandExt; c.process_group(0); }
    #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(0x00000008 | 0x00000200); }
    c.spawn().context("falha ao iniciar o gateway")?;
    for _ in 0..40 {
        if gateway_alive(port) { return Ok(port); }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("gateway não subiu")
}

pub fn run_gateway(port: u16) -> Result<()> {
    let l = TcpListener::bind(("127.0.0.1", port))?;
    for c in l.incoming().flatten() {
        std::thread::spawn(move || { let _ = handle(c); });
    }
    Ok(())
}

fn reply(mut c: TcpStream, code: u16, msg: &str) -> Result<()> {
    let body = format!("{msg}\n");
    write!(c, "HTTP/1.1 {code} {msg}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())?;
    Ok(())
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> { h.windows(n.len()).position(|w| w == n) }

fn handle(mut c: TcpStream) -> Result<()> {
    c.set_read_timeout(Some(Duration::from_secs(5)))?;
    let (mut buf, mut tmp) = (Vec::new(), [0u8; 4096]);
    let end = loop {
        let n = c.read(&mut tmp)?;
        if n == 0 { return Ok(()); }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(p) = find(&buf, b"\r\n\r\n") { break p + 4; }
        if buf.len() > 16384 { return reply(c, 431, "cabeçalho grande demais"); }
    };
    let head = String::from_utf8_lossy(&buf[..end]).to_string();
    let rest = buf[end..].to_vec();
    let mut lines = head.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut p = first.split(' ');
    let (method, path, ver) = (p.next().unwrap_or(""), p.next().unwrap_or(""), p.next().unwrap_or("HTTP/1.1"));
    let Some(after) = path.strip_prefix("/s/") else { return reply(c, 404, "não encontrado") };
    let (token, sub) = after.split_once('/').unwrap_or((after, ""));
    let Some(share) = valid(token) else { return reply(c, 403, "link inválido, expirado ou revogado") };
    let Ok(m) = Machine::load(&share.machine) else { return reply(c, 404, "máquina não existe mais") };
    if ops::start(&m).is_err() { return reply(c, 502, "não foi possível ligar a máquina"); }
    let port = if share.view_only { m.view_ro_port } else { m.view_port };
    if port == 0 { return reply(c, 501, "máquina sem tela compartilhável"); }

    let mut upgrade = false;
    let mut out = format!("{method} /{sub} {ver}\r\n");
    for l in lines.filter(|l| !l.is_empty()) {
        let low = l.to_ascii_lowercase();
        if low.starts_with("upgrade:") { upgrade = low.contains("websocket"); }
        if low.starts_with("connection:") { continue; }
        out += l; out += "\r\n";
    }
    out += if upgrade { "Connection: Upgrade\r\n\r\n" } else { "Connection: close\r\n\r\n" };

    // O websockify (Python) às vezes derruba conexões sob rajada (o noVNC pede dezenas de
    // módulos de uma vez): reconecta e reenvia até vir o primeiro byte da resposta.
    let (up, first) = 'conn: {
        for _ in 0..60 { // o guest pode estar retomando/subindo
            let Ok(mut u) = TcpStream::connect(("127.0.0.1", port)) else { std::thread::sleep(Duration::from_millis(500)); continue };
            u.set_read_timeout(Some(Duration::from_secs(15)))?;
            if u.write_all(out.as_bytes()).is_err() || u.write_all(&rest).is_err() { continue; }
            let mut b = vec![0u8; 16384];
            match u.read(&mut b) {
                Ok(n) if n > 0 => { b.truncate(n); u.set_read_timeout(None)?; break 'conn (u, b); }
                _ => std::thread::sleep(Duration::from_millis(100)),
            }
        }
        return reply(c, 504, "tela ainda indisponível");
    };
    c.write_all(&first)?;
    c.set_read_timeout(None)?;

    let done = Arc::new(AtomicBool::new(false));
    let (c2, u2) = (c.try_clone()?, up.try_clone()?);
    let (c3, u3) = (c.try_clone()?, up.try_clone()?);
    // Vigia: revogação/expiração derruba conexões abertas em poucos segundos.
    let (d, tok) = (done.clone(), token.to_string());
    std::thread::spawn(move || {
        while !d.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(2));
            if valid(&tok).is_none() { let _ = c3.shutdown(Shutdown::Both); let _ = u3.shutdown(Shutdown::Both); break; }
        }
    });
    let t = std::thread::spawn(move || { let (mut a, mut b) = (c2, u2); let _ = std::io::copy(&mut a, &mut b); let _ = b.shutdown(Shutdown::Both); });
    {
        let (mut a, mut b) = (up, c);
        let _ = std::io::copy(&mut a, &mut b);
        // Fecho gracioso: Shutdown::Both com dados do cliente por ler vira RST e o navegador
        // perde a resposta (ERR_EMPTY_RESPONSE). Fecha só a escrita e deixa a thread do
        // cliente drenar o que sobrar até dar timeout.
        let _ = b.shutdown(Shutdown::Write);
        let _ = b.set_read_timeout(Some(Duration::from_millis(800)));
    }
    let _ = t.join();
    done.store(true, Ordering::Relaxed);
    Ok(())
}
