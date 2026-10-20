//! Vigia por máquina (processo `hyzevm _watch`): vault no boot, timeout e auto-suspend.
use crate::{ops, qemu, ssh, store::*, vault};
use anyhow::Result;
use std::process::{Command, Stdio};
use std::time::Duration;

const TICK: u64 = 15;

fn alive(m: &Machine) -> bool {
    std::fs::metadata(m.watch_file()).and_then(|x| x.modified()).ok()
        .and_then(|t| t.elapsed().ok()).is_some_and(|e| e < Duration::from_secs(TICK * 3))
}

pub fn ensure(m: &Machine) -> Result<()> {
    if alive(m) { return Ok(()); }
    let _ = std::fs::write(m.watch_file(), b"1");
    let mut c = Command::new(std::env::current_exe()?);
    c.args(["_watch", &m.name]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)] { use std::os::unix::process::CommandExt; c.process_group(0); }
    #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(0x00000008 | 0x00000200); }
    c.spawn()?;
    Ok(())
}

/// Há clientes conectados (desktop/viewer/portas publicadas)? Conta conexões no guest.
fn busy(m: &Machine) -> bool {
    let mut ports = vec![6080u16, 5900, 6081, 5901];
    ports.extend(m.ports.iter().map(|(_, g)| *g));
    let f = ports.iter().map(|p| format!("sport = :{p}")).collect::<Vec<_>>().join(" or ");
    ssh::exec_capture(m, &format!("ss -Htn state established '( {f} )' | wc -l"))
        .ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().ok()).is_some_and(|n| n > 0)
}

pub fn run(name: &str) -> Result<()> {
    let mut vault_done = false;
    let mut ready_tries = 0;
    loop {
        let Ok(m) = Machine::load(name) else { return Ok(()) };
        if !qemu::is_running(&m) {
            // 24/7: se caiu sem o usuário pedir, sobe de novo.
            if m.always_on && m.wants_up() && ops::start(&m).is_ok() { vault_done = false; ready_tries = 0; continue; }
            let _ = std::fs::remove_file(m.watch_file());
            return Ok(());
        }
        let _ = std::fs::write(m.watch_file(), b"1");
        if qemu::state(&m) == "paused" { std::thread::sleep(Duration::from_secs(TICK)); continue; } // pausada ao vivo: nada a vigiar

        if !vault_done && m.vault && ready_tries < 60 {
            ready_tries += 1;
            if ssh::exec_capture(&m, "test -f /var/lib/hyzevm-ready").is_ok_and(|o| o.status.success()) {
                let _ = vault::push(&m);
                vault_done = true;
            }
        }
        if let Some(exp) = m.expires_at {
            if now() >= exp {
                if m.on_timeout == "kill" { let _ = std::fs::remove_file(m.watch_file()); return ops::remove(&m); }
                let mut m2 = m.clone(); m2.expires_at = None; let _ = m2.save();
                let _ = qemu::pause(&m);
                continue;
            }
        }
        if m.auto_suspend_secs > 0 && m.idle_secs() >= m.auto_suspend_secs {
            if busy(&m) { m.touch(); } else { let _ = qemu::pause(&m); continue; }
        }
        std::thread::sleep(Duration::from_secs(TICK));
    }
}
