//! ask: o agente faz uma pergunta e espera a resposta do humano (CLI, app ou MCP).
use crate::{store::*, takeover};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone)]
pub struct Ask {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub asked_at: u64,
    pub answer: Option<String>,
}

fn dir(m: &Machine) -> std::path::PathBuf { m.dir().join("asks") }

fn read(p: &std::path::Path) -> Option<Ask> { std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()) }

/// Registra a pergunta e avisa o usuário. Devolve o pedido (com o id para esperar/responder).
pub fn send(m: &Machine, question: &str, options: Vec<String>) -> Result<Ask> {
    if question.trim().is_empty() { bail!("a pergunta está vazia"); }
    std::fs::create_dir_all(dir(m))?;
    let t = now();
    let a = Ask { id: format!("{t:x}{:02x}", std::process::id() % 256), question: question.into(), options, asked_at: t, answer: None };
    std::fs::write(dir(m).join(format!("{}.json", a.id)), serde_json::to_vec_pretty(&a)?)?;
    crate::activity::log(m, "ask", question);
    takeover::notify(&format!("{} pergunta", m.name), question);
    Ok(a)
}

pub fn get(m: &Machine, id: &str) -> Option<Ask> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) { return None; }
    read(&dir(m).join(format!("{id}.json")))
}

/// Perguntas sem resposta, da mais antiga para a mais nova.
pub fn pending(m: &Machine) -> Vec<Ask> {
    let mut v: Vec<Ask> = std::fs::read_dir(dir(m)).into_iter().flatten().flatten().filter_map(|e| read(&e.path())).filter(|a| a.answer.is_none()).collect();
    v.sort_by_key(|a| a.asked_at);
    v
}

pub fn answer(m: &Machine, id: &str, text: &str) -> Result<()> {
    let Some(mut a) = get(m, id) else { bail!("pergunta {id} não encontrada em {}", m.name) };
    if a.answer.is_some() { bail!("a pergunta {id} já foi respondida"); }
    if !a.options.is_empty() && !a.options.iter().any(|o| o.eq_ignore_ascii_case(text)) {
        bail!("resposta inválida — opções: {}", a.options.join(", "));
    }
    crate::activity::log(m, "answer", &format!("{} → {text}", a.question));
    a.answer = Some(text.to_string());
    std::fs::write(dir(m).join(format!("{id}.json")), serde_json::to_vec_pretty(&a)?)?;
    Ok(())
}

/// Espera a resposta. Some(resposta) ou None se o tempo acabou (a pergunta continua pendente).
pub fn wait(m: &Machine, id: &str, secs: u64) -> Result<Option<String>> {
    let t0 = Instant::now();
    loop {
        match get(m, id) {
            None => bail!("pergunta {id} não encontrada em {}", m.name),
            Some(a) if a.answer.is_some() => return Ok(a.answer),
            _ => {}
        }
        if t0.elapsed() >= Duration::from_secs(secs) { return Ok(None); }
        std::thread::sleep(Duration::from_secs(1));
    }
}
