//! Saída de terminal consistente: cores (só em TTY, respeita NO_COLOR), tabelas e mensagens.
use std::io::IsTerminal;

use std::sync::{Mutex, OnceLock};

type Hook = Box<dyn Fn(&str) + Send + Sync>;
static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();

/// O app registra um callback para receber o progresso (create etc.) em vez de ler o terminal.
pub fn set_hook(f: Option<Hook>) { *HOOK.get_or_init(|| Mutex::new(None)).lock().unwrap() = f; }
fn emit(msg: &str) { if let Some(h) = HOOK.get() { if let Some(f) = h.lock().unwrap().as_ref() { f(msg); } } }

fn color_on(stream_is_tty: bool) -> bool {
    stream_is_tty && std::env::var_os("NO_COLOR").is_none() && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true)
}
pub fn out_color() -> bool { color_on(std::io::stdout().is_terminal()) }
pub fn err_color() -> bool { color_on(std::io::stderr().is_terminal()) }

fn paint(on: bool, code: &str, s: &str) -> String { if on { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() } }

// stdout
pub fn bold(s: &str) -> String { paint(out_color(), "1", s) }
pub fn dim(s: &str) -> String { paint(out_color(), "2", s) }
pub fn green(s: &str) -> String { paint(out_color(), "32", s) }
pub fn yellow(s: &str) -> String { paint(out_color(), "33", s) }
pub fn cyan(s: &str) -> String { paint(out_color(), "36", s) }

// stderr (mensagens de progresso/erro não poluem o stdout dos scripts)
fn e(code: &str, s: &str) -> String { paint(err_color(), code, s) }

pub fn ok(msg: &str) { eprintln!("{} {msg}", e("32", "✓")); }
pub fn step(msg: &str) { emit(msg); eprintln!("{} {msg}", e("36", "▸")); }
/// Linha de detalhe de uma etapa longa (ex.: o que o guest está instalando).
pub fn detail(msg: &str) { emit(msg); eprintln!("  {}", e("2", msg)); }
pub fn warn(msg: &str) { eprintln!("{} {msg}", e("33", "!")); }
pub fn hint(msg: &str) { eprintln!("  {}", e("2", msg)); }
pub fn fail(msg: &str, hint_msg: Option<&str>) {
    eprintln!("{} {msg}", e("1;31", "erro:"));
    if let Some(h) = hint_msg { eprintln!("  {} {h}", e("2", "→")); }
}

/// Bolinha de estado: ● verde (running), ◐ amarelo (paused), ○ cinza (stopped).
pub fn state(s: &str) -> String {
    match s {
        "running" => format!("{} {}", green("●"), "running"),
        "paused" => format!("{} {}", yellow("◐"), "paused"),
        _ => format!("{} {}", dim("○"), dim(s)),
    }
}

fn visible_len(s: &str) -> usize {
    let (mut n, mut esc) = (0, false);
    for c in s.chars() {
        if esc { if c == 'm' { esc = false; } } else if c == '\x1b' { esc = true; } else { n += 1; }
    }
    n
}

/// Tabela alinhada; cabeçalho em dim.
pub fn table(header: &[&str], rows: &[Vec<String>]) {
    let cols = header.len();
    let mut w: Vec<usize> = header.iter().map(|h| h.len()).collect();
    for r in rows { for (i, c) in r.iter().enumerate().take(cols) { w[i] = w[i].max(visible_len(c)); } }
    let line = |cells: Vec<String>| {
        let mut s = String::new();
        for (i, c) in cells.iter().enumerate() {
            s += c;
            if i + 1 < cols { s += &" ".repeat(w[i] - visible_len(c) + 3); }
        }
        println!("{}", s.trim_end());
    };
    line(header.iter().map(|h| dim(h)).collect());
    for r in rows { line(r.clone()); }
}

/// Rótulo: valor (alinhado) para cartões de detalhe.
pub fn kv(k: &str, v: &str) { println!("  {}  {v}", dim(&format!("{k:<10}"))); }

pub fn humanize_secs(s: u64) -> String {
    match s { 0..=59 => format!("{s}s"), 60..=3599 => format!("{}m", s / 60), 3600..=86399 => format!("{}h", s / 3600), _ => format!("{}d", s / 86400) }
}

/// Horário local de um timestamp unix: "14:03:22" (hoje) ou "19/10 14:03" (outros dias).
pub fn clock(ts: u64) -> String {
    use chrono::{Local, TimeZone};
    let Some(t) = Local.timestamp_opt(ts as i64, 0).single() else { return ts.to_string() };
    if t.date_naive() == Local::now().date_naive() { t.format("%H:%M:%S").to_string() } else { t.format("%d/%m %H:%M").to_string() }
}
