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
    crate::activity::log(m, "takeover", reason);
    notify(&format!("{} precisa de você", m.name), reason);
    Ok(t)
}

pub fn done(m: &Machine) -> Result<()> {
    let Some(mut t) = status(m) else { bail!("não há pedido de takeover em {}", m.name) };
    crate::activity::log(m, "takeover", "controle devolvido");
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

/// Aviso no computador (sistema) e, se configurado, no celular (ntfy).
pub fn notify(title: &str, body: &str) {
    push_phone(title, body);
    match std::env::consts::OS {
        "macos" => {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let _ = Command::new("osascript").args(["-e", &format!("display notification \"{}\" with title \"{}\" sound name \"Glass\"", esc(body), esc(title))]).output();
        }
        "linux" => { let _ = Command::new("notify-send").args([title, body]).output(); }
        "windows" => {
            // toast nativo via PowerShell (sem módulos extras)
            let esc = |s: &str| s.replace('\'', "''").replace('\n', " ");
            let ps = format!("[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
                $x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
                $t = $x.GetElementsByTagName('text'); $t.Item(0).AppendChild($x.CreateTextNode('{}')) > $null; $t.Item(1).AppendChild($x.CreateTextNode('{}')) > $null; \
                [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('HyzeVM').Show([Windows.UI.Notifications.ToastNotification]::new($x))", esc(title), esc(body));
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps]);
            #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(0x08000000); }
            let _ = c.output();
        }
        _ => {}
    }
}

fn phone_file() -> std::path::PathBuf { home().join("notify.json") }

#[derive(Serialize, Deserialize)]
pub struct Phone { pub server: String, pub topic: String }

pub fn phone() -> Option<Phone> { std::fs::read(phone_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()) }

pub fn set_phone(p: Option<Phone>) -> Result<()> {
    match p {
        Some(p) => { std::fs::create_dir_all(home())?; std::fs::write(phone_file(), serde_json::to_vec_pretty(&p)?)?; }
        None => { let _ = std::fs::remove_file(phone_file()); }
    }
    Ok(())
}

/// Manda o aviso ao celular via ntfy (opt-in: só se você configurou `hyzevm notify set`). Não bloqueia.
pub fn push_phone(title: &str, body: &str) {
    let Some(p) = phone() else { return };
    let _ = Command::new("curl").args(["-s", "-m", "10", "-H", &format!("Title: {}", title.replace('\n', " ")), "-H", "Priority: high", "-d", body, &format!("{}/{}", p.server.trim_end_matches('/'), p.topic)])
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}
