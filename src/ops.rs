//! Operações compartilhadas entre o CLI e o MCP.
use crate::{qemu, ssh, store::*, template, vault, watch};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct CreateOpts {
    pub name: String,
    pub template: String,
    pub snapshot: Option<String>,
    pub cpus: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub ports: Vec<(u16, u16)>,
    pub wait: bool,
    pub meta: BTreeMap<String, String>,
    pub auto_suspend: Option<u64>,
    pub timeout: Option<u64>,
    pub on_timeout: String,
    pub volumes: Vec<String>,
    pub vault: bool,
    pub always_on: bool,
}

impl CreateOpts {
    pub fn simple(name: &str, template: &str) -> Self {
        CreateOpts { name: name.into(), template: template.into(), snapshot: None, cpus: 4, memory_mb: 4096,
            disk_gb: 32, ports: vec![], wait: true, meta: BTreeMap::new(), auto_suspend: None, timeout: None,
            on_timeout: "pause".into(), volumes: vec![], vault: true, always_on: false }
    }
}

fn template_image_path(tpl: &str) -> PathBuf {
    images_dir().join(format!("tpl-{tpl}-{}.qcow2", std::env::consts::ARCH))
}

/// Liga a máquina, marca atividade e garante o watcher (auto-suspend/timeout/vault).
pub fn start(m: &Machine) -> Result<()> {
    let how = qemu::start(m)?;
    if !matches!(how, qemu::Started::Already) { crate::activity::log(m, "power", match how { qemu::Started::Live => "retomou", _ => "ligou" }); }
    m.touch();
    watch::ensure(m)?;
    let has_key = m.dir().join("id_ed25519").exists();
    match how {
        qemu::Started::Live => sync_clock(m),
        // Estado salvo que não volta a responder (visto após horas suspensa: guest congelado):
        // reinicia do disco em vez de deixar a máquina presa. Perde só o que estava na memória.
        qemu::Started::Disk if has_key => {
            if ssh::wait_ssh(m, 25).is_err() {
                crate::ui::warn(&format!("{} não voltou do estado salvo — reiniciando do disco", m.name));
                qemu::system_reset(m)?;
                let _ = std::fs::write(m.dir().join("restore-error.txt"), format!("{}: estado salvo não respondeu; reiniciada a frio\n", now()));
            } else { sync_clock(m); }
        }
        _ => {}
    }
    Ok(())
}

/// Depois de pausada, o relógio do guest ficou parado: acerta pelo do host (melhor esforço).
fn sync_clock(m: &Machine) {
    let _ = ssh::exec_capture(m, &format!("sudo -n date -u -s @{} >/dev/null 2>&1", now()));
}

fn provision(m: &Machine, backing: &Path, full: bool, wait: bool) -> Result<()> {
    for v in &m.volumes {
        if !volumes_dir().join(format!("{v}.qcow2")).exists() { bail!("volume “{v}” não existe (`hyzevm volume create {v}`)"); }
    }
    crate::ui::step("criando disco");
    qemu::create_disk(m, backing)?;
    let pubkey = ssh::keygen(m)?;
    template::write_seed(m, &pubkey, full)?;
    crate::ui::step("ligando");
    start(m)?;
    if wait {
        crate::ui::step(if full { "instalando o template (só desta vez)" } else { "aguardando o boot" });
        ssh::wait_ready(m, if full { 1800 } else { 300 })?;
        if m.vault && !full { let _ = vault::push(m); }
    }
    Ok(())
}

/// Imagem pronta do template (pacotes já instalados): construída uma vez, depois
/// cada `create` leva segundos.
fn ensure_template_image(tpl: &str) -> Result<PathBuf> {
    let img = template_image_path(tpl);
    if img.exists() { return Ok(img); }
    eprintln!("primeira vez com o template “{tpl}”: preparando a imagem (alguns minutos, só uma vez)…");
    let base = template::base_image()?;
    let m = Machine {
        name: format!("_build-{tpl}"), template: tpl.into(), cpus: 4, memory_mb: 4096, disk_gb: 16,
        ssh_port: free_port()?, qmp_port: free_port()?, ports: vec![], user: "agent".into(), view_port: 0, view_ro_port: 0,
        auto_suspend_secs: 0, expires_at: None, on_timeout: "pause".into(), meta: BTreeMap::new(),
        volumes: vec![], vault: false, always_on: false, record_keep_secs: 0, auto_checkpoint: false, guard: vec![], dry_run: false, net_allow: vec![], net_limit_kbps: 0, net_proxy_port: 0, timezone: None, lang: None,
    };
    let _ = remove(&m); // sobra de build interrompido
    m.save()?;
    let r = (|| -> Result<()> {
        provision(&m, &base, true, true)?;
        // cloud-init precisa terminar de vez antes de desligar.
        let _ = ssh::exec_capture(&m, "sudo cloud-init status --wait >/dev/null 2>&1; sudo cloud-init clean --logs --seed >/dev/null 2>&1; sync");
        qemu::stop(&m, false).or_else(|_| qemu::stop(&m, true))?;
        std::fs::create_dir_all(images_dir())?;
        let part = img.with_extension("part");
        let st = Command::new(qemu::qemu_img()?).args(["convert", "-O", "qcow2"]).arg(m.disk()).arg(&part).status()?;
        if !st.success() { bail!("qemu-img convert falhou"); }
        std::fs::rename(&part, &img)?;
        Ok(())
    })();
    let _ = remove(&m);
    r.map(|_| img)
}

pub fn create(o: CreateOpts) -> Result<Machine> {
    valid_name(&o.name)?;
    if Machine::load(&o.name).is_ok() { bail!("já existe uma máquina “{}”", o.name); }
    if !template::known(&o.template) { bail!("template desconhecido “{}” (veja `hyzevm templates`)", o.template); }
    qemu::qemu_img()?; // falha cedo se o QEMU não estiver instalado
    let backing = match &o.snapshot {
        Some(s) => {
            let p = snapshots_dir().join(format!("{s}.qcow2"));
            if !p.exists() { bail!("snapshot “{s}” não existe"); }
            p
        }
        // minimal = imagem oficial pura: nada a construir.
        None if o.template == "minimal" => template::base_image()?,
        None => ensure_template_image(&o.template)?,
    };
    let m = Machine {
        name: o.name, template: o.template, cpus: o.cpus, memory_mb: o.memory_mb, disk_gb: o.disk_gb,
        ssh_port: free_port()?, qmp_port: free_port()?, ports: o.ports, user: "agent".into(),
        view_port: free_port()?, view_ro_port: free_port()?,
        auto_suspend_secs: if o.always_on { 0 } else { o.auto_suspend.unwrap_or(1800) },
        expires_at: o.timeout.filter(|t| *t > 0).map(|t| now() + t), on_timeout: o.on_timeout,
        meta: o.meta, volumes: o.volumes, vault: o.vault, always_on: o.always_on, record_keep_secs: 0, auto_checkpoint: false, guard: vec![], dry_run: false, net_allow: vec![], net_limit_kbps: 0, net_proxy_port: 0,
        timezone: host_timezone(), lang: host_lang(),
    };
    m.save()?;
    if let Err(e) = provision(&m, &backing, false, o.wait) {
        let _ = qemu::stop(&m, true);
        let _ = m.remove();
        return Err(e);
    }
    Ok(m)
}

pub fn remove(m: &Machine) -> Result<()> {
    let _ = qemu::stop(m, true);
    crate::share::revoke_machine(&m.name);
    if m.dir().exists() { m.remove()?; }
    Ok(())
}

/// Snapshot independente (disco achatado) — a máquina precisa estar parada.
pub fn snapshot_save(m: &Machine, name: &str) -> Result<()> {
    crate::activity::log(m, "snapshot", name);
    valid_name(name)?;
    if qemu::state(m) != "stopped" { bail!("pare a máquina antes (`hyzevm stop {}`)", m.name); }
    std::fs::create_dir_all(snapshots_dir())?;
    let dst = snapshots_dir().join(format!("{name}.qcow2"));
    if dst.exists() { bail!("snapshot “{name}” já existe"); }
    let st = Command::new(qemu::qemu_img()?).args(["convert", "-O", "qcow2"]).arg(m.disk()).arg(&dst).status()?;
    if !st.success() { bail!("qemu-img convert falhou"); }
    Ok(())
}

/// Clona uma máquina (disco, navegador logado, arquivos): vira uma "máquina modelo". Desliga a origem um instante.
pub fn clone_machine(src: &Machine, dst: &str) -> Result<Machine> {
    valid_name(dst)?;
    let was_up = qemu::is_running(src);
    if was_up { qemu::stop(src, false).or_else(|_| qemu::stop(src, true))?; }
    let tmp = format!("clone-{}-{}", src.name, now());
    let r = snapshot_save(src, &tmp).and_then(|_| {
        let mut o = CreateOpts::simple(dst, &src.template);
        o.snapshot = Some(tmp.clone()); o.cpus = src.cpus; o.memory_mb = src.memory_mb; o.disk_gb = src.disk_gb;
        o.always_on = false; o.vault = src.vault; o.wait = true;
        create(o)
    });
    let _ = snapshot_remove(&tmp);
    if was_up { let _ = start(src); }
    r
}

/// Muda CPUs/RAM (a máquina precisa estar desligada; vale no próximo boot).
pub fn resize(m: &mut Machine, cpus: Option<u32>, memory_mb: Option<u32>) -> Result<()> {
    if qemu::is_running(m) { bail!("desligue a máquina antes (`hyzevm stop {}`)", m.name); }
    if let Some(c) = cpus { if !(1..=64).contains(&c) { bail!("cpus entre 1 e 64"); } m.cpus = c; }
    if let Some(r) = memory_mb { if r < 512 { bail!("mínimo 512 MB"); } m.memory_mb = r; }
    m.save()
}

pub fn snapshot_export(name: &str, to: &Path) -> Result<()> {
    valid_name(name)?;
    let src = snapshots_dir().join(format!("{name}.qcow2"));
    if !src.exists() { bail!("snapshot “{name}” não existe"); }
    std::fs::copy(&src, to).map(|_| ()).with_context(|| format!("não consegui gravar {}", to.display()))
}

pub fn snapshot_import(from: &Path, name: &str) -> Result<()> {
    valid_name(name)?;
    std::fs::create_dir_all(snapshots_dir())?;
    let dst = snapshots_dir().join(format!("{name}.qcow2"));
    if dst.exists() { bail!("snapshot “{name}” já existe"); }
    let o = Command::new(qemu::qemu_img()?).args(["info", "--output=json"]).arg(from).output()?;
    if !o.status.success() { bail!("{} não é uma imagem de disco válida", from.display()); }
    std::fs::copy(from, &dst)?;
    Ok(())
}

pub fn snapshot_list() -> Vec<String> { list_dir(&snapshots_dir()) }

pub fn snapshot_remove(name: &str) -> Result<()> {
    valid_name(name)?;
    Ok(std::fs::remove_file(snapshots_dir().join(format!("{name}.qcow2")))?)
}

fn list_dir(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d).into_iter().flatten().flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "qcow2"))
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string())).collect();
    v.sort();
    v
}

// --- volumes persistentes (/data/<nome> no guest) ---
pub fn volume_create(name: &str, gb: u32) -> Result<()> {
    valid_name(name)?;
    std::fs::create_dir_all(volumes_dir())?;
    let p = volumes_dir().join(format!("{name}.qcow2"));
    if p.exists() { bail!("volume “{name}” já existe"); }
    let st = Command::new(qemu::qemu_img()?).args(["create", "-f", "qcow2"]).arg(&p).arg(format!("{gb}G"))
        .stdout(std::process::Stdio::null()).status()?;
    if !st.success() { bail!("qemu-img create falhou"); }
    Ok(())
}
pub fn volume_list() -> Vec<String> { list_dir(&volumes_dir()) }
pub fn volume_remove(name: &str) -> Result<()> {
    valid_name(name)?;
    if let Some(m) = list()?.iter().find(|m| m.volumes.iter().any(|v| v == name)) {
        bail!("volume em uso por {}", m.name);
    }
    Ok(std::fs::remove_file(volumes_dir().join(format!("{name}.qcow2")))?)
}

// --- portas públicas (URL local; túnel/HTTPS fica para depois) ---
pub fn port_publish(m: &mut Machine, guest: u16, host: Option<u16>) -> Result<u16> {
    if let Some((h, _)) = m.ports.iter().find(|(_, g)| *g == guest) { return Ok(*h); }
    let host = match host { Some(h) => h, None => free_port()? };
    if qemu::is_running(m) { qemu::hostfwd(m, true, host, guest)?; }
    m.ports.push((host, guest));
    m.save()?;
    Ok(host)
}
pub fn port_unpublish(m: &mut Machine, guest: u16) -> Result<()> {
    let Some(i) = m.ports.iter().position(|(_, g)| *g == guest) else { bail!("porta {guest} não está publicada") };
    let (h, _) = m.ports.remove(i);
    if qemu::is_running(m) { qemu::hostfwd(m, false, h, guest)?; }
    m.save()
}
