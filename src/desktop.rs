//! Computer use: age no desktop virtual (Xvfb :99) do guest via xdotool/ImageMagick.
use crate::{activity, ssh, store::Machine};
use anyhow::{bail, Result};

fn run(m: &Machine, cmd: &str) -> Result<Vec<u8>> {
    let o = ssh::exec_capture(m, &format!("export DISPLAY=:99; {cmd}"))?;
    if !o.status.success() {
        bail!("{} (a máquina tem o template `desktop`?)", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(o.stdout)
}

pub fn screenshot(m: &Machine) -> Result<Vec<u8>> { run(m, "import -window root png:-") }

/// Miniatura leve (JPEG 640×400) para grades/menus: ~30 KB em vez de ~300 KB do PNG cheio.
pub fn thumbnail(m: &Machine) -> Result<Vec<u8>> { run(m, "import -window root -resize 640x400 -quality 72 jpg:-") }

pub fn click(m: &Machine, x: i32, y: i32, button: &str, double: bool) -> Result<()> {
    let b = match button { "left" => 1, "middle" => 2, "right" => 3, o => bail!("botão “{o}” (left|middle|right)") };
    let rep = if double { "--repeat 2 --delay 80" } else { "" };
    activity::log(m, "desktop", &format!("clique {button}{} em ({x}, {y})", if double { " duplo" } else { "" }));
    run(m, &format!("xdotool mousemove {x} {y} click {rep} {b}")).map(|_| ())
}

pub fn mv(m: &Machine, x: i32, y: i32) -> Result<()> { activity::log(m, "desktop", &format!("mouse para ({x}, {y})")); run(m, &format!("xdotool mousemove {x} {y}")).map(|_| ()) }

pub fn drag(m: &Machine, from: (i32, i32), to: (i32, i32)) -> Result<()> {
    activity::log(m, "desktop", &format!("arrastar de {:?} para {:?}", from, to));
    run(m, &format!("xdotool mousemove {} {} mousedown 1 mousemove {} {} mouseup 1", from.0, from.1, to.0, to.1)).map(|_| ())
}

pub fn type_text(m: &Machine, text: &str) -> Result<()> {
    activity::log(m, "desktop", &format!("digitou {} caracteres", text.chars().count()));
    run(m, &format!("xdotool type --delay 8 -- {}", ssh::sh_quote(text))).map(|_| ())
}

/// Tecla ou combinação: "Enter", "ctrl+l", "alt+Tab".
pub fn press(m: &Machine, keys: &str) -> Result<()> {
    activity::log(m, "desktop", &format!("tecla {keys}"));
    run(m, &format!("xdotool key -- {}", ssh::sh_quote(keys))).map(|_| ())
}

pub fn scroll(m: &Machine, dir: &str, amount: u32) -> Result<()> {
    let b = match dir { "up" => 4, "down" => 5, "left" => 6, "right" => 7, o => bail!("direção “{o}” (up|down|left|right)") };
    activity::log(m, "desktop", &format!("rolou {dir} ×{amount}"));
    run(m, &format!("xdotool click --repeat {amount} --delay 30 {b}")).map(|_| ())
}

pub fn cursor(m: &Machine) -> Result<String> {
    Ok(String::from_utf8_lossy(&run(m, "xdotool getmouselocation")?).trim().to_string())
}

/// Abre uma URL no Chromium do guest (o mesmo que o `browser` controla; perfil persistente).
pub fn open(m: &Machine, url: &str) -> Result<()> {
    crate::browser::run(m, &["goto", url]).map(|_| ())
}

pub fn view_url(m: &Machine) -> Result<String> {
    if m.template != "desktop" || m.view_port == 0 {
        bail!("{} usa o template “{}”, sem tela. Crie uma com desktop: hyzevm create <nome> --template desktop", m.name, m.template);
    }
    Ok(format!("http://127.0.0.1:{}/vnc.html?autoconnect=1&resize=scale&reconnect=1", m.view_port))
}
