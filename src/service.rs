//! 24/7: `hyzevm up` mantém as máquinas always-on de pé; `service install` roda isso no login/boot do host.
use crate::{ops, qemu, store, watch};
use anyhow::{bail, Context, Result};
use std::process::Command;
use std::time::Duration;

fn ensure_all() {
    for m in store::list().unwrap_or_default().iter().filter(|m| m.always_on) {
        // Só sobe se o usuário não a desligou de propósito (intent=down).
        let first_boot = !m.intent_file().exists();
        if !qemu::is_running(m) && (m.wants_up() || first_boot) {
            if let Err(e) = ops::start(m) { eprintln!("{}: {e:#}", m.name); }
        } else if qemu::is_running(m) {
            let _ = watch::ensure(m);
        }
    }
}

/// `wait`: fica de supervisor (ideal para launchd/systemd).
pub fn up(wait: bool) -> Result<()> {
    ensure_all();
    while wait {
        std::thread::sleep(Duration::from_secs(30));
        ensure_all();
    }
    Ok(())
}

fn exe() -> Result<String> { Ok(std::env::current_exe()?.to_string_lossy().to_string()) }

const LABEL: &str = "sh.hyzevm.up";

fn plist(exe: &str) -> String {
    // caffeinate -i: o Mac não dorme por inatividade enquanto o supervisor roda.
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>/usr/bin/caffeinate</string><string>-i</string><string>{exe}</string><string>up</string><string>--wait</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>StandardErrorPath</key><string>{log}</string><key>StandardOutPath</key><string>{log}</string>
</dict></plist>
"#, log = store::home().join("up.log").display())
}

fn systemd_unit(exe: &str) -> String {
    format!("[Unit]\nDescription=HyzeVM (máquinas 24/7)\n[Service]\nExecStart={exe} up --wait\nRestart=always\n[Install]\nWantedBy=default.target\n")
}

pub fn install(print_only: bool) -> Result<()> {
    let exe = exe()?;
    let home = dirs::home_dir().context("sem home")?;
    match std::env::consts::OS {
        "macos" => {
            let p = home.join(format!("Library/LaunchAgents/{LABEL}.plist"));
            if print_only { println!("# {}\n{}", p.display(), plist(&exe)); return Ok(()); }
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(&p, plist(&exe))?;
            let _ = Command::new("launchctl").args(["unload"]).arg(&p).output();
            if !Command::new("launchctl").args(["load", "-w"]).arg(&p).status()?.success() { bail!("launchctl load falhou"); }
            println!("serviço instalado ({}). O Mac não dorme por inatividade enquanto ele roda.", p.display());
        }
        "linux" => {
            let p = home.join(".config/systemd/user/hyzevm.service");
            if print_only { println!("# {}\n{}", p.display(), systemd_unit(&exe)); return Ok(()); }
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(&p, systemd_unit(&exe))?;
            let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).status();
            if !Command::new("systemctl").args(["--user", "enable", "--now", "hyzevm.service"]).status()?.success() { bail!("systemctl falhou"); }
            println!("serviço instalado. Para subir sem login: loginctl enable-linger $USER");
        }
        "windows" => {
            let cmd = format!("\"{exe}\" up --wait");
            if print_only { println!("schtasks /Create /TN HyzeVM /SC ONLOGON /RL LIMITED /F /TR '{cmd}'"); return Ok(()); }
            if !Command::new("schtasks").args(["/Create", "/TN", "HyzeVM", "/SC", "ONLOGON", "/RL", "LIMITED", "/F", "/TR", &cmd]).status()?.success() { bail!("schtasks falhou"); }
            println!("tarefa HyzeVM criada (roda no login).");
        }
        o => bail!("sistema {o} não suportado"),
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let home = dirs::home_dir().context("sem home")?;
    match std::env::consts::OS {
        "macos" => {
            let p = home.join(format!("Library/LaunchAgents/{LABEL}.plist"));
            let _ = Command::new("launchctl").args(["unload", "-w"]).arg(&p).status();
            let _ = std::fs::remove_file(p);
        }
        "linux" => {
            let _ = Command::new("systemctl").args(["--user", "disable", "--now", "hyzevm.service"]).status();
            let _ = std::fs::remove_file(home.join(".config/systemd/user/hyzevm.service"));
        }
        "windows" => { let _ = Command::new("schtasks").args(["/Delete", "/TN", "HyzeVM", "/F"]).status(); }
        _ => {}
    }
    println!("serviço removido");
    Ok(())
}

#[cfg(target_os = "macos")]
const MENUBAR_LABEL: &str = "sh.hyzevm.menubar";

#[cfg(target_os = "macos")]
pub fn install_menubar() -> Result<()> {
    let exe = exe()?;
    let p = dirs::home_dir().context("sem home")?.join(format!("Library/LaunchAgents/{MENUBAR_LABEL}.plist"));
    let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{MENUBAR_LABEL}</string>
<key>ProgramArguments</key><array><string>{exe}</string><string>menubar</string><string>--foreground</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#);
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, xml)?;
    let _ = Command::new("launchctl").args(["unload"]).arg(&p).output();
    if !Command::new("launchctl").args(["load", "-w"]).arg(&p).status()?.success() { bail!("launchctl load falhou"); }
    println!("menu bar vai abrir sozinho no login ({})", p.display());
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall_menubar() -> Result<()> {
    let p = dirs::home_dir().context("sem home")?.join(format!("Library/LaunchAgents/{MENUBAR_LABEL}.plist"));
    let _ = Command::new("launchctl").args(["unload", "-w"]).arg(&p).status();
    let _ = std::fs::remove_file(p);
    println!("menu bar removido do login");
    Ok(())
}
