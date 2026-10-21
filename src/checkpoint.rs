//! Pontos de restauração: snapshots internos do qcow2, tirados COM a máquina ligada (só o disco; consistente
//! como depois de uma queda de energia — o ext4 recupera pelo journal). Voltar ao ponto desliga, restaura e religa.
use crate::{activity, ops, qemu, ssh, store::*};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::process::Command;

const PREFIX: &str = "ck-";
const AUTO_KEEP: usize = 7;

#[derive(Clone, Debug)]
pub struct Point { pub name: String, pub ts: u64, pub auto: bool }

fn img() -> Result<std::path::PathBuf> { qemu::qemu_img() }

fn valid(name: &str) -> Result<()> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { bail!("nome inválido “{name}” (use letras, números, - e _)"); }
    Ok(())
}

pub fn list(m: &Machine) -> Result<Vec<Point>> {
    let o = Command::new(img()?).args(["info", "-U", "--output=json"]).arg(m.disk()).output()?;
    if !o.status.success() { bail!("qemu-img info falhou: {}", String::from_utf8_lossy(&o.stderr).trim()); }
    let v: Value = serde_json::from_slice(&o.stdout)?;
    let mut pts: Vec<Point> = v["snapshots"].as_array().into_iter().flatten().filter_map(|s| {
        let n = s["name"].as_str()?.strip_prefix(PREFIX)?.to_string();
        Some(Point { auto: n.starts_with("auto-"), ts: s["date-sec"].as_u64().unwrap_or(0), name: n })
    }).collect();
    pts.sort_by_key(|p| p.ts);
    Ok(pts)
}

/// Cria o ponto (com a máquina ligada ou desligada).
pub fn save(m: &Machine, name: &str) -> Result<()> {
    valid(name)?;
    if list(m)?.iter().any(|p| p.name == name) { bail!("o ponto “{name}” já existe"); }
    let full = format!("{PREFIX}{name}");
    if qemu::is_running(m) {
        qemu::qmp(m, json!({"execute": "blockdev-snapshot-internal-sync", "arguments": {"device": "virtio0", "name": full}}))
            .context("não consegui criar o ponto com a máquina ligada")?;
    } else {
        let st = Command::new(img()?).args(["snapshot", "-c", &full]).arg(m.disk()).status()?;
        if !st.success() { bail!("qemu-img snapshot falhou"); }
    }
    activity::log(m, "checkpoint", name);
    Ok(())
}

pub fn remove(m: &Machine, name: &str) -> Result<()> {
    valid(name)?;
    let full = format!("{PREFIX}{name}");
    if qemu::is_running(m) {
        qemu::qmp(m, json!({"execute": "blockdev-snapshot-delete-internal-sync", "arguments": {"device": "virtio0", "name": full}}))?;
    } else if !Command::new(img()?).args(["snapshot", "-d", &full]).arg(m.disk()).status()?.success() { bail!("qemu-img snapshot -d falhou"); }
    Ok(())
}

/// Volta o disco ao ponto: desliga a máquina, restaura e liga de novo. O que veio depois do ponto se perde.
pub fn restore(m: &Machine, name: &str) -> Result<()> {
    valid(name)?;
    if !list(m)?.iter().any(|p| p.name == name) { bail!("ponto “{name}” não existe (veja `hyzevm checkpoint ls {}`)", m.name); }
    let was_up = qemu::is_running(m);
    activity::log(m, "checkpoint", &format!("voltando ao ponto {name}"));
    if was_up { qemu::stop(m, false).or_else(|_| qemu::stop(m, true))?; }
    let st = Command::new(img()?).args(["snapshot", "-a", &format!("{PREFIX}{name}")]).arg(m.disk()).status()?;
    if !st.success() { bail!("qemu-img snapshot -a falhou"); }
    let _ = std::fs::remove_file(m.dir().join("suspended")); // estado salvo na RAM não vale mais
    if was_up { ops::start(m)?; ssh::wait_ssh(m, 90)?; }
    Ok(())
}

/// Ponto automático diário (mantém os últimos AUTO_KEEP).
pub fn auto(m: &Machine) -> Result<()> {
    save(m, &format!("auto-{}", now()))?;
    let autos: Vec<Point> = list(m)?.into_iter().filter(|p| p.auto).collect();
    let extra = autos.len().saturating_sub(AUTO_KEEP);
    for p in autos.iter().take(extra) { let _ = remove(m, &p.name); }
    Ok(())
}
