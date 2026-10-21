//! Firewall de saída por máquina: a VM só fala com o proxy do host (QEMU `restrict=on` + guestfwd);
//! o proxy libera só os sites da lista e limita a banda. Roda como thread do vigia (`_watch`).
use crate::{activity, ssh, store::*};
use anyhow::{bail, Result};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const GUEST_PROXY: &str = "http://10.0.2.100:3128";

pub fn allowed(list: &[String], host: &str) -> bool {
    let h = host.trim_end_matches('.').to_lowercase();
    list.iter().any(|a| {
        let a = a.trim().trim_start_matches("*.").to_lowercase();
        a == "*" || h == a || h.ends_with(&format!(".{a}"))
    })
}

/// Balde de fichas simples compartilhado por todas as conexões da máquina.
struct Limiter { kbps: u64, next: Mutex<Instant> }
impl Limiter {
    fn take(&self, n: usize) {
        if self.kbps == 0 { return; }
        let cost = Duration::from_micros(n as u64 * 1_000_000 / (self.kbps * 1024));
        let wait = { let mut t = self.next.lock().unwrap(); let now = Instant::now(); let at = (*t).max(now); *t = at + cost; at - now };
        if !wait.is_zero() { std::thread::sleep(wait); }
    }
}

fn pipe(mut from: TcpStream, mut to: TcpStream, lim: Arc<Limiter>) {
    let mut buf = [0u8; 16 * 1024];
    while let Ok(n) = from.read(&mut buf) {
        if n == 0 || to.write_all(&buf[..n]).is_err() { break; }
        lim.take(n);
    }
    let _ = to.shutdown(std::net::Shutdown::Write);
}

fn handle(mut c: TcpStream, m: Machine, lim: Arc<Limiter>, seen: Arc<Mutex<Vec<(String, Instant)>>>) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(30)));
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 16 * 1024 {
        match c.read(&mut b) { Ok(1) => head.push(b[0]), _ => return }
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let first = text.lines().next().unwrap_or("");
    let mut p = first.split_whitespace();
    let (method, target) = (p.next().unwrap_or(""), p.next().unwrap_or(""));
    let (host, port, rewritten) = if method.eq_ignore_ascii_case("CONNECT") {
        let (h, pt) = target.rsplit_once(':').unwrap_or((target, "443"));
        (h.to_string(), pt.parse().unwrap_or(443u16), None)
    } else if let Some(rest) = target.strip_prefix("http://") {
        let (hp, path) = rest.split_once('/').map_or((rest, "/".to_string()), |(a, b)| (a, format!("/{b}")));
        let (h, pt) = hp.rsplit_once(':').map_or((hp, 80u16), |(a, b)| (a, b.parse().unwrap_or(80)));
        let req = text.replacen(target, &path, 1);
        (h.to_string(), pt, Some(req))
    } else { let _ = c.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n"); return };

    if !allowed(&m.net_allow, &host) {
        let _ = c.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        // anota no registro (no máx. 1 vez por minuto por site)
        let mut s = seen.lock().unwrap();
        s.retain(|(_, t)| t.elapsed() < Duration::from_secs(60));
        if !s.iter().any(|(h, _)| *h == host) { s.push((host.clone(), Instant::now())); activity::log(&m, "net-blocked", &host); }
        return;
    }
    let Some(up) = (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next()).and_then(|a| TcpStream::connect_timeout(&a, Duration::from_secs(10)).ok()) else {
        let _ = c.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n"); return };
    let _ = c.set_read_timeout(None);
    match rewritten {
        None => { let _ = c.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n"); }
        Some(req) => { if (&up).write_all(req.as_bytes()).is_err() { return; } }
    }
    let (Ok(c2), Ok(up2)) = (c.try_clone(), up.try_clone()) else { return };
    let l2 = lim.clone();
    std::thread::spawn(move || pipe(up2, c2, l2));
    pipe(c, up, lim);
}

/// Processo do proxy (`hyzevm _netproxy <nome>`): escuta e vive enquanto a máquina existir ligada.
pub fn run_proxy(name: &str) -> Result<()> {
    activity::set_actor("net");
    let m = Machine::load(name)?;
    if m.net_allow.is_empty() || m.net_proxy_port == 0 { return Ok(()); }
    let l = TcpListener::bind(("127.0.0.1", m.net_proxy_port))?;
    let lim = Arc::new(Limiter { kbps: m.net_limit_kbps, next: Mutex::new(Instant::now()) });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let n = name.to_string();
    // some junto com a máquina (60 s depois de ela parar)
    std::thread::spawn(move || {
        let mut down = 0;
        loop {
            std::thread::sleep(Duration::from_secs(10));
            match Machine::load(&n) { Ok(m) if crate::qemu::is_running(&m) => down = 0, Ok(_) => { down += 10; if down >= 60 { std::process::exit(0); } } Err(_) => std::process::exit(0) }
        }
    });
    for c in l.incoming().flatten() {
        let Ok(m) = Machine::load(name) else { return Ok(()) }; // lê a lista atual a cada conexão
        let (lim, seen) = (lim.clone(), seen.clone());
        std::thread::spawn(move || handle(c, m, lim, seen));
    }
    Ok(())
}

/// Garante o proxy no ar ANTES de o QEMU subir (o guestfwd conecta nele na partida).
pub fn ensure_proxy(m: &Machine) -> Result<()> {
    if m.net_allow.is_empty() || m.net_proxy_port == 0 { return Ok(()); }
    let up = || TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], m.net_proxy_port)), Duration::from_millis(300)).is_ok();
    if up() { return Ok(()); }
    let mut c = std::process::Command::new(std::env::current_exe()?);
    c.args(["_netproxy", &m.name]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)] { use std::os::unix::process::CommandExt; c.process_group(0); }
    #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(0x00000008 | 0x00000200); }
    c.spawn()?;
    for _ in 0..30 { if up() { return Ok(()); } std::thread::sleep(Duration::from_millis(100)); }
    bail!("o proxy de rede não subiu")
}

/// Escreve (ou remove) a configuração de proxy dentro da VM: ambiente, apt e Chromium.
pub fn apply(m: &Machine) -> Result<()> {
    let on = !m.net_allow.is_empty();
    let mut sc = String::from("set -e\nsudo sed -i '/# hyzevm-net begin/,/# hyzevm-net end/d' /etc/environment\n");
    if on {
        sc += &format!("{{ echo '# hyzevm-net begin'; for v in http_proxy https_proxy HTTP_PROXY HTTPS_PROXY; do echo \"$v=\\\"{GUEST_PROXY}\\\"\"; done; echo 'no_proxy=\"localhost,127.0.0.1\"'; echo '# hyzevm-net end'; }} | sudo tee -a /etc/environment >/dev/null\n\
            echo {GUEST_PROXY} | sudo tee /etc/hyze-proxy >/dev/null\n\
            echo 'Acquire::http::Proxy \"{GUEST_PROXY}\";' | sudo tee /etc/apt/apt.conf.d/99hyze-proxy >/dev/null\n");
    } else {
        sc += "sudo rm -f /etc/hyze-proxy /etc/apt/apt.conf.d/99hyze-proxy\n";
    }
    sc += &format!("echo {} | base64 -d | sudo tee /usr/local/bin/hyze-chromium >/dev/null; sudo chmod 755 /usr/local/bin/hyze-chromium\n",
        ssh::sh_quote(&base64::Engine::encode(&base64::engine::general_purpose::STANDARD, include_str!("hyze-chromium.sh"))));
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("não consegui configurar o proxy na máquina: {}", String::from_utf8_lossy(&o.stderr).trim()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::allowed;
    fn l(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }
    #[test]
    fn allow_list_matches_domain_and_subdomains_only() {
        let a = l(&["instagram.com", "*.cdninstagram.com"]);
        assert!(allowed(&a, "instagram.com"));
        assert!(allowed(&a, "www.instagram.com"));
        assert!(allowed(&a, "scontent.cdninstagram.com"));
        assert!(allowed(&a, "Instagram.COM."));
        assert!(!allowed(&a, "evilinstagram.com"));
        assert!(!allowed(&a, "instagram.com.evil.io"));
        assert!(allowed(&l(&["*"]), "qualquer.coisa"));
        assert!(!allowed(&[], "example.com"));
    }
}
