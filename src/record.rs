//! Gravação da tela do guest (ffmpeg x11grab) em trechos de 5 min, com retenção.
//! Os trechos são mp4 fragmentados (seguros contra queda) em ~/.hyze/rec/<epoch>.mp4 no guest;
//! o nome é o instante de início, o que liga cada ação do registro ao ponto certo do vídeo.
use crate::{activity, ssh, store::*};
use anyhow::{bail, Result};

const DIR: &str = "$HOME/.hyze/rec";
const SEG: u64 = 300;

#[derive(Clone, Debug)]
pub struct Segment { pub start: u64, pub bytes: u64 }

fn need_screen(m: &Machine) -> Result<()> {
    if m.template != "desktop" { bail!("{} não tem tela (template “{}”) — gravar só no template desktop", m.name, m.template); }
    Ok(())
}

pub fn is_recording(m: &Machine) -> bool {
    ssh::exec_capture(m, "pgrep -f '[x]11grab.*hyze/rec' >/dev/null").is_ok_and(|o| o.status.success())
}

pub fn start(m: &Machine) -> Result<bool> {
    need_screen(m)?;
    if is_recording(m) { return Ok(false); }
    let sc = format!("mkdir -p {DIR}\nexport DISPLAY=:99\n\
        setsid nohup ffmpeg -loglevel error -f x11grab -framerate 5 -video_size 1280x800 -i :99 \
        -c:v libx264 -preset ultrafast -crf 32 -pix_fmt yuv420p -g 25 \
        -f segment -segment_time {SEG} -reset_timestamps 1 -strftime 1 -segment_format mp4 \
        -segment_format_options movflags=+frag_keyframe+empty_moov+default_base_moof \
        \"{DIR}/%s.mp4\" >/dev/null 2>&1 </dev/null &\nsleep 1\n");
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("{}", String::from_utf8_lossy(&o.stderr).trim()); }
    activity::log(m, "record", "gravação iniciada");
    Ok(true)
}

pub fn stop(m: &Machine) -> Result<()> {
    ssh::exec_capture(m, "pkill -INT -f '[x]11grab.*hyze/rec'; true")?;
    activity::log(m, "record", "gravação parada");
    Ok(())
}

/// Trechos gravados, do mais antigo para o mais novo.
pub fn list(m: &Machine) -> Result<Vec<Segment>> {
    let o = ssh::exec_capture(m, &format!("cd {DIR} 2>/dev/null && stat -c '%n %s' *.mp4 2>/dev/null; true"))?;
    let mut v: Vec<Segment> = String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| {
        let (n, b) = l.split_once(' ')?;
        Some(Segment { start: n.strip_suffix(".mp4")?.parse().ok()?, bytes: b.parse().ok()? })
    }).collect();
    v.sort_by_key(|s| s.start);
    Ok(v)
}

/// Apaga trechos mais velhos que `keep_secs`; devolve quantos.
pub fn prune(m: &Machine, keep_secs: u64) -> Result<usize> {
    let cut = now().saturating_sub(keep_secs);
    let old: Vec<String> = list(m)?.into_iter().filter(|s| s.start + SEG < cut).map(|s| format!("{DIR}/{}.mp4", s.start)).collect();
    if !old.is_empty() { ssh::exec_capture(m, &format!("rm -f {}", old.join(" ")))?; }
    Ok(old.len())
}

/// Recorte curto (MP4 pequeno, sem recodificar) em volta de um instante, para tocar no app.
pub fn clip(m: &Machine, ts: u64, before: u64, after: u64) -> Result<Vec<u8>> {
    let segs = list(m)?;
    let Some(seg) = segs.iter().rev().find(|s| s.start <= ts) else { bail!("não há gravação desse momento") };
    let from = ts.saturating_sub(before).max(seg.start) - seg.start;
    let sc = format!("ffmpeg -loglevel error -ss {from} -i {DIR}/{}.mp4 -t {} -c copy -movflags +frag_keyframe+empty_moov+default_base_moof -f mp4 - ", seg.start, before + after);
    let o = ssh::exec_capture(m, &sc)?;
    if !o.status.success() || o.stdout.is_empty() { bail!("não consegui recortar o vídeo"); }
    Ok(o.stdout)
}

/// Usado pelo vigia: mantém a gravação automática ligada e apaga o que passou da retenção.
pub fn maintain(m: &Machine) {
    if m.record_keep_secs == 0 || m.template != "desktop" { return; }
    let _ = start(m);
    let _ = prune(m, m.record_keep_secs);
}
