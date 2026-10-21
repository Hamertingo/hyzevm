//! Browser por snapshot: o agente enxerga a página como uma lista de elementos com referências
//! (menos tokens e menos erro que clicar em pixels). Fala com o Chromium do guest via CDP (só loopback no guest).
use crate::{activity, ssh, store::Machine};
use anyhow::{bail, Result};

const CDP: &str = include_str!("cdp.py");

static PROFILE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Perfil de navegador (uma conta por perfil): None/"default" = o padrão. Cada perfil tem pasta e porta próprias.
pub fn set_profile(p: Option<String>) -> Result<()> {
    if let Some(n) = &p { if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { bail!("perfil inválido “{n}” (use letras, números, - e _)"); } }
    *PROFILE.lock().unwrap() = p.filter(|n| n != "default");
    Ok(())
}

/// (pasta do perfil, porta de depuração)
fn profile() -> (String, u16) {
    match PROFILE.lock().unwrap().as_deref() {
        None => ("hyze-chromium".into(), 9222),
        Some(n) => (format!("hyze-chromium-{n}"), 9300 + (n.bytes().map(|b| b as u16).fold(0u16, |a, b| a.wrapping_mul(31).wrapping_add(b)) % 600)),
    }
}

/// Garante o Chromium rodando com depuração, num perfil persistente (logins sobrevivem).
fn ensure_browser(m: &Machine) -> Result<()> {
    let (dir, port) = profile();
    let sc = format!("curl -sf http://127.0.0.1:{port}/json/version >/dev/null && exit 0\n\
        export DISPLAY=:99\n\
        L=$(cat /etc/hyze-lang 2>/dev/null || echo en-US)\n\
        P=$(cat /etc/hyze-proxy 2>/dev/null)\n\
        setsid nohup chromium --user-data-dir=\"$HOME/.config/{dir}\" --remote-debugging-port={port} \
        ${{P:+--proxy-server=\"$P\"}} --lang=\"$L\" --accept-lang=\"$L,${{L%%-*}},en-US,en\" \
        --no-first-run --no-default-browser-check --start-maximized --password-store=basic about:blank \
        >/dev/null 2>&1 </dev/null &\n\
        for i in $(seq 1 40); do curl -sf http://127.0.0.1:{port}/json/version >/dev/null && exit 0; sleep 0.5; done\n\
        echo 'Chromium não subiu' >&2; exit 1\n");
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("{} (a máquina tem o template desktop?)", String::from_utf8_lossy(&o.stderr).trim()); }
    Ok(())
}

/// Roda um subcomando do cdp.py no guest; devolve a saída.
pub fn run(m: &Machine, args: &[&str]) -> Result<String> {
    match args.first().copied() {
        Some("snapshot" | "tabs") | None => {}
        // o texto digitado pode ser senha: só o tamanho vai para o registro
        Some("type") => activity::log(m, "browser", &format!("digitou {} caracteres em {}", args.get(2).map_or(0, |t| t.chars().count()), args.get(1).copied().unwrap_or("?"))),
        Some(_) => activity::log(m, "browser", &format!("{}{}", PROFILE.lock().unwrap().as_deref().map_or(String::new(), |p| format!("[{p}] ")), args.join(" "))),
    }
    if args.first() == Some(&"upload") { crate::guard::enforce(m, "upload", &args[1..].join(" "))?; }
    ensure_browser(m)?;
    let q = args.iter().map(|a| ssh::sh_quote(a)).collect::<Vec<_>>().join(" ");
    let sc = format!("cat > /tmp/hyze-cdp.py <<'HYZE_PY_EOF'\n{CDP}\nHYZE_PY_EOF\nHYZE_CDP_PORT={port} python3 /tmp/hyze-cdp.py {q}\n", port = profile().1);
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("{}", String::from_utf8_lossy(&o.stderr).trim().trim_start_matches("erro: ")); }
    Ok(String::from_utf8_lossy(&o.stdout).trim_end().to_string())
}
