// Sem janela de console no Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hyzevm::{update, desktop, doctor, ops, qemu, share, ssh, store::{self, Machine}, takeover, ui, watch};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

type R<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String { format!("{e:#}") }

/// Roda trabalho bloqueante (QEMU, SSH) fora da thread da UI.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?.map_err(err)
}

#[derive(Serialize)]
struct Dto {
    name: String,
    state: &'static str,
    template: String,
    cpus: u32,
    memory_mb: u32,
    disk_gb: u32,
    always_on: bool,
    has_screen: bool,
    ports: Vec<(u16, u16)>,
    takeover: Option<String>,
    auto_suspend_secs: u64,
    ssh_port: u16,
    meta: BTreeMap<String, String>,
}

fn dto(m: &Machine) -> Dto {
    Dto {
        name: m.name.clone(), state: qemu::state(m), template: m.template.clone(), cpus: m.cpus, memory_mb: m.memory_mb,
        disk_gb: m.disk_gb, always_on: m.always_on, has_screen: m.template == "desktop", ports: m.ports.clone(),
        takeover: takeover::pending(m).map(|t| t.reason), auto_suspend_secs: m.auto_suspend_secs, ssh_port: m.ssh_port, meta: m.meta.clone(),
    }
}

#[tauri::command]
async fn platform() -> String { std::env::consts::OS.to_string() }

#[tauri::command]
async fn list_machines() -> R<Vec<Dto>> { blocking(|| Ok(store::list()?.iter().map(dto).collect())).await }

#[tauri::command]
async fn machine_action(name: String, action: String) -> R<()> {
    blocking(move || {
        let m = Machine::load(&name)?;
        match action.as_str() {
            "start" => ops::start(&m)?,
            "suspend" | "pause" => qemu::pause(&m)?,
            "suspend_disk" => qemu::suspend(&m)?,
            "stop" => qemu::stop(&m, false)?,
            "force_stop" => qemu::stop(&m, true)?,
            "remove" => ops::remove(&m)?,
            a => anyhow::bail!("ação desconhecida: {a}"),
        }
        Ok(())
    }).await
}

#[derive(Deserialize)]
struct CreateReq { name: String, template: String, cpus: u32, memory_gb: u32, disk_gb: u32, always_on: bool }

static CREATING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
async fn create_machine(app: tauri::AppHandle, req: CreateReq) -> R<()> {
    if CREATING.swap(true, Ordering::SeqCst) { return Err("já existe uma criação em andamento".into()); }
    let name = req.name.clone();
    ui::set_hook(Some(Box::new(move |msg| { let _ = app.emit("create-progress", serde_json::json!({ "name": name, "msg": msg })); })));
    let r = blocking(move || {
        let mut o = ops::CreateOpts::simple(&req.name, &req.template);
        o.cpus = req.cpus; o.memory_mb = req.memory_gb * 1024; o.disk_gb = req.disk_gb; o.always_on = req.always_on;
        ops::create(o).map(|_| ())
    }).await;
    ui::set_hook(None);
    CREATING.store(false, Ordering::SeqCst);
    r
}

/// Link da tela dentro do app: gateway local com token (somente-leitura ou controle).
#[tauri::command]
async fn screen_url(name: String, control: bool) -> R<String> {
    blocking(move || {
        let m = Machine::load(&name)?;
        ops::start(&m)?;
        Ok(share::create(&m, !control, 3600)?.1)
    }).await
}

/// URL do WebSocket da tela (RFB) para o visor embutido do app. Um token de controle por chamada (6 h).
#[tauri::command]
async fn screen_ws(name: String) -> R<String> {
    blocking(move || {
        let m = Machine::load(&name)?;
        ops::start(&m)?;
        // O Caps Lock da VM nunca é encaminhado (o case já vem nas teclas); se ficou ligado de uma sessão
        // antiga, desliga para não inverter as letras.
        let _ = ssh::exec_capture(&m, "export DISPLAY=:99; xset q 2>/dev/null | grep -q 'Caps Lock: *on' && xdotool key Caps_Lock; true");
        let (token, url, _) = share::create(&m, false, 6 * 3600)?;
        let host = url.split("/s/").next().unwrap_or("http://127.0.0.1").replacen("http://", "ws://", 1);
        Ok(format!("{host}/s/{token}/websockify"))
    }).await
}

#[derive(Serialize, Default)]
struct Stats { cpu_total: u64, cpu_idle: u64, cores: u32, mem_total_mb: u64, mem_used_mb: u64, disk_total_gb: f64, disk_used_gb: f64, uptime_secs: u64 }

/// Uso ao vivo lido de /proc e df dentro do guest (não conta como atividade: não adia o auto-suspend).
#[tauri::command]
async fn machine_stats(name: String) -> R<Stats> {
    blocking(move || {
        let m = Machine::load(&name)?;
        if qemu::state(&m) != "running" { anyhow::bail!("máquina parada"); }
        let o = ssh::exec_capture(&m, "head -1 /proc/stat; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo; df -Pk / | tail -1; cut -d' ' -f1 /proc/uptime; nproc")?;
        if !o.status.success() { anyhow::bail!("sem resposta"); }
        let txt = String::from_utf8_lossy(&o.stdout).to_string();
        let mut st = Stats::default();
        let (mut mt, mut ma) = (0u64, 0u64);
        for l in txt.lines() {
            let f: Vec<&str> = l.split_whitespace().collect();
            if l.starts_with("cpu ") {
                let v: Vec<u64> = f[1..].iter().filter_map(|x| x.parse().ok()).collect();
                st.cpu_total = v.iter().sum();
                st.cpu_idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            } else if l.starts_with("MemTotal:") { mt = f[1].parse().unwrap_or(0); }
            else if l.starts_with("MemAvailable:") { ma = f[1].parse().unwrap_or(0); }
            else if f.len() >= 6 && f[0].starts_with('/') { st.disk_total_gb = f[1].parse::<f64>().unwrap_or(0.0) / 1048576.0; st.disk_used_gb = f[2].parse::<f64>().unwrap_or(0.0) / 1048576.0; }
            else if f.len() == 1 && f[0].contains('.') { st.uptime_secs = f[0].parse::<f64>().unwrap_or(0.0) as u64; }
            else if f.len() == 1 { st.cores = f[0].parse().unwrap_or(0); }
        }
        st.mem_total_mb = mt / 1024; st.mem_used_mb = mt.saturating_sub(ma) / 1024;
        Ok(st)
    }).await
}

#[tauri::command]
async fn set_auto_suspend(name: String, secs: u64) -> R<()> {
    blocking(move || { let mut m = Machine::load(&name)?; m.auto_suspend_secs = secs; if secs > 0 { m.always_on = false; } m.save()?; Ok(()) }).await
}

/// Miniatura da tela (data URL JPEG) — só lê, não acorda nem conta como atividade.
#[tauri::command]
async fn machine_thumb(name: String) -> R<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    blocking(move || {
        let m = Machine::load(&name)?;
        if qemu::state(&m) != "running" || m.template != "desktop" { anyhow::bail!("sem tela"); }
        Ok(format!("data:image/jpeg;base64,{}", STANDARD.encode(desktop::thumbnail(&m)?)))
    }).await
}

/// Área de transferência do computador (o webview não deixa ler/escrever sem permissão).
#[tauri::command]
async fn clipboard_read() -> R<String> {
    blocking(|| Ok(arboard::Clipboard::new()?.get_text().unwrap_or_default())).await
}
#[tauri::command]
async fn clipboard_write(text: String) -> R<()> {
    blocking(move || { arboard::Clipboard::new()?.set_text(text)?; Ok(()) }).await
}

/// ⌘V na VM: leva o texto do computador para a área de transferência do Linux (o app em seguida manda Ctrl+V).
#[tauri::command]
async fn clipboard_to_vm(name: String) -> R<usize> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    blocking(move || {
        let text = arboard::Clipboard::new()?.get_text().unwrap_or_default();
        let m = Machine::load(&name)?;
        let b64 = STANDARD.encode(text.as_bytes());
        let sc = format!("export DISPLAY=:99\nprintf %s '{b64}' | base64 -d | xclip -selection clipboard -i >/dev/null 2>&1\nprintf %s '{b64}' | base64 -d | xclip -selection primary -i >/dev/null 2>&1\n");
        let o = ssh::exec_stdin(&m, &sc)?;
        if !o.status.success() { anyhow::bail!("{}", String::from_utf8_lossy(&o.stderr).trim()); }
        Ok(text.chars().count())
    }).await
}

/// ⌘C na VM: traz o que o Linux copiou para a área de transferência do computador.
#[tauri::command]
async fn clipboard_from_vm(name: String) -> R<usize> {
    blocking(move || {
        let m = Machine::load(&name)?;
        let o = ssh::exec_capture(&m, "DISPLAY=:99 xclip -selection clipboard -o 2>/dev/null")?;
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        if o.status.success() && !text.is_empty() { arboard::Clipboard::new()?.set_text(text.clone())?; }
        Ok(text.chars().count())
    }).await
}

/// Digita texto no desktop da máquina (xdotool) — o jeito confiável de preencher campos.
#[tauri::command]
async fn desktop_type(name: String, text: String) -> R<()> {
    blocking(move || { let m = Machine::load(&name)?; ops::start(&m)?; desktop::type_text(&m, &text) }).await
}

/// Salva a captura (data URL PNG) em Downloads e devolve o caminho.
#[tauri::command]
async fn save_screenshot(name: String, data_url: String) -> R<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    blocking(move || {
        let b64 = data_url.split_once(',').map(|x| x.1).unwrap_or(&data_url);
        let bytes = STANDARD.decode(b64)?;
        let dir = dirs::download_dir().or_else(dirs::home_dir).ok_or_else(|| anyhow::anyhow!("sem pasta de downloads"))?;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        let path = dir.join(format!("{name}-{secs}.png"));
        std::fs::write(&path, bytes)?;
        if cfg!(target_os = "macos") { let _ = std::process::Command::new("open").arg("-R").arg(&path).status(); }
        Ok(path.display().to_string())
    }).await
}

#[derive(Serialize)]
struct ExecOut { code: i32, stdout: String, stderr: String }

#[tauri::command]
async fn exec(name: String, command: String) -> R<ExecOut> {
    blocking(move || {
        let m = Machine::load(&name)?;
        let was_up = qemu::state(&m) == "running";
        ops::start(&m)?;
        if !was_up { ssh::wait_ssh(&m, 120)?; }
        let o = ssh::exec_capture(&m, &command)?;
        Ok(ExecOut { code: o.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&o.stdout).into(), stderr: String::from_utf8_lossy(&o.stderr).into() })
    }).await
}

#[tauri::command]
async fn takeover_done(name: String) -> R<()> { blocking(move || takeover::done(&Machine::load(&name)?)).await }

#[tauri::command]
async fn set_always_on(name: String, on: bool) -> R<()> {
    blocking(move || {
        let mut m = Machine::load(&name)?;
        m.always_on = on;
        if on { m.auto_suspend_secs = 0; } else if m.auto_suspend_secs == 0 { m.auto_suspend_secs = 1800; }
        m.save()?;
        if on { ops::start(&m)?; }
        Ok(())
    }).await
}

#[tauri::command]
async fn open_terminal(name: String) -> R<()> {
    blocking(move || {
        let exe = std::env::current_exe()?;
        // O binário `hyzevm` (CLI) fica ao lado do app; senão usa o do PATH.
        // Pacote do macOS: Contents/SharedSupport/hyzevm (não pode ficar em MacOS/: o FS ignora caixa e
        // "hyzevm" colidiria com "HyzeVM"). Ao lado do app (Windows/Linux) ou, por fim, no PATH.
        let bin = if cfg!(windows) { "hyzevm.exe" } else { "hyzevm" };
        let cands = [exe.with_file_name(bin), exe.with_file_name("../SharedSupport").join(bin)];
        let cli = cands.iter().find(|p| p.exists()).map(|p| p.display().to_string()).unwrap_or_else(|| "hyzevm".to_string());
        match std::env::consts::OS {
            "macos" => { std::process::Command::new("osascript").args(["-e", &format!("tell application \"Terminal\"\nactivate\ndo script \"'{cli}' ssh {name}\"\nend tell")]).status()?; }
            "windows" => { std::process::Command::new("cmd").args(["/c", "start", "cmd", "/k", &format!("\"{cli}\" ssh {name}")]).spawn()?; }
            _ => { std::process::Command::new("x-terminal-emulator").args(["-e", &cli, "ssh", &name]).spawn()?; }
        }
        Ok(())
    }).await
}

#[derive(Serialize)]
struct Upd { version: String, current: String, notes: String, page: String, can_install: bool }

/// Há versão nova? (consulta o GitHub no máx. a cada 6 h, ou sempre com `force`).
#[tauri::command]
async fn update_check(force: bool) -> R<Option<Upd>> {
    blocking(move || Ok(update::check(force)?.map(|r| Upd {
        can_install: update::app_asset_name().is_some_and(|n| r.assets.iter().any(|(a, _)| *a == n)) && update::running_bundle().is_some(),
        version: r.version, current: update::CURRENT.to_string(), notes: r.notes, page: r.page,
    }))).await
}

/// Baixa e verifica a versão nova, agenda a troca do .app e fecha para o script reabrir já atualizado.
#[tauri::command]
async fn update_install(app: tauri::AppHandle) -> R<()> {
    blocking(|| {
        let rel = update::check(true)?.ok_or_else(|| anyhow::anyhow!("você já está na versão mais recente"))?;
        let new_app = update::download_app(&rel)?;
        update::swap_app_and_relaunch(&new_app, std::process::id())
    }).await?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn open_url(url: String) -> R<()> {
    if !url.starts_with("https://github.com/") { return Err("endereço não permitido".into()); }
    blocking(move || {
        let (bin, args): (&str, Vec<&str>) = match std::env::consts::OS { "macos" => ("open", vec![]), "windows" => ("cmd", vec!["/c", "start", ""]), _ => ("xdg-open", vec![]) };
        std::process::Command::new(bin).args(args).arg(&url).spawn()?; Ok(())
    }).await
}

#[derive(Serialize)]
struct Check { name: String, ok: bool, detail: String }

#[tauri::command]
async fn doctor_report() -> R<Vec<Check>> {
    blocking(|| Ok(doctor::checks().into_iter().map(|(name, ok, detail)| Check { name, ok, detail }).collect())).await
}

/// Windows: app GUI sem console faria cada ssh/qemu-img filho abrir uma janela preta piscando.
/// Cria um console oculto que os filhos herdam.
#[cfg(windows)]
fn hide_child_consoles() {
    #[link(name = "kernel32")] extern "system" { fn AllocConsole() -> i32; fn GetConsoleWindow() -> isize; }
    #[link(name = "user32")] extern "system" { fn ShowWindow(h: isize, cmd: i32) -> i32; }
    unsafe { if AllocConsole() != 0 { let w = GetConsoleWindow(); if w != 0 { ShowWindow(w, 0); } } }
}

fn main() {
    #[cfg(windows)]
    if !std::env::args().nth(1).is_some_and(|a| a.starts_with('_')) { hide_child_consoles(); }
    // Processos auxiliares (vigia e gateway) são esta mesma binária chamada com argumentos.
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("_watch") if a.len() > 2 => { let _ = watch::run(&a[2]); return; }
        Some("_gateway") if a.len() > 2 => { if let Ok(p) = a[2].parse() { let _ = share::run_gateway(p); } return; }
        _ => {}
    }
    tauri::Builder::default()
        .setup(|app| {
            // Menu enxuto no macOS, sem ⌘V/⌘C/⌘X/⌘A/⌘W/⌘M: esses atalhos precisam chegar ao app e à VM.
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
                let h = app.handle();
                let app_menu = Submenu::with_items(h, "HyzeVM", true, &[
                    &PredefinedMenuItem::about(h, Some("Sobre o HyzeVM"), None)?,
                    &PredefinedMenuItem::separator(h)?,
                    &PredefinedMenuItem::hide(h, Some("Ocultar o HyzeVM"))?,
                    &PredefinedMenuItem::separator(h)?,
                    &PredefinedMenuItem::quit(h, Some("Sair do HyzeVM"))?,
                ])?;
                app.set_menu(Menu::with_items(h, &[&app_menu])?)?;
            }
            let _ = desktop::view_url; // mantém o módulo linkado em todas as plataformas
            let _ = app;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform, list_machines, machine_action, create_machine, screen_url, screen_ws, machine_stats, set_auto_suspend, desktop_type, save_screenshot, machine_thumb, update_check, update_install, open_url,
            clipboard_read, clipboard_write, clipboard_to_vm, clipboard_from_vm, exec, takeover_done, set_always_on, open_terminal, doctor_report
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o app");
}
