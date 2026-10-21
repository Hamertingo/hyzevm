//! `hyzevm doctor`: diagnóstico do ambiente.
use crate::{qemu, store};
use serde_json::json;
use std::process::Command;

/// Verificações estruturadas (nome, ok, detalhe) — usadas pelo CLI e pelo app.
pub fn checks() -> Vec<(String, bool, String)> {
    let mut rows: Vec<(String, bool, String)> = vec![];
    let mut add = |n: &str, ok: bool, d: String| rows.push((n.into(), ok, d));
    let arch = if std::env::consts::ARCH == "aarch64" { "aarch64" } else { "x86_64" };
    add("sistema", true, format!("{} {}", std::env::consts::OS, std::env::consts::ARCH));
    match qemu::qemu_img() { Ok(p) => add("qemu-img", true, p.display().to_string()), Err(e) => add("qemu-img", false, e.to_string()) }
    match qemu::qemu_system() {
        Ok(p) => {
            add(&format!("qemu-system-{arch}"), true, p.display().to_string());
            let out = Command::new(&p).args(["-accel", "help"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            let want = qemu::accel();
            add("aceleração", out.contains(want), format!("{want} {}", if out.contains(want) { "disponível" } else { "ausente (cairá em emulação lenta; no Linux confira /dev/kvm)" }));
        }
        Err(e) => add(&format!("qemu-system-{arch}"), false, e.to_string()),
    }
    #[cfg(windows)]
    {
        let hv = Command::new("powershell").args(["-NoProfile", "-Command", "(Get-CimInstance Win32_ComputerSystem).HypervisorPresent"]).output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().eq_ignore_ascii_case("true")).unwrap_or(false);
        add("Plataforma do Hipervisor", hv, if hv { "ativa".into() } else { "desativada: ative “Plataforma do Hipervisor do Windows” nos recursos do Windows e reinicie".into() });
    }
    match qemu::uefi_check() { Ok(d) => add("firmware UEFI", true, d), Err(e) => add("firmware UEFI", false, e.to_string()) }
    for t in ["ssh", "ssh-keygen", "scp", "curl"] {
        let ok = Command::new(t).arg("-V").output().is_ok() || Command::new(t).arg("--version").output().is_ok();
        add(t, ok, if ok { "ok".into() } else { "não encontrado".to_string() });
    }
    let machines = store::list().map(|v| v.len()).unwrap_or(0);
    add("biblioteca", true, format!("{} · {} máquina(s) · {} snapshot(s) · {} volume(s)", store::home().display(), machines,
        crate::ops::snapshot_list().len(), crate::ops::volume_list().len()));
    rows
}

pub fn run(json_out: bool) -> i32 {
    let rows = checks();
    let all = rows.iter().all(|r| r.1);
    if json_out {
        println!("{}", json!({"ok": all, "checks": rows.iter().map(|(n, ok, d)| json!({"name": n, "ok": ok, "detail": d})).collect::<Vec<_>>()}));
    } else {
        for (n, ok, d) in &rows { println!("{} {n}: {d}", if *ok { "✓" } else { "✗" }); }
    }
    if all { 0 } else { 1 }
}
