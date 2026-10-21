//! Motor: QEMU com aceleração nativa (HVF no macOS, KVM no Linux, WHPX no Windows).
use crate::store::Machine;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SUSPEND_TAG: &str = "hyzevm-suspend";

fn arch() -> &'static str { if std::env::consts::ARCH == "aarch64" { "aarch64" } else { "x86_64" } }

pub fn accel() -> &'static str {
    match std::env::consts::OS { "macos" => "hvf", "linux" => "kvm", "windows" => "whpx", _ => "tcg" }
}

/// Procura um binário no PATH e nos locais de instalação comuns.
fn find_bin(name: &str) -> Result<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "C:\\Program Files\\qemu"] { dirs.push(d.into()); }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file()).with_context(|| {
        format!("{name} não encontrado — instale o QEMU (macOS: brew install qemu · Linux: apt install qemu-system qemu-utils · Windows: winget install SoftwareFreedomConservancy.QEMU)")
    })
}

pub fn qemu_img() -> Result<PathBuf> { find_bin("qemu-img") }
pub fn qemu_system() -> Result<PathBuf> { find_bin(&format!("qemu-system-{}", arch())) }
pub fn uefi_check() -> Result<String> {
    match uefi_firmware()? { Some(p) => Ok(p.display().to_string()), None => Ok("não necessário (x86_64 usa SeaBIOS)".into()) }
}

/// Firmware UEFI — só o arm64 precisa (x86_64 usa SeaBIOS embutido).
fn uefi_firmware() -> Result<Option<PathBuf>> {
    if arch() != "aarch64" { return Ok(None); }
    let c = ["/opt/homebrew/share/qemu/edk2-aarch64-code.fd", "/usr/local/share/qemu/edk2-aarch64-code.fd",
        "/usr/share/qemu/edk2-aarch64-code.fd", "/usr/share/AAVMF/AAVMF_CODE.fd",
        "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd", "C:\\Program Files\\qemu\\share\\edk2-aarch64-code.fd"];
    c.iter().map(PathBuf::from).find(|p| p.is_file()).map(Some)
        .context("firmware UEFI arm64 não encontrado (instale qemu-efi-aarch64 / o pacote completo do QEMU)")
}

/// Copia `src` para `dst` como arquivo independente. Em APFS/btrfs/XFS é um clone instantâneo (copy-on-write,
/// sem gastar espaço até divergir); em outros sistemas de arquivos é uma cópia comum.
fn clone_file(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    let _ = std::fs::remove_file(dst);
    let fast = if cfg!(target_os = "macos") { Command::new("cp").arg("-c").arg(src).arg(dst).stderr(Stdio::null()).status() }
        else if cfg!(target_os = "linux") { Command::new("cp").args(["--reflink=auto"]).arg(src).arg(dst).stderr(Stdio::null()).status() }
        else { Err(std::io::Error::other("sem clone")) };
    if fast.map(|s| s.success()).unwrap_or(false) { return Ok(()); }
    std::fs::copy(src, dst).with_context(|| format!("não consegui copiar {}", src.display()))?;
    Ok(())
}

fn virtual_size(p: &std::path::Path) -> u64 {
    qemu_img().ok().and_then(|q| Command::new(q).args(["info", "--output=json", "-U"]).arg(p).output().ok())
        .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
        .and_then(|v| v["virtual-size"].as_u64()).unwrap_or(0)
}

/// Disco da máquina: uma cópia **independente** da imagem (não uma camada em cima dela). Assim apagar,
/// refazer ou atualizar a imagem-base nunca estraga uma máquina que já existe. Nunca menor que a imagem
/// (um disco menor corta a partição raiz e o guest não boota).
pub fn create_disk(m: &Machine, backing: &std::path::Path) -> Result<()> {
    let virt = virtual_size(backing);
    let want = (m.disk_gb as u64) << 30;
    let gb = if virt > want {
        let g = virt.div_ceil(1 << 30);
        crate::ui::warn(&format!("disco de {} GB é menor que a imagem ({g} GB) — usando {g} GB", m.disk_gb));
        g
    } else { m.disk_gb as u64 };
    clone_file(backing, &m.disk())?;
    if gb << 30 > virt {
        let st = Command::new(qemu_img()?).args(["resize", "-q"]).arg(m.disk()).arg(format!("{gb}G")).status()?;
        if !st.success() { bail!("qemu-img resize falhou"); }
    }
    Ok(())
}

/// Máquinas criadas por versões antigas têm o disco "em camadas" sobre uma imagem-base. Antes de ligar
/// (QEMU parado), copia os dados da base para dentro do disco: daí em diante ele não depende de mais nada.
pub fn ensure_standalone(m: &Machine) -> Result<()> {
    let q = qemu_img()?;
    let o = Command::new(&q).args(["info", "--output=json"]).arg(m.disk()).output()?;
    let v: Value = serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
    let Some(backing) = v["full-backing-filename"].as_str().or(v["backing-filename"].as_str()) else { return Ok(()) };
    if !std::path::Path::new(backing).exists() {
        bail!("o disco de {} depende de {backing}, que não existe mais — recrie a máquina (ou restaure esse arquivo)", m.name);
    }
    crate::ui::step(&format!("{}: tornando o disco independente da imagem-base (uma vez só)…", m.name));
    let st = Command::new(&q).args(["rebase", "-q", "-f", "qcow2", "-b", ""]).arg(m.disk()).status()?;
    if !st.success() { bail!("qemu-img rebase falhou"); }
    Ok(())
}

/// A imagem base é qcow2 disfarçada de .img — o backing já é qcow2.
pub fn is_running(m: &Machine) -> bool {
    TcpStream::connect_timeout(&format!("127.0.0.1:{}", m.qmp_port).parse().unwrap(), Duration::from_millis(300)).is_ok()
}

/// Como a máquina ficou de pé: já estava, boot novo, retomada da pausa ao vivo ou do estado em disco.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Started { Already, Fresh, Live, Disk }

fn live_marker(m: &Machine) -> std::path::PathBuf { m.dir().join("paused-live") }

pub fn start(m: &Machine) -> Result<Started> {
    m.set_intent(true);
    if is_running(m) {
        if live_marker(m).exists() {
            qmp(m, json!({"execute": "cont"}))?;
            let _ = std::fs::remove_file(live_marker(m));
            return Ok(Started::Live);
        }
        return Ok(Started::Already);
    }
    crate::net::ensure_proxy(m)?;
    let _ = std::fs::remove_file(live_marker(m)); // sobra de um QEMU que morreu pausado
    if !m.dir().join("suspended").exists() { ensure_standalone(m)?; } // estado salvo em disco: só depois de um boot limpo
    let suspended = m.dir().join("suspended").exists();
    let mut a: Vec<String> = vec![];
    let mut push = |s: &[&str]| a.extend(s.iter().map(|x| x.to_string()));
    let host_cpu = matches!(accel(), "hvf" | "kvm");
    if arch() == "aarch64" {
        push(&["-machine", &format!("virt,accel={},highmem=on", accel()), "-cpu", if host_cpu { "host" } else { "max" }]);
        if let Some(fw) = uefi_firmware()? { push(&["-bios", &fw.to_string_lossy()]); }
    } else {
        push(&["-machine", &format!("q35,accel={}", accel()), "-cpu", if host_cpu { "host" } else { "max" }]);
    }
    let mut fwd = format!("user,id=net0,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:{}-:22", m.ssh_port);
    // Firewall de saída: sem rota para a internet; o único caminho é o proxy do host (10.0.2.100:3128) com lista de sites.
    if !m.net_allow.is_empty() && m.net_proxy_port != 0 { fwd += &format!(",restrict=on,guestfwd=tcp:10.0.2.100:3128-tcp:127.0.0.1:{}", m.net_proxy_port); }
    if m.view_port != 0 { fwd += &format!(",hostfwd=tcp:127.0.0.1:{}-:6080", m.view_port); }
    if m.view_ro_port != 0 { fwd += &format!(",hostfwd=tcp:127.0.0.1:{}-:6081", m.view_ro_port); }
    for (h, g) in &m.ports { fwd += &format!(",hostfwd=tcp:127.0.0.1:{h}-:{g}"); }
    push(&["-m", &m.memory_mb.to_string(), "-smp", &m.cpus.to_string()]);
    push(&["-drive", &format!("file={},if=virtio,format=qcow2,cache=writeback,discard=unmap,detect-zeroes=unmap", m.disk().display())]);
    for (i, v) in m.volumes.iter().enumerate() {
        let p = crate::store::volumes_dir().join(format!("{v}.qcow2"));
        push(&["-drive", &format!("file={},if=none,id=vol{i},format=qcow2", p.display()),
            "-device", &format!("virtio-blk-pci,drive=vol{i},serial=hyzevol{i}")]);
    }
    if m.seed_dir().exists() {
        // NoCloud: pasta FAT virtual com label "cidata" (cross-platform, sem mkisofs).
        push(&["-blockdev", &format!("driver=vvfat,node-name=seed,dir={},label=cidata,rw=off,read-only=on", m.seed_dir().display()),
            "-device", "virtio-blk-pci,drive=seed"]);
    }
    push(&["-nic", &fwd, "-device", "virtio-rng-pci", "-device", "virtio-balloon-pci,free-page-reporting=on", "-display", "none",
        "-serial", &format!("file:{}", m.serial_log().display()),
        "-qmp", &format!("tcp:127.0.0.1:{},server,nowait", m.qmp_port)]);
    if suspended { push(&["-loadvm", SUSPEND_TAG]); }

    let mut cmd = Command::new(find_bin(&format!("qemu-system-{}", arch()))?);
    // stderr vai para qemu.log na pasta da máquina (diagnóstico de falha de boot).
    let errlog = std::fs::File::create(m.dir().join("qemu.log"))?;
    cmd.args(&a).stdin(Stdio::null()).stdout(Stdio::null()).stderr(errlog);
    #[cfg(unix)] { use std::os::unix::process::CommandExt; cmd.process_group(0); }
    #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x00000008 | 0x00000200); }
    cmd.spawn().context("falha ao iniciar o QEMU")?;

    let t = Instant::now();
    while !is_running(m) {
        if t.elapsed() > Duration::from_secs(20) {
            let log = std::fs::read_to_string(m.dir().join("qemu.log")).unwrap_or_default();
            bail!("QEMU não subiu em 20s:\n{}{}", log, accel_hint(&log));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if suspended {
        let _ = hmp(m, &format!("delvm {SUSPEND_TAG}"));
        let _ = std::fs::remove_file(m.dir().join("suspended"));
    }
    Ok(if suspended { Started::Disk } else { Started::Fresh })
}

pub fn qmp(m: &Machine, cmd: Value) -> Result<Value> {
    // O QMP atende um cliente por vez: logo após outra conexão (ex.: is_running)
    // o QEMU pode resetar a nova — tenta de novo.
    let mut last = None;
    for _ in 0..20 {
        let attempt = (|| -> Result<(TcpStream, BufReader<TcpStream>, String)> {
            let s = TcpStream::connect(format!("127.0.0.1:{}", m.qmp_port)).context("máquina parada")?;
            s.set_read_timeout(Some(Duration::from_secs(300)))?;
            let mut r = BufReader::new(s.try_clone()?);
            let mut line = String::new();
            if r.read_line(&mut line)? == 0 { bail!("QMP fechou a conexão"); } // greeting
            Ok((s, r, line))
        })();
        match attempt { Ok(x) => { last = Some(Ok(x)); break }, Err(e) => { last = Some(Err(e)); std::thread::sleep(Duration::from_millis(150)) } }
    }
    let (mut s, mut r, mut line) = last.unwrap()?;
    writeln!(s, "{}", json!({"execute": "qmp_capabilities"}))?;
    line.clear(); r.read_line(&mut line)?;
    writeln!(s, "{cmd}")?;
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 { return Ok(Value::Null); } // quit fecha a conexão
        let v: Value = serde_json::from_str(&line)?;
        if v.get("return").is_some() { return Ok(v["return"].clone()); }
        if let Some(e) = v.get("error") { bail!("{}", e["desc"].as_str().unwrap_or("erro QMP")); }
    }
}

fn hmp(m: &Machine, c: &str) -> Result<String> {
    let v = qmp(m, json!({"execute": "human-monitor-command", "arguments": {"command-line": c}}))?;
    Ok(v.as_str().unwrap_or("").to_string())
}

fn wait_down(m: &Machine, secs: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        if !is_running(m) { return true; }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Desligamento limpo via ACPI; `force` mata o QEMU.
pub fn stop(m: &Machine, force: bool) -> Result<()> {
    crate::activity::log(m, "power", "parou");
    m.set_intent(false);
    if !is_running(m) { let _ = std::fs::remove_file(live_marker(m)); return Ok(()); }
    if live_marker(m).exists() { // pausada não atende ACPI: solta antes
        let _ = qmp(m, json!({"execute": "cont"}));
        let _ = std::fs::remove_file(live_marker(m));
    }
    if !force {
        qmp(m, json!({"execute": "system_powerdown"}))?;
        if wait_down(m, 45) { return Ok(()); }
        bail!("guest não desligou em 45s — use `stop --force`");
    }
    let _ = qmp(m, json!({"execute": "quit"}));
    if wait_down(m, 10) { Ok(()) } else { bail!("QEMU não encerrou") }
}

/// Pausa ao vivo: congela as CPUs da VM (0% de CPU no host) sem fechar o QEMU.
/// Retoma na hora e sem salto de relógio — é a pausa padrão (app, MCP, ociosidade).
pub fn pause(m: &Machine) -> Result<()> {
    crate::activity::log(m, "power", "pausou");
    if !is_running(m) { bail!("{} não está em execução", m.name); }
    qmp(m, json!({"execute": "stop"}))?;
    std::fs::write(live_marker(m), b"1")?;
    Ok(())
}

/// Suspender para disco: grava RAM+CPU no qcow2 e encerra o QEMU (libera a memória do host).
/// Atenção: após muitas horas, a retomada pode congelar o guest (relógio do HVF); `ops::start`
/// detecta e reinicia a frio.
pub fn suspend(m: &Machine) -> Result<()> {
    m.set_intent(false);
    if !is_running(m) { bail!("{} não está em execução", m.name); }
    let out = hmp(m, &format!("savevm {SUSPEND_TAG}"))?;
    if !out.trim().is_empty() { bail!("savevm falhou: {}", out.trim()); }
    std::fs::write(m.dir().join("suspended"), b"1")?;
    let _ = std::fs::remove_file(live_marker(m));
    let _ = qmp(m, json!({"execute": "quit"}));
    if !wait_down(m, 15) { bail!("QEMU não encerrou após o savevm"); }
    Ok(())
}

/// Reinício a frio (como apertar reset): usado quando um estado salvo não volta a responder.
pub fn system_reset(m: &Machine) -> Result<()> {
    qmp(m, json!({"execute": "system_reset"}))?;
    Ok(())
}

pub fn state(m: &Machine) -> &'static str {
    if is_running(m) { if live_marker(m).exists() { "paused" } else { "running" } }
    else if m.dir().join("suspended").exists() { "paused" } else { "stopped" }
}

/// Publica/remove porta a quente (aplica já, sem reiniciar).
pub fn hostfwd(m: &Machine, add: bool, host: u16, guest: u16) -> Result<()> {
    let c = if add { format!("hostfwd_add net0 tcp:127.0.0.1:{host}-:{guest}") }
            else { format!("hostfwd_remove net0 tcp:127.0.0.1:{host}") };
    let out = hmp(m, &c)?;
    if !out.trim().is_empty() { bail!("{}", out.trim()); }
    Ok(())
}

/// Explica em português os erros de aceleração mais comuns (o texto do QEMU engana: "No space left on device"
/// no WHPX quer dizer que a Plataforma do Hipervisor não está ativa, não que falta disco).
fn accel_hint(log: &str) -> String {
    let l = log.to_lowercase();
    if l.contains("whpx") {
        return "\n→ A aceleração do Windows (WHPX) não está ativa. Ative “Plataforma do Hipervisor do Windows” em \
            Painel de Controle > Programas > Ativar ou desativar recursos do Windows (ou, no PowerShell como administrador: \
            Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All) e REINICIE o computador. \
            O texto “No space left on device” acima é enganoso: não é falta de disco.\n".into();
    }
    if l.contains("kvm") { return "\n→ KVM indisponível: confira /dev/kvm e se seu usuário está no grupo kvm (sudo usermod -aG kvm $USER; reinicie a sessão).\n".into(); }
    if l.contains("hvf") { return "\n→ HVF indisponível: o macOS precisa permitir o Hypervisor (Apple Silicon ou Intel com VT-x).\n".into(); }
    String::new()
}
