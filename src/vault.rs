//! Vault: segredos cadastrados uma vez, escritos em toda máquina no boot.
//! Guardado cifrado em ~/.hyzevm/vault.enc (AES-256-GCM; a chave fica no Keychain/DPAPI/secret-tool).
//! Sem chaveiro disponível, cai para vault.json (0600) com aviso. Env → /etc/environment; arquivos → home.
use crate::{ssh, store::*};
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Default)]
pub struct Vault {
    pub env: BTreeMap<String, String>,
    /// caminho (relativo à home do guest) → conteúdo em base64
    pub files: BTreeMap<String, String>,
}

impl Vault {
    pub fn load() -> Vault {
        if let Ok(blob) = std::fs::read(vault_enc_file()) {
            if let Some(v) = crate::keychain::key(false).and_then(|k| crate::keychain::open(&k, &blob)).and_then(|b| serde_json::from_slice(&b).ok()) { return v; }
            crate::ui::warn("não consegui abrir o cofre cifrado (chave do chaveiro indisponível) — os segredos não serão escritos");
            return Vault::default();
        }
        // vault.json antigo (texto): ainda lê; o próximo `save` migra para o cifrado.
        std::fs::read(vault_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(home())?;
        let json = serde_json::to_vec_pretty(self)?;
        if let Some(k) = crate::keychain::key(true) {
            std::fs::write(vault_enc_file(), crate::keychain::seal(&k, &json)?)?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(vault_enc_file(), std::fs::Permissions::from_mode(0o600)); }
            if vault_file().exists() { let _ = std::fs::remove_file(vault_file()); } // migrou: apaga o texto
            return Ok(());
        }
        crate::ui::warn("sem chaveiro do sistema: o cofre fica em texto (vault.json, 0600)");
        let p = vault_file();
        std::fs::write(&p, json)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?; }
        Ok(())
    }
    /// Migra um vault.json em texto para o cofre cifrado (silencioso se já migrado ou sem chaveiro).
    pub fn migrate() {
        if vault_file().exists() && !vault_enc_file().exists() { let v = Vault::load(); if v.env.len() + v.files.len() > 0 { let _ = v.save(); } }
    }
    pub fn set_env(&mut self, name: &str, value: &str) -> Result<()> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("nome inválido “{name}” (use A-Z, 0-9, _)");
        }
        if value.contains('\n') { bail!("valor com quebra de linha: use `vault set-file`"); }
        self.env.insert(name.into(), value.into());
        self.save()
    }
    pub fn set_file(&mut self, path: &str, content: &[u8]) -> Result<()> {
        // O shell do usuário expande `~` para a home do host — devolve para relativo.
        let mut rel = path.to_string();
        if let Some(h) = dirs::home_dir() {
            if let Some(r) = path.strip_prefix(&format!("{}/", h.display())) { rel = r.to_string(); }
        }
        self.files.insert(rel.trim_start_matches("~/").to_string(), B64.encode(content));
        self.save()
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if self.env.remove(name).is_none() && self.files.remove(name.trim_start_matches("~/")).is_none() {
            bail!("“{name}” não está no vault");
        }
        self.save()
    }
}

/// Escreve o vault atual na máquina (precisa estar rodando).
pub fn push(m: &Machine) -> Result<usize> {
    let v = Vault::load();
    let mut sc = String::from("set -e\nsudo sed -i '/# hyzevm-vault begin/,/# hyzevm-vault end/d' /etc/environment\n");
    sc += "{ echo '# hyzevm-vault begin'\n";
    for (k, val) in &v.env { sc += &format!("echo {}\n", ssh::sh_quote(&format!("{k}=\"{}\"", val.replace('\\', "\\\\").replace('"', "\\\"")))); }
    sc += "echo '# hyzevm-vault end'; } | sudo tee -a /etc/environment >/dev/null\n";
    for (path, b64) in &v.files {
        let q = ssh::sh_quote(path);
        sc += &format!("mkdir -p \"$HOME/$(dirname {q})\" && echo {} | base64 -d > \"$HOME/\"{q} && chmod 600 \"$HOME/\"{q}\n", ssh::sh_quote(b64));
    }
    let o = ssh::exec_stdin(m, &sc)?;
    if !o.status.success() { bail!("vault push falhou: {}", String::from_utf8_lossy(&o.stderr)); }
    Ok(v.env.len() + v.files.len())
}
