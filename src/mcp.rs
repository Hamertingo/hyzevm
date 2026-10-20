//! Servidor MCP (JSON-RPC por linha, stdio): as mesmas operações do CLI.
use crate::{browser, desktop, takeover, ops, qemu, share, ssh, store, vault};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn tools() -> Value {
    let t = |name: &str, desc: &str, props: Value, req: &[&str]| json!({
        "name": name, "description": desc,
        "inputSchema": {"type": "object", "properties": props, "required": req}});
    let n = json!({"name": {"type": "string", "description": "nome da máquina"}});
    let xy = json!({"name": {"type":"string"}, "x": {"type":"integer"}, "y": {"type":"integer"}});
    json!([
        t("machine_create", "Cria e liga uma máquina Linux própria. template: minimal (padrão, instantâneo) | base (docker/node/python) | desktop (desktop virtual + Chromium, para computer use)",
            json!({"name": {"type":"string"}, "template": {"type":"string"}, "snapshot": {"type":"string"}, "always_on": {"type":"boolean", "description": "máquina 24/7: sobe com o host, reinicia se cair, nunca pausa"}}), &["name"]),
        t("machine_list", "Lista máquinas e estados", json!({}), &[]),
        t("machine_exec", "Roda um comando (bash) na máquina; retoma se pausada. Retorna exit code, stdout e stderr",
            json!({"name": {"type":"string"}, "command": {"type":"string"}}), &["name", "command"]),
        t("machine_read_file", "Lê um arquivo de texto da máquina", json!({"name": {"type":"string"}, "path": {"type":"string"}}), &["name", "path"]),
        t("machine_write_file", "Escreve um arquivo de texto na máquina",
            json!({"name": {"type":"string"}, "path": {"type":"string"}, "content": {"type":"string"}}), &["name", "path", "content"]),
        t("machine_start", "Liga/retoma a máquina", n.clone(), &["name"]),
        t("machine_stop", "Desliga a máquina", n.clone(), &["name"]),
        t("machine_suspend", "Pausa a máquina na hora (CPU zerada, memória e processos mantidos); retoma sozinha no próximo uso", n.clone(), &["name"]),
        t("machine_delete", "Apaga a máquina", n.clone(), &["name"]),
        t("snapshot_save", "Salva a máquina (parada) como snapshot", json!({"name": {"type":"string"}, "snapshot": {"type":"string"}}), &["name", "snapshot"]),
        t("snapshot_list", "Lista snapshots", json!({}), &[]),
        t("port_publish", "Publica uma porta do guest num URL local", json!({"name": {"type":"string"}, "port": {"type":"integer"}}), &["name", "port"]),
        t("vault_list", "Lista os nomes dos segredos do vault (valores nunca são expostos)", json!({}), &[]),
        t("desktop_screenshot", "Captura a tela do desktop da máquina (PNG)", n.clone(), &["name"]),
        t("desktop_click", "Clica em (x,y). button: left|right|middle; double: true",
            json!({"name": {"type":"string"}, "x": {"type":"integer"}, "y": {"type":"integer"}, "button": {"type":"string"}, "double": {"type":"boolean"}}), &["name", "x", "y"]),
        t("desktop_move", "Move o mouse para (x,y)", xy, &["name", "x", "y"]),
        t("desktop_type", "Digita texto no desktop", json!({"name": {"type":"string"}, "text": {"type":"string"}}), &["name", "text"]),
        t("desktop_press", "Pressiona tecla/combinação, ex.: Enter, ctrl+l, alt+Tab", json!({"name": {"type":"string"}, "keys": {"type":"string"}}), &["name", "keys"]),
        t("desktop_scroll", "Rola: direction up|down|left|right",
            json!({"name": {"type":"string"}, "direction": {"type":"string"}, "amount": {"type":"integer"}}), &["name", "direction"]),
        t("desktop_open", "Abre uma URL no Chromium do desktop", json!({"name": {"type":"string"}, "url": {"type":"string"}}), &["name", "url"]),
        t("desktop_share", "Gera um link local da tela com token e expiração. Por padrão somente-leitura (imposto no servidor); control=true permite mouse/teclado",
            json!({"name": {"type":"string"}, "ttl_seconds": {"type":"integer"}, "control": {"type":"boolean"}}), &["name"]),
        t("desktop_share_revoke", "Revoga todos os links de tela da máquina (derruba viewers abertos)", n.clone(), &["name"]),
        t("browser_goto", "Abre uma URL no Chromium do desktop (perfil persistente: logins ficam)", json!({"name": {"type":"string"}, "url": {"type":"string"}}), &["name", "url"]),
        t("browser_snapshot", "Lista os elementos interativos da página com referências (e1, e2…) + texto visível. PREFIRA isto a screenshot para navegar e clicar", n.clone(), &["name"]),
        t("browser_click", "Clica no elemento pela referência do último snapshot", json!({"name": {"type":"string"}, "ref": {"type":"string"}}), &["name", "ref"]),
        t("browser_type", "Digita texto no elemento (substitui o conteúdo). submit=true aperta Enter depois",
            json!({"name": {"type":"string"}, "ref": {"type":"string"}, "text": {"type":"string"}, "submit": {"type":"boolean"}}), &["name", "ref", "text"]),
        t("browser_press", "Tecla ou combinação: Enter, Tab, Escape, ctrl+a…", json!({"name": {"type":"string"}, "keys": {"type":"string"}}), &["name", "keys"]),
        t("browser_scroll", "Rola a página: up|down|left|right", json!({"name": {"type":"string"}, "direction": {"type":"string"}, "amount": {"type":"integer"}}), &["name", "direction"]),
        t("browser_upload", "Anexa arquivos (caminhos DENTRO da máquina; envie antes com machine_write_file/cp) a um input de arquivo — use para vídeos",
            json!({"name": {"type":"string"}, "ref": {"type":"string"}, "files": {"type":"array", "items": {"type":"string"}}}), &["name", "ref", "files"]),
        t("browser_wait", "Espera N milissegundos (número) ou um texto aparecer na página", json!({"name": {"type":"string"}, "what": {"type":"string"}}), &["name", "what"]),
        t("request_takeover", "Chame quando travar em CAPTCHA, 2FA, login ou tela inesperada: notifica o usuário e devolve um link de controle. Depois use takeover_wait. NÃO tente contornar CAPTCHA/2FA",
            json!({"name": {"type":"string"}, "reason": {"type":"string"}}), &["name", "reason"]),
        t("takeover_wait", "Espera o usuário devolver o controle (até timeout_seconds, máx 600). Retorna 'devolvido' ou 'esperando' (chame de novo)",
            json!({"name": {"type":"string"}, "timeout_seconds": {"type":"integer"}}), &["name"]),
        t("desktop_view_url", "URL para o usuário assistir/controlar a tela no navegador", n, &["name"]),
    ])
}

fn s<'a>(a: &'a Value, k: &str) -> Result<&'a str> { a[k].as_str().ok_or_else(|| anyhow!("argumento “{k}” ausente")) }
fn i(a: &Value, k: &str) -> Result<i32> { a[k].as_i64().map(|v| v as i32).ok_or_else(|| anyhow!("argumento “{k}” ausente")) }

/// Máquina ligada (auto-resume) e com atividade marcada.
fn live(a: &Value) -> Result<store::Machine> {
    let m = store::Machine::load(s(a, "name")?)?;
    let was_up = qemu::state(&m) == "running";
    ops::start(&m)?;
    if !was_up { ssh::wait_ssh(&m, 120)?; }
    Ok(m)
}

fn text(t: impl Into<String>) -> Value { json!({"content": [{"type": "text", "text": t.into()}]}) }

fn call(tool: &str, a: &Value) -> Result<Value> {
    Ok(text(match tool {
        "machine_create" => {
            let mut o = ops::CreateOpts::simple(s(a, "name")?, a["template"].as_str().unwrap_or("minimal"));
            o.snapshot = a["snapshot"].as_str().map(Into::into);
            o.always_on = a["always_on"].as_bool().unwrap_or(false);
            format!("{} pronta", ops::create(o)?.name)
        }
        "machine_list" => store::list()?.iter().map(|m| format!("{}\t{}\t{}", m.name, qemu::state(m), m.template)).collect::<Vec<_>>().join("\n"),
        "machine_exec" => {
            let o = ssh::exec_capture(&live(a)?, s(a, "command")?)?;
            format!("exit={}\n--- stdout\n{}\n--- stderr\n{}", o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
        }
        "machine_read_file" => {
            let o = ssh::exec_capture(&live(a)?, &format!("cat {}", ssh::sh_quote(s(a, "path")?)))?;
            if !o.status.success() { return Err(anyhow!("{}", String::from_utf8_lossy(&o.stderr))); }
            String::from_utf8_lossy(&o.stdout).to_string()
        }
        "machine_write_file" => {
            let p = ssh::sh_quote(s(a, "path")?);
            let sc = format!("mkdir -p \"$(dirname {p})\" && echo {} | base64 -d > {p}\n", ssh::sh_quote(&B64.encode(s(a, "content")?)));
            let o = ssh::exec_stdin(&live(a)?, &sc)?;
            if !o.status.success() { return Err(anyhow!("{}", String::from_utf8_lossy(&o.stderr))); }
            "escrito".into()
        }
        "machine_start" => { live(a)?; "running".into() }
        "machine_stop" => { qemu::stop(&store::Machine::load(s(a, "name")?)?, false)?; "stopped".into() }
        "machine_suspend" => { qemu::pause(&store::Machine::load(s(a, "name")?)?)?; "paused".into() }
        "machine_delete" => { ops::remove(&store::Machine::load(s(a, "name")?)?)?; "removida".into() }
        "snapshot_save" => { ops::snapshot_save(&store::Machine::load(s(a, "name")?)?, s(a, "snapshot")?)?; "salvo".into() }
        "snapshot_list" => ops::snapshot_list().join("\n"),
        "port_publish" => {
            let mut m = store::Machine::load(s(a, "name")?)?;
            format!("http://127.0.0.1:{}", ops::port_publish(&mut m, i(a, "port")? as u16, None)?)
        }
        "vault_list" => { let v = vault::Vault::load(); v.env.keys().cloned().chain(v.files.keys().map(|k| format!("~/{k}"))).collect::<Vec<_>>().join("\n") }
        "desktop_screenshot" => {
            let png = desktop::screenshot(&live(a)?)?;
            return Ok(json!({"content": [{"type": "image", "data": B64.encode(png), "mimeType": "image/png"}]}));
        }
        "desktop_click" => { desktop::click(&live(a)?, i(a, "x")?, i(a, "y")?, a["button"].as_str().unwrap_or("left"), a["double"].as_bool().unwrap_or(false))?; "ok".into() }
        "desktop_move" => { desktop::mv(&live(a)?, i(a, "x")?, i(a, "y")?)?; "ok".into() }
        "desktop_type" => { desktop::type_text(&live(a)?, s(a, "text")?)?; "ok".into() }
        "desktop_press" => { desktop::press(&live(a)?, s(a, "keys")?)?; "ok".into() }
        "desktop_scroll" => { desktop::scroll(&live(a)?, s(a, "direction")?, a["amount"].as_u64().unwrap_or(5) as u32)?; "ok".into() }
        "desktop_open" => { desktop::open(&live(a)?, s(a, "url")?)?; "ok".into() }
        "desktop_share" => {
            let (_, url, _) = share::create(&live(a)?, !a["control"].as_bool().unwrap_or(false), a["ttl_seconds"].as_u64().unwrap_or(3600))?;
            url
        }
        "desktop_share_revoke" => { share::revoke_machine(s(a, "name")?); "revogado".into() }
        "browser_goto" => browser::run(&live(a)?, &["goto", s(a, "url")?])?,
        "browser_snapshot" => browser::run(&live(a)?, &["snapshot"])?,
        "browser_click" => browser::run(&live(a)?, &["click", s(a, "ref")?])?,
        "browser_type" => {
            let mut v = vec!["type", s(a, "ref")?, s(a, "text")?];
            if a["submit"].as_bool().unwrap_or(false) { v.push("--submit"); }
            browser::run(&live(a)?, &v)?
        }
        "browser_press" => browser::run(&live(a)?, &["press", s(a, "keys")?])?,
        "browser_scroll" => { let amt = a["amount"].as_u64().unwrap_or(5).to_string(); browser::run(&live(a)?, &["scroll", s(a, "direction")?, &amt])? }
        "browser_upload" => {
            let files: Vec<&str> = a["files"].as_array().map(|v| v.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
            browser::run(&live(a)?, &[&["upload", s(a, "ref")?][..], &files].concat())?
        }
        "browser_wait" => browser::run(&live(a)?, &["wait", s(a, "what")?])?,
        "request_takeover" => {
            let t = takeover::request(&live(a)?, s(a, "reason")?)?;
            format!("O usuário foi notificado. Link para ele assumir: {}\nAgora chame takeover_wait e NÃO continue a tarefa até devolverem o controle.", t.url)
        }
        "takeover_wait" => {
            let m = store::Machine::load(s(a, "name")?)?;
            if takeover::wait(&m, a["timeout_seconds"].as_u64().unwrap_or(120).min(600))? { "devolvido: pode continuar (rode browser_snapshot para ver o estado atual)".into() } else { "esperando".into() }
        }
        "desktop_view_url" => desktop::view_url(&live(a)?)?,
        t => return Err(anyhow!("tool desconhecida: {t}")),
    }))
}

pub fn run() -> Result<i32> {
    let out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(req) = serde_json::from_str::<Value>(&line?) else { continue };
        let Some(id) = req.get("id").cloned() else { continue }; // notificações não respondem
        let result = match req["method"].as_str().unwrap_or("") {
            "initialize" => json!({"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                "serverInfo": {"name": "hyzevm", "version": env!("CARGO_PKG_VERSION")}}),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => match call(req["params"]["name"].as_str().unwrap_or(""), &req["params"]["arguments"]) {
                Ok(v) => v,
                Err(e) => json!({"isError": true, "content": [{"type": "text", "text": format!("{e:#}")}]}),
            },
            "ping" => json!({}),
            _ => { writeln!(out.lock(), "{}", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"método desconhecido"}}))?; continue }
        };
        writeln!(out.lock(), "{}", json!({"jsonrpc": "2.0", "id": id, "result": result}))?;
        out.lock().flush()?;
    }
    Ok(0)
}
