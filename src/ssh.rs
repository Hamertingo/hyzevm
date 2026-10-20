//! Acesso ao guest: SSH com chave por máquina, via porta local (igual nos 3 SOs).
use crate::store::Machine;
use anyhow::{bail, Context, Result};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn base(m: &Machine) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-p", &m.ssh_port.to_string(), "-i"]).arg(m.key()).args([
        "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=no", "-o", "LogLevel=ERROR",
        "-o", "ConnectTimeout=5",
        "-o", if cfg!(windows) { "UserKnownHostsFile=NUL" } else { "UserKnownHostsFile=/dev/null" },
    ]);
    c
}

pub fn keygen(m: &Machine) -> Result<String> {
    let st = Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-C", "hyzevm", "-f"])
        .arg(m.key()).status().context("ssh-keygen não encontrado (instale o OpenSSH client)")?;
    if !st.success() { bail!("ssh-keygen falhou"); }
    Ok(std::fs::read_to_string(m.key().with_extension("pub"))?
        .trim().to_string()) // id_ed25519.pub
}

fn target(m: &Machine) -> String { format!("{}@127.0.0.1", m.user) }

/// Executa um comando capturando saída (usado pelo MCP).
pub fn exec_capture(m: &Machine, cmd: &str) -> Result<Output> {
    Ok(base(m).arg(target(m)).arg(cmd).stdin(Stdio::null()).output()?)
}

/// Executa herdando o terminal; devolve o exit code do comando remoto.
pub fn exec_inherit(m: &Machine, cmd: Option<&str>, tty: bool) -> Result<i32> {
    let mut c = base(m);
    if tty { c.arg("-t"); }
    c.arg(target(m));
    if let Some(cmd) = cmd { c.arg(cmd); }
    Ok(c.status()?.code().unwrap_or(255))
}

/// Espera o SSH responder e o provisionamento (cloud-init) terminar,
/// mostrando o que o guest está fazendo.
pub fn wait_ready(m: &Machine, secs: u64) -> Result<()> {
    let t = Instant::now();
    let mut last_msg = Instant::now() - Duration::from_secs(60);
    loop {
        if let Ok(o) = exec_capture(m, "test -f /var/lib/hyzevm-ready") {
            if o.status.success() { return Ok(()); }
        }
        if t.elapsed() > Duration::from_secs(secs) {
            bail!("máquina não ficou pronta em {secs}s — veja {}", m.serial_log().display());
        }
        if last_msg.elapsed() >= Duration::from_secs(10) {
            last_msg = Instant::now();
            let log = std::fs::read_to_string(m.serial_log()).unwrap_or_default();
            let what = log.lines().rev().map(clean).find(|l| l.chars().filter(|c| c.is_alphabetic()).count() >= 8)
                .unwrap_or_else(|| "iniciando".into());
            let what: String = what.chars().rev().take(70).collect::<Vec<_>>().into_iter().rev().collect();
            crate::ui::detail(&format!("{:>3}s  {}", t.elapsed().as_secs(), what));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Espera o sshd do guest responder (após ligar/retomar). Retomada de suspend responde na hora.
pub fn wait_ssh(m: &Machine, secs: u64) -> Result<()> {
    let t = Instant::now();
    loop {
        if exec_capture(m, "true").is_ok_and(|o| o.status.success()) { return Ok(()); }
        if t.elapsed() > Duration::from_secs(secs) { bail!("SSH da máquina não respondeu em {secs}s"); }
        std::thread::sleep(Duration::from_millis(700));
    }
}

pub fn copy(m: &Machine, from: &str, to: &str) -> Result<()> {
    let mut c = Command::new("scp");
    c.args(["-r", "-P", &m.ssh_port.to_string(), "-i"]).arg(m.key()).args([
        "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=no", "-o", "LogLevel=ERROR",
        "-o", if cfg!(windows) { "UserKnownHostsFile=NUL" } else { "UserKnownHostsFile=/dev/null" }]);
    let fix = |p: &str| p.strip_prefix(&format!("{}:", m.name)).map(|r| format!("{}:{}", target(m), r)).unwrap_or(p.to_string());
    let st = c.arg(fix(from)).arg(fix(to)).status()?;
    if !st.success() { bail!("cp falhou"); }
    Ok(())
}

/// Roda `bash -s` no guest alimentando o script por stdin (segredos não vão no argv).
pub fn exec_stdin(m: &Machine, script: &str) -> Result<Output> {
    use std::io::Write;
    let mut c = base(m).arg(target(m)).arg("bash -s").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    c.stdin.take().unwrap().write_all(script.as_bytes())?;
    Ok(c.wait_with_output()?)
}

pub fn sh_quote(s: &str) -> String { format!("'{}'", s.replace('\'', "'\\''")) }

/// Tira sequências de escape (ESC[...) e caracteres de controle de uma linha do serial.
fn clean(l: &str) -> String {
    let mut out = String::new();
    let mut it = l.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            if it.peek() == Some(&'[') { it.next(); while let Some(&n) = it.peek() { it.next(); if n.is_ascii_alphabetic() { break; } } }
        } else if !c.is_control() { out.push(c); }
    }
    out.trim().to_string()
}
