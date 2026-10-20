//! Browser por snapshot: o agente enxerga a página como uma lista de elementos com referências
//! (menos tokens e menos erro que clicar em pixels). Fala com o Chromium do guest via CDP (só loopback no guest).
use crate::{ssh, store::Machine};
use anyhow::{bail, Result};

const CDP: &str = include_str!("cdp.py");

/// Garante o Chromium rodando com depuração, num perfil persistente (logins sobrevivem).
fn ensure_browser(m: &Machine) -> Result<()> {
    let sc = "curl -sf http://127.0.0.1:9222/json/version >/dev/null && exit 0\n\
        export DISPLAY=:99\n\
        setsid nohup chromium --user-data-dir=\"$HOME/.config/hyze-chromium\" --remote-debugging-port=9222 \
        --no-first-run --no-default-browser-check --start-maximized --password-store=basic about:blank \
        >/dev/null 2>&1 </dev/null &\n\
        for i in $(seq 1 40); do curl -sf http://127.0.0.1:9222/json/version >/dev/null && exit 0; sleep 0.5; done\n\
        echo 'Chromium não subiu' >&2; exit 1\n";
    let o = ssh::exec_stdin(m, sc)?;
    if !o.status.success() { bail!("{} (a máquina tem o template desktop?)", String::from_utf8_lossy(&o.stderr).trim()); }
    Ok(())
}

/// Roda um subcomando do cdp.py no guest; devolve a saída.
pub fn run(m: &Machine, args: &[&str]) -> Result<String> {
    ensure_browser(m)?;
    let q = args.iter().map(|a| ssh::sh_quote(a)).collect::<Vec<_>>().join(" ");
    let sc = format!("cat > /tmp/hyze-cdp.py <<'HYZE_PY_EOF'\n{CDP}\nHYZE_PY_EOF\npython3 /tmp/hyze-cdp.py {q}\n");
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("{}", String::from_utf8_lossy(&o.stderr).trim().trim_start_matches("erro: ")); }
    Ok(String::from_utf8_lossy(&o.stdout).trim_end().to_string())
}
