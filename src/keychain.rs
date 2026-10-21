//! Chave do cofre no chaveiro do sistema + cifra AES-256-GCM.
//! macOS: Keychain (`security`); Windows: DPAPI (PowerShell, ligada ao usuário); Linux: `secret-tool` (libsecret).
//! Protege o arquivo em disco (backup, disco roubado, script que lê ~/.hyzevm); não protege de um programa
//! rodando como você, que pode pedir a mesma chave ao sistema.
use crate::store::home;
use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::process::Command;

const SERVICE: &str = "hyzevm-vault";

fn out(c: &mut Command) -> Option<String> {
    let o = c.output().ok()?;
    if !o.status.success() { return None; }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn fetch() -> Option<String> {
    match std::env::consts::OS {
        "macos" => out(Command::new("security").args(["find-generic-password", "-s", SERVICE, "-a", "hyzevm", "-w"])),
        "linux" => out(Command::new("secret-tool").args(["lookup", "service", SERVICE])),
        "windows" => {
            let f = home().join("vault.key");
            let enc = std::fs::read_to_string(f).ok()?;
            out(Command::new("powershell").args(["-NoProfile", "-Command",
                &format!("$s = ConvertTo-SecureString '{}'; [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($s))", enc.trim())]))
        }
        _ => None,
    }
}

fn store(b64key: &str) -> bool {
    match std::env::consts::OS {
        // -T security: o próprio `security` lê sem pedir senha a cada atualização do app; sem -A (não é "qualquer app")
        "macos" => Command::new("security").args(["add-generic-password", "-U", "-s", SERVICE, "-a", "hyzevm", "-T", "/usr/bin/security", "-w", b64key])
            .output().is_ok_and(|o| o.status.success()),
        "linux" => {
            use std::io::Write;
            let Ok(mut c) = Command::new("secret-tool").args(["store", "--label", "HyzeVM vault", "service", SERVICE]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() else { return false };
            if let Some(mut i) = c.stdin.take() { let _ = i.write_all(b64key.as_bytes()); }
            c.wait().is_ok_and(|s| s.success())
        }
        "windows" => {
            let Some(enc) = out(Command::new("powershell").args(["-NoProfile", "-Command",
                &format!("ConvertTo-SecureString '{b64key}' -AsPlainText -Force | ConvertFrom-SecureString")])) else { return false };
            std::fs::create_dir_all(home()).is_ok() && std::fs::write(home().join("vault.key"), enc).is_ok()
        }
        _ => false,
    }
}

/// Chave de 32 bytes do cofre. `create`: gera e guarda uma nova se não existir. None = sem chaveiro.
pub fn key(create: bool) -> Option<[u8; 32]> {
    let decode = |s: String| B64.decode(s).ok().and_then(|b| <[u8; 32]>::try_from(b).ok());
    if let Some(k) = fetch().and_then(decode) { return Some(k); }
    if !create { return None; }
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).ok()?;
    if store(&B64.encode(k)) && fetch().and_then(decode) == Some(k) { Some(k) } else { None }
}

/// nonce(12) || ciphertext
pub fn seal(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    let mut n = [0u8; 12];
    getrandom::getrandom(&mut n).map_err(|e| anyhow!("{e}"))?;
    let c = Aes256Gcm::new(key.into()).encrypt(Nonce::from_slice(&n), data).map_err(|_| anyhow!("falha ao cifrar"))?;
    Ok([n.to_vec(), c].concat())
}

pub fn open(key: &[u8; 32], blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < 13 { return None; }
    Aes256Gcm::new(key.into()).decrypt(Nonce::from_slice(&blob[..12]), &blob[12..]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seal_open_roundtrip_and_tamper() {
        let k = [7u8; 32];
        let blob = seal(&k, b"segredo").unwrap();
        assert!(!blob.windows(7).any(|w| w == b"segredo"));
        assert_eq!(open(&k, &blob).as_deref(), Some(&b"segredo"[..]));
        assert!(open(&[8u8; 32], &blob).is_none(), "chave errada");
        let mut bad = blob.clone(); *bad.last_mut().unwrap() ^= 1;
        assert!(open(&k, &bad).is_none(), "dado adulterado");
    }
}
