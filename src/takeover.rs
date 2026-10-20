//! request_takeover: o agente para (CAPTCHA, 2FA, tela inesperada) e chama o humano, que assume pela tela.
use crate::{share, store::*};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
pub struct Takeover {
    pub reason: String,
    pub url: String,
    pub requested_at: u64,
    pub done: bool,
}

fn file(m: &Machine) -> std::path::PathBuf { m.dir().join("takeover.json") }

pub fn status(m: &Machine) -> Option<Takeover> {
    std::fs::read(file(m)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn pending(m: &Machine) -> Option<Takeover> { status(m).filter(|t| !t.done) }

/// Cria o link de controle, registra o pedido e avisa o usuário (notificação do sistema).
pub fn request(m: &Machine, reason: &str) -> Result<Takeover> {
    let (_, url, _) = share::create(m, false, 3600)?;
    let t = Takeover { reason: reason.into(), url, requested_at: now(), done: false };
    std::fs::write(file(m), serde_json::to_vec_pretty(&t)?)?;
    notify(&format!("{} precisa de você", m.name), reason);
    Ok(t)
}

pub fn done(m: &Machine) -> Result<()> {
    let Some(mut t) = status(m) else { bail!("não há pedido de takeover em {}", m.name) };
    t.done = true;
    std::fs::write(file(m), serde_json::to_vec_pretty(&t)?)?;
    share::revoke_machine(&m.name); // o link de controle morre junto
    Ok(())
}

/// Espera o humano devolver o controle. Ok(true) = devolveu; Ok(false) = ainda esperando.
pub fn wait(m: &Machine, secs: u64) -> Result<bool> {
    let t0 = Instant::now();
    loop {
        match status(m) {
            None => bail!("não há pedido de takeover em {}", m.name),
            Some(t) if t.done => return Ok(true),
            _ => {}
        }
        if t0.elapsed() >= Duration::from_secs(secs) { return Ok(false); }
        std::thread::sleep(Duration::from_secs(2));
    }
}

pub fn notify(title: &str, body: &str) {
    match std::env::consts::OS {
        "macos" => {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = Command::new("osascript").args(["-e", &format!("display notification \"{}\" with title \"{}\" sound name \"Glass\"", esc(body), esc(title))]).output();
        }
        "linux" => { let _ = Command::new("notify-send").args([title, body]).output(); }
        _ => {}
    }
}
