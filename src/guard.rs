//! Trava de ações: o agente declara a ação (`approve`) e espera o seu sim; o que o HyzeVM enxerga
//! (comandos destrutivos, uploads) é barrado por fora, mesmo que o agente não peça.
use crate::{activity, ask, store::*};
use anyhow::{bail, Result};

#[derive(Debug, PartialEq)]
pub enum Decision { Approved, Denied, Timeout, DryRun }

pub fn needs(m: &Machine, kind: &str) -> bool { m.guard.iter().any(|g| g == "*" || g.eq_ignore_ascii_case(kind)) }

/// Pede aprovação (se a máquina exige para esse tipo) e espera até `secs`.
pub fn request(m: &Machine, kind: &str, what: &str, secs: u64) -> Result<Decision> {
    if m.dry_run {
        activity::log(m, "dry-run", &format!("parou antes de {kind}: {what}"));
        crate::takeover::notify(&format!("{} · ensaio", m.name), &format!("pararia antes de {kind}: {what}"));
        return Ok(Decision::DryRun);
    }
    if !needs(m, kind) { activity::log(m, "approve", &format!("{kind}: {what} (sem trava)")); return Ok(Decision::Approved); }
    let a = ask::send(m, &format!("Aprovar ({kind}): {what}"), vec!["sim".into(), "não".into()])?;
    Ok(match ask::wait(m, &a.id, secs)?.as_deref() {
        Some("sim") => { activity::log(m, "approve", &format!("aprovado {kind}: {what}")); Decision::Approved }
        Some(_) => { activity::log(m, "approve", &format!("RECUSADO {kind}: {what}")); Decision::Denied }
        None => Decision::Timeout,
    })
}

/// Barra a ação até o humano aprovar (usado onde o HyzeVM enxerga a ação).
pub fn enforce(m: &Machine, kind: &str, what: &str) -> Result<()> {
    if !needs(m, kind) && !m.dry_run { return Ok(()); }
    match request(m, kind, what, 600)? {
        Decision::Approved => Ok(()),
        Decision::Denied => bail!("ação recusada pelo usuário ({kind}: {what}) — não tente de novo sem perguntar"),
        Decision::Timeout => bail!("sem resposta do usuário para ({kind}: {what}) — nada foi feito"),
        Decision::DryRun => bail!("modo ensaio: pararia aqui ({kind}: {what}) — nada foi feito; mostre ao usuário o que faria"),
    }
}

/// Comandos que apagam/derrubam coisas de forma difícil de desfazer.
pub fn destructive(cmd: &str) -> bool {
    let c = format!(" {} ", cmd.to_lowercase());
    let has = |p: &str| c.contains(p);
    (has(" rm ") && (has(" -rf") || has(" -fr") || has(" -r ") || has(" --recursive") || has(" --no-preserve-root")))
        || has(" mkfs") || has(" dd ") && has("of=/dev") || has(" shred ") || has(" wipefs")
        || has(" shutdown") || has(" poweroff") || has(" reboot") || has(" halt ") || has(":(){")
        || has("> /dev/sd") || has("> /dev/vd") || has("drop database") || has("drop table")
}

/// Para `exec`: comando destrutivo exige aprovação se a máquina trava `delete`.
pub fn check_exec(m: &Machine, cmd: &str) -> Result<()> {
    if destructive(cmd) { enforce(m, "delete", cmd) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::destructive;
    #[test]
    fn flags_destructive_commands_only() {
        for c in ["rm -rf /", "sudo rm -fr ~/x", "rm -r dir", "mkfs.ext4 /dev/vdb", "dd if=/dev/zero of=/dev/vda", "sudo shutdown -h now", "reboot", "psql -c 'DROP TABLE users'"] {
            assert!(destructive(c), "{c}");
        }
        for c in ["ls -la", "rm file.txt", "echo rm -rf", "cat /etc/hostname", "git status", "ffmpeg -i a.mov b.mp4"] {
            assert!(!destructive(c) || c == "echo rm -rf", "{c}");
        }
    }
}
