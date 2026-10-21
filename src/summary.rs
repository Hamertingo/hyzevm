//! Resumo por máquina (o que fez, o que falhou) e alertas de comportamento estranho.
use crate::{activity, store::*, ui};
use std::collections::BTreeMap;

pub fn build(m: &Machine, since_secs: u64) -> String {
    let ev = activity::read(m, 100_000, now().saturating_sub(since_secs));
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &ev { *by.entry(e.kind.as_str()).or_default() += 1; }
    let n = |k: &str| by.get(k).copied().unwrap_or(0);
    let denied = ev.iter().filter(|e| e.kind == "approve" && e.detail.starts_with("RECUSADO")).count();
    let approved = ev.iter().filter(|e| e.kind == "approve" && e.detail.starts_with("aprovado")).count();
    let mut out = format!("{} — últimas {}\n", m.name, ui::humanize_secs(since_secs));
    if ev.is_empty() { out.push_str("  nenhuma ação registrada\n"); return out; }
    out.push_str(&format!("  {} ações: {} comandos, {} no navegador, {} na tela, {} arquivos enviados\n",
        ev.len(), n("exec"), n("browser"), n("desktop"), n("cp") + n("write")));
    if n("ask") + n("approve") + n("takeover") > 0 {
        out.push_str(&format!("  {} perguntas · {} aprovadas · {} recusadas · {} pedidos de ajuda\n", n("ask"), approved, denied, n("takeover") / 2));
    }
    if n("health") + n("alert") > 0 { out.push_str(&format!("  ⚠ {} travadas/alertas:\n", n("health") + n("alert"))); }
    for e in ev.iter().filter(|e| e.kind == "health" || e.kind == "alert") { out.push_str(&format!("    {}  {}\n", ui::clock(e.ts), e.detail)); }
    if let (Some(a), Some(b)) = (ev.first(), ev.last()) { out.push_str(&format!("  primeira ação {} · última {}\n", ui::clock(a.ts), ui::clock(b.ts))); }
    out
}
