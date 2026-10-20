//! Ícone na barra de menus do Mac: lista as máquinas, controla o ciclo de vida e,
//! no desktop, mostra um preview ao passar o mouse (clique abre a tela).
use hyzevm::{desktop, ops, qemu, store::{self, Machine}, takeover};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};

#[derive(Clone, PartialEq)]
struct Info {
    name: String,
    state: &'static str,
    desktop: bool,
    takeover: Option<String>,
    always_on: bool,
}

enum Ev {
    Model(Vec<Info>),
    Preview(String, Vec<u8>),
    Menu(MenuEvent),
}

fn snapshot_model() -> Vec<Info> {
    store::list().unwrap_or_default().iter().map(|m| Info {
        name: m.name.clone(), state: qemu::state(m), desktop: m.template == "desktop",
        takeover: takeover::pending(m).map(|t| t.reason), always_on: m.always_on,
    }).collect()
}

/// Glifo monocromático (template) de um monitor, desenhado em pixels.
fn tray_icon_glyph() -> Icon {
    let (w, h) = (44usize, 44usize);
    let mut px = vec![0u8; w * h * 4];
    let mut put = |x: usize, y: usize| { let i = (y * w + x) * 4; px[i + 3] = 255; };
    for y in 9..30 { for x in 6..38 {
        let (dx, dy) = (x.min(37 - (x - 6) % 1) as i32, y as i32);
        let _ = (dx, dy);
        let edge = x < 9 || x >= 35 || y < 12 || y >= 27;
        let corner = (x < 9 || x >= 35) && (y < 12 || y >= 27) && (x == 6 || x == 37) && (y == 9 || y == 29);
        if edge && !corner { put(x, y); }
    } }
    for y in 30..34 { for x in 20..24 { put(x, y); } }
    for x in 13..31 { put(x, 34); put(x, 35); }
    Icon::from_rgba(px, w as u32, h as u32).expect("ícone")
}

fn glyph(i: &Info) -> &'static str {
    if i.takeover.is_some() { "◉" } else { match i.state { "running" => "●", "paused" => "◐", _ => "○" } }
}

/// Devolve o menu e, para cada desktop ligado, o NSMenu do submenu (onde entra o preview).
fn build_menu(model: &[Info], previews: &HashMap<String, Vec<u8>>) -> (Menu, Vec<(String, *mut std::ffi::c_void)>) {
    let mut slots = vec![];
    let menu = Menu::new();
    let head = if model.is_empty() { "HyzeVM — nenhuma máquina".to_string() } else { format!("HyzeVM · {} máquina{}", model.len(), if model.len() == 1 { "" } else { "s" }) };
    let _ = menu.append(&MenuItem::with_id("noop", head, false, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    for i in model {
        let title = match &i.takeover { Some(_) => format!("{} {}  —  precisa de você", glyph(i), i.name), None => format!("{} {}{}", glyph(i), i.name, if i.always_on { "  · 24/7" } else { "" }) };
        let sub = Submenu::new(title, true);
        let id = |a: &str| format!("{a}|{}", i.name);
        if i.desktop && i.state == "running" {
            // O 1º item vira a view de preview — só existe depois que chegou o 1º frame (senão fica uma linha vazia).
            if previews.contains_key(&i.name) {
                let _ = sub.append(&MenuItem::with_id(id("open"), " ", true, None));
                slots.push((i.name.clone(), sub.ns_menu()));
            }
            let _ = sub.append(&MenuItem::with_id(id("open"), "Abrir tela", true, None));
            let _ = sub.append(&PredefinedMenuItem::separator());
        }
        if let Some(reason) = &i.takeover {
            let _ = sub.append(&MenuItem::with_id("noop", reason.clone(), false, None));
            let _ = sub.append(&MenuItem::with_id(id("open"), "Assumir agora", true, None));
            let _ = sub.append(&MenuItem::with_id(id("takeover_done"), "Devolver ao agente", true, None));
            let _ = sub.append(&PredefinedMenuItem::separator());
        }
        match i.state {
            "running" => {
                let _ = sub.append(&MenuItem::with_id(id("suspend"), "Pausar", true, None));
                let _ = sub.append(&MenuItem::with_id(id("stop"), "Desligar", true, None));
                let _ = sub.append(&MenuItem::with_id(id("terminal"), "Abrir terminal", true, None));
            }
            "paused" => { let _ = sub.append(&MenuItem::with_id(id("start"), "Retomar", true, None)); }
            _ => { let _ = sub.append(&MenuItem::with_id(id("start"), "Ligar", true, None)); }
        }
        let _ = menu.append(&sub);
    }
    if !model.is_empty() { let _ = menu.append(&PredefinedMenuItem::separator()); }
    let _ = menu.append(&MenuItem::with_id("quit", "Sair do HyzeVM", true, None));
    (menu, slots)
}

fn open_url(url: &str) { let _ = Command::new("open").arg(url).status(); }

pub fn act(action: &str, name: &str) {
    let Ok(m) = Machine::load(name) else { return };
    let r: Result<()> = (|| {
        match action {
            "start" => ops::start(&m)?,
            "suspend" => qemu::pause(&m)?,
            "stop" => qemu::stop(&m, false)?,
            "open" => open_url(&desktop::view_url(&m)?),
            "takeover_done" => takeover::done(&m)?,
            "terminal" => {
                let exe = std::env::current_exe()?;
                let script = format!("tell application \"Terminal\"\nactivate\ndo script \"{} ssh {}\"\nend tell", exe.display(), name);
                Command::new("osascript").args(["-e", &script]).status()?;
            }
            _ => {}
        }
        Ok(())
    })();
    if let Err(e) = r { takeover::notify(&format!("HyzeVM · {name}"), &format!("{e:#}")); }
}

fn pid_file() -> std::path::PathBuf { store::home().join("menubar.pid") }

fn already_running() -> bool {
    std::fs::read_to_string(pid_file()).ok().and_then(|s| s.trim().parse::<i32>().ok())
        .is_some_and(|p| Command::new("kill").args(["-0", &p.to_string()]).output().is_ok_and(|o| o.status.success()))
}

/// Sobe o ícone em segundo plano (e volta ao terminal).
pub fn launch() -> Result<()> {
    if already_running() { bail!("o menu bar já está rodando"); }
    let mut c = Command::new(std::env::current_exe()?);
    c.args(["menubar", "--foreground"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    use std::os::unix::process::CommandExt;
    c.process_group(0).spawn()?;
    Ok(())
}

pub fn run() -> Result<()> {
    std::fs::create_dir_all(store::home())?;
    std::fs::write(pid_file(), std::process::id().to_string())?;
    let mut event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory); // sem ícone no Dock
    let proxy = event_loop.create_proxy();

    let p = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| { let _ = p.send_event(Ev::Menu(e)); }));

    // Estado das máquinas.
    let p = proxy.clone();
    std::thread::spawn(move || {
        let mut last: Vec<Info> = vec![];
        loop {
            let cur = snapshot_model();
            if cur != last { last = cur.clone(); if p.send_event(Ev::Model(cur)).is_err() { return; } }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    // Previews: só de desktops ligados (máquina pausada/parada não é acordada por isso).
    let p = proxy.clone();
    std::thread::spawn(move || loop {
        for m in store::list().unwrap_or_default().iter().filter(|m| m.template == "desktop" && qemu::state(m) == "running") {
            if let Ok(png) = desktop::screenshot(m) { if let Some(small) = shrink(&png) { let _ = p.send_event(Ev::Preview(m.name.clone(), small)); } }
        }
        std::thread::sleep(Duration::from_secs(5));
    });

    let tray = TrayIconBuilder::new().with_icon_templated(tray_icon_glyph()).with_tooltip("HyzeVM")
        .with_menu(Box::new(build_menu(&[], &HashMap::new()).0)).build()?;
    let mut previews: HashMap<String, Vec<u8>> = HashMap::new();
    let mut model: Vec<Info> = vec![];

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        if let tao::event::Event::UserEvent(ev) = event {
            match ev {
                Ev::Model(m) => {
                    previews.retain(|n, _| m.iter().any(|i| &i.name == n && i.desktop && i.state == "running"));
                    model = m;
                    let (menu, slots) = build_menu(&model, &previews);
                    tray.set_menu(Some(Box::new(menu)));
                    crate::menubar_view::install(&slots, &previews);
                }
                Ev::Preview(name, png) => {
                    let first = !previews.contains_key(&name);
                    crate::menubar_view::update(&name, &png);
                    previews.insert(name, png);
                    if first { // 1º frame: agora o menu ganha a linha do preview
                        let (menu, slots) = build_menu(&model, &previews);
                        tray.set_menu(Some(Box::new(menu)));
                        crate::menubar_view::install(&slots, &previews);
                    }
                }
                Ev::Menu(e) => {
                    let id = e.id.0.clone();
                    if id == "quit" { let _ = std::fs::remove_file(pid_file()); *flow = ControlFlow::Exit; return; }
                    if let Some((a, n)) = id.split_once('|') { let (a, n) = (a.to_string(), n.to_string()); std::thread::spawn(move || act(&a, &n)); }
                }
            }
        }
    })
}

/// 1280×800 → cartão 680×432 (2x de 340×216) com cantos arredondados, em PNG.
fn shrink(png: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    let small = img.resize_exact(640, 400, image::imageops::FilterType::Triangle).to_rgba8();
    let mut canvas = image::RgbaImage::new(680, 432);
    let r = 20.0f32;
    for (x, y, p) in small.enumerate_pixels() {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let cx = fx.clamp(r, 640.0 - r);
        let cy = fy.clamp(r, 400.0 - r);
        if (fx - cx).hypot(fy - cy) <= r { canvas.put_pixel(x + 20, y + 16, *p); }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    canvas.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}
