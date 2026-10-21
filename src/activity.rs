//! Registro de ações por máquina (`activity.jsonl`): o que o agente (e você) fez, com horário.
use crate::store::*;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::OnceLock;

static ACTOR: OnceLock<&'static str> = OnceLock::new();

/// Quem está agindo neste processo: "cli", "mcp", "app" ou "watch". Chamado uma vez no início.
pub fn set_actor(a: &'static str) { let _ = ACTOR.set(a); }

#[derive(Serialize, Deserialize, Clone)]
pub struct Event {
    pub ts: u64,
    pub actor: String,
    pub kind: String,
    pub detail: String,
}

fn file(m: &Machine) -> std::path::PathBuf { m.dir().join("activity.jsonl") }

fn clip(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ⏎ ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// Anota uma ação (nunca falha o comando principal). Segredos: não passe texto digitado, só o tamanho.
pub fn log(m: &Machine, kind: &str, detail: &str) {
    let p = file(m);
    // rotação simples: passou de 4 MB, o arquivo vira .1 (o antigo .1 é descartado)
    if std::fs::metadata(&p).is_ok_and(|x| x.len() > 4 << 20) { let _ = std::fs::rename(&p, p.with_extension("jsonl.1")); }
    let e = Event { ts: now(), actor: ACTOR.get().copied().unwrap_or("cli").into(), kind: kind.into(), detail: clip(detail, 300) };
    if let (Ok(line), Ok(mut f)) = (serde_json::to_string(&e), std::fs::OpenOptions::new().create(true).append(true).open(&p)) {
        let _ = writeln!(f, "{line}");
    }
}

/// Últimos `limit` eventos (mais antigo primeiro), a partir de `since` (segundos unix; 0 = tudo).
pub fn read(m: &Machine, limit: usize, since: u64) -> Vec<Event> {
    let mut v: Vec<Event> = [file(m).with_extension("jsonl.1"), file(m)].iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|t| t.lines().filter_map(|l| serde_json::from_str::<Event>(l).ok()).collect::<Vec<_>>())
        .filter(|e| e.ts >= since).collect();
    let n = v.len();
    if n > limit { v.drain(..n - limit); }
    v
}
