//! O "Linux modificado do agente": imagem oficial + provisionamento no 1º boot.
//! Templates são só listas de pacotes/comandos — a máquina pronta vira snapshot.
use crate::store::{images_dir, Machine};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub const TEMPLATES: &[(&str, &str)] = &[
    ("minimal", "Debian 12 puro, sem instalar nada — sobe em segundos (padrão; `sudo apt install` o que precisar)"),
    ("base", "Debian + ferramentas de dev (git, python, node, docker, ripgrep, jq, tmux)"),
    ("desktop", "base + desktop (Xvfb+openbox, Chromium, ffmpeg, xdotool, noVNC) para computer use e `hyzevm view`"),
];

const BASE_PKGS: &[&str] = &[
    "git", "curl", "wget", "jq", "ripgrep", "tmux", "build-essential", "python3", "python3-pip",
    "python3-venv", "nodejs", "npm", "docker.io", "docker-compose", "sudo", "unzip", "htop", "openssh-server",
];
const DESKTOP_PKGS: &[&str] = &[
    "xvfb", "xdotool", "x11-utils", "xterm", "x11-xserver-utils", "imagemagick", "openbox", "fonts-noto", "x11vnc", "novnc", "websockify",
    "tint2", "feh", "picom", "adwaita-icon-theme", "pcmanfm",
    "chromium", "ffmpeg", "xclip", "fonts-liberation",
];

pub fn known(t: &str) -> bool { TEMPLATES.iter().any(|(n, _)| *n == t) }

/// Imagem base oficial: Debian 12 genericcloud (leve: ~340 MB), baixada uma vez.
pub fn base_image() -> Result<PathBuf> {
    let (arch, url) = match std::env::consts::ARCH {
        "aarch64" => ("arm64", "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-arm64.qcow2"),
        "x86_64" => ("amd64", "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2"),
        a => bail!("arquitetura {a} não suportada"),
    };
    std::fs::create_dir_all(images_dir())?;
    let path = images_dir().join(format!("debian-12-{arch}.qcow2"));
    if !path.exists() {
        crate::ui::step(&format!("baixando a imagem base — Debian 12 {arch} (uma vez só)"));
        let part = path.with_extension("part");
        let st = Command::new("curl").args(["-fL", "--progress-bar", "-C", "-", "-o"]).arg(&part).arg(url)
            .status().context("curl não encontrado")?;
        if !st.success() { bail!("download falhou"); }
        std::fs::rename(part, &path)?;
    }
    Ok(path)
}


fn unit(desc: &str, after: &str, exec: &str, env: &str) -> String {
    format!("[Unit]\nDescription={desc}\nAfter={after}\n[Service]\nUser=agent\n{env}ExecStart={exec}\nRestart=always\nRestartSec=2\nTimeoutStopSec=5\n[Install]\nWantedBy=multi-user.target\n")
}

fn write_file(path: &str, content: &str, out: &mut String) { write_file_mode(path, content, "0644", out) }

fn write_file_mode(path: &str, content: &str, mode: &str, out: &mut String) {
    out.push_str(&format!("  - path: {path}\n    permissions: '{mode}'\n    content: |\n"));
    for l in content.lines() { out.push_str(&format!("      {l}\n")); }
}

/// Gera a seed NoCloud (user-data/meta-data) na pasta da máquina.
/// `full` = provisionamento completo (build da imagem do template);
/// senão só identidade/chave/volumes — a imagem já traz os pacotes.
pub fn write_seed(m: &Machine, pubkey: &str, full: bool) -> Result<()> {
    let desktop = m.template == "desktop";
    let mut ud = format!(
        "#cloud-config\nhostname: {name}\nmanage_etc_hosts: true\nusers:\n  - name: {user}\n    sudo: ALL=(ALL) NOPASSWD:ALL\n    shell: /bin/bash\n{groups}    ssh_authorized_keys:\n      - {pubkey}\nssh_pwauth: false\n",
        name = m.name, user = m.user, pubkey = pubkey.trim(),
        groups = if m.template == "minimal" { "" } else { "    groups: [docker]\n" });
    // Fuso e idioma do computador do usuário: o IP é local e o navegador não deve dizer "UTC / en-US".
    if let Some(tz) = m.timezone.as_deref().filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))) {
        ud += &format!("timezone: {tz}\n");
    }
    let mut early_files = String::new();
    if let Some(l) = m.lang.as_deref().filter(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        write_file("/etc/hyze-lang", l, &mut early_files);
    }
    let mut runcmd: Vec<String> = vec![];
    if full {
        let mut pkgs: Vec<&str> = BASE_PKGS.to_vec();
        if desktop { pkgs.extend(DESKTOP_PKGS); }
        ud += "package_update: true\npackages:\n";
        for p in pkgs { ud += &format!("  - {p}\n"); }
        // Sem pacotes "recomendados": instala bem menos e bem mais rápido.
        ud += "apt:\n  conf: |\n    APT::Install-Recommends \"false\";\n    Acquire::Languages \"none\";\n";
        ud += "write_files:\n";
        ud += &early_files; early_files.clear();
        if desktop {
            let mut wf = String::new();
            write_file("/etc/systemd/system/hyze-xvfb.service",
                &unit("Xvfb :99", "network.target", "/usr/bin/Xvfb :99 -screen 0 1280x800x24 -nolisten tcp", ""), &mut wf);
            write_file("/etc/systemd/system/hyze-openbox.service",
                &unit("Openbox", "hyze-xvfb.service", "/usr/bin/openbox", "Environment=DISPLAY=:99\n"), &mut wf);
            write_file_mode("/usr/local/bin/hyze-chromium", include_str!("hyze-chromium.sh"), "0755", &mut wf);
            write_file_mode("/usr/local/bin/hyze-session", include_str!("hyze-session.sh"), "0755", &mut wf);
            write_file("/etc/systemd/system/hyze-session.service",
                &unit("Sessão do desktop (fundo + dock)", "hyze-openbox.service", "/usr/local/bin/hyze-session", "Environment=DISPLAY=:99\n"), &mut wf);
            write_file("/etc/systemd/system/hyze-vnc.service",
                &unit("x11vnc", "hyze-xvfb.service", "/usr/bin/x11vnc -display :99 -forever -shared -nopw -rfbport 5900 -noxdamage -quiet", ""), &mut wf);
            write_file("/etc/systemd/system/hyze-novnc.service",
                &unit("noVNC", "hyze-vnc.service", "/usr/bin/websockify --web /usr/share/novnc 6080 127.0.0.1:5900", ""), &mut wf);
            write_file("/etc/systemd/system/hyze-vnc-ro.service",
                &unit("x11vnc somente-leitura", "hyze-xvfb.service", "/usr/bin/x11vnc -display :99 -forever -shared -nopw -viewonly -rfbport 5901 -noxdamage -quiet", ""), &mut wf);
            write_file("/etc/systemd/system/hyze-novnc-ro.service",
                &unit("noVNC somente-leitura", "hyze-vnc-ro.service", "/usr/bin/websockify --web /usr/share/novnc 6081 127.0.0.1:5901", ""), &mut wf);
            ud += &wf;
            runcmd.push("echo DISPLAY=:99 >> /etc/environment".into());
            runcmd.push("ln -sf /usr/share/novnc/vnc.html /usr/share/novnc/index.html".into());
            runcmd.push("systemctl daemon-reload".into());
            runcmd.push("systemctl enable --now hyze-xvfb hyze-openbox hyze-session hyze-vnc hyze-novnc hyze-vnc-ro hyze-novnc-ro".into());
        }
        runcmd.push("systemctl enable --now docker || true".into());
        runcmd.push("mkdir -p /data".into());
        runcmd.push(format!("chown {}: /data", m.user));
    }
    // Volumes persistentes: formata na primeira vez e monta em /data/<nome>.
    for (i, v) in m.volumes.iter().enumerate() {
        let dev = format!("/dev/disk/by-id/virtio-hyzevol{i}");
        runcmd.push(format!(
            "blkid {dev} >/dev/null 2>&1 || mkfs.ext4 -q -L {v} {dev}; mkdir -p /data/{v}; mountpoint -q /data/{v} || mount {dev} /data/{v}; chown {u}: /data/{v}",
            u = m.user));
    }
    if !early_files.is_empty() { ud += "write_files:\n"; ud += &early_files; }
    runcmd.push("touch /var/lib/hyzevm-ready".into());
    ud += "runcmd:\n";
    for c in runcmd { ud += &format!("  - [ sh, -c, \"{}\" ]\n", c.replace('\\', "\\\\").replace('"', "\\\"")); }
    let d = m.seed_dir();
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("user-data"), ud)?;
    std::fs::write(d.join("meta-data"), format!("instance-id: {}\nlocal-hostname: {}\n", m.name, m.name))?;
    Ok(())
}
