//! Vigia por máquina (processo `hyzevm _watch`): vault no boot, timeout e auto-suspend.
use crate::{activity, ops, qemu, record, ssh, store::*, takeover, vault};
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
    crate::activity::set_actor("watch");
    let mut vault_done = false;
    let mut ready_tries = 0;
    let mut health_fails = 0u32; // checagens seguidas sem o SSH responder
    let mut tick = 0u32;
    let mut last_screen: Option<u64> = None;
    let mut same_screen = 0u32;
    let mut was_ok = false;      // só vigia depois de a máquina ter respondido uma vez (boot não conta)
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
                let _ = crate::net::apply(&m); // escreve ou limpa o proxy conforme a lista atual
                vault_done = true;
            }
        }
        // Detector de VM travada: a cada ~1 min testa o SSH; 4 falhas seguidas (~4 min) = reset (preserva o disco).
        tick += 1;
        if tick % 4 == 0 {
        if ssh::exec_capture(&m, "true").is_ok_and(|o| o.status.success()) { was_ok = true; health_fails = 0; }
        else if was_ok {
            health_fails += 1;
            if health_fails >= 4 {
                activity::log(&m, "health", "sem resposta do SSH há ~4 min — reiniciando a máquina (o disco é preservado)");
                takeover::notify(&format!("{} travou", m.name), "reiniciando a máquina (o disco é preservado)");
                let _ = qemu::system_reset(&m);
                health_fails = 0; was_ok = false; vault_done = false; ready_tries = 0;
            }
        }
        }
        if tick % 20 == 2 { record::maintain(&m); }
        // Tela parada há horas numa máquina que deveria estar trabalhando: avisa uma vez.
        if tick % 20 == 7 && m.template == "desktop" {
            use std::hash::{Hash, Hasher};
            if let Ok(img) = crate::desktop::thumbnail(&m) {
                let mut h = std::collections::hash_map::DefaultHasher::new(); img.hash(&mut h);
                let cur = h.finish();
                if Some(cur) == last_screen { same_screen += 1 } else { same_screen = 0; last_screen = Some(cur); }
                if same_screen == 36 { // 36 checagens de 5 min = 3 h
                    activity::log(&m, "alert", "a tela não muda há 3 horas");
                    takeover::notify(&format!("{} · tela parada", m.name), "a tela não muda há 3 horas — o agente travou?");
                }
            }
        }
        // Ponto de restauração automático (1 por dia, guarda 7).
        if m.auto_checkpoint && tick % 240 == 13 && now().saturating_sub(crate::checkpoint::list(&m).ok().and_then(|v| v.iter().filter(|p| p.auto).map(|p| p.ts).max()).unwrap_or(0)) >= 86_400 {
            let _ = crate::checkpoint::auto(&m);
        }
        // Resumo diário: grava em summaries/AAAA-MM-DD.txt e avisa (uma vez a cada 24 h).
        if tick % 240 == 11 {
            let f = m.dir().join("summary-last");
            let last = std::fs::read_to_string(&f).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
            if now().saturating_sub(last) >= 86_400 {
                let text = crate::summary::build(&m, 86_400);
                let _ = std::fs::create_dir_all(m.dir().join("summaries"));
                let _ = std::fs::write(m.dir().join("summaries").join(format!("{}.txt", crate::ui::clock(now()).replace([':', '/', ' '], "-"))), &text);
                let _ = std::fs::write(&f, now().to_string());
                if text.lines().count() > 2 { takeover::notify(&format!("{} · resumo do dia", m.name), text.lines().nth(1).unwrap_or("").trim()); }
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
