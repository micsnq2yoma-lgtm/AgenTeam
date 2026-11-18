//! Shared verification for first-party downloadable resource manifests.
//!
//! The upstream resource publisher and AgenTeam application updates use separate keys.

use base64::{engine::general_purpose, Engine as _};
use minisign_verify::{PublicKey, Signature};

const MYAGENTS_MINISIGN_PUBKEY: &str = include_str!("../upstream-resources.pub");

fn decode_base64_text(value: &str, label: &str) -> Result<String, String> {
    let bytes = general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| format!("invalid {label} base64: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("invalid {label} UTF-8: {error}"))
}

pub(super) fn public_key() -> Result<PublicKey, String> {
    let decoded = decode_base64_text(MYAGENTS_MINISIGN_PUBKEY, "public key")?;
    PublicKey::decode(&decoded).map_err(|error| format!("invalid minisign public key: {error}"))
}

pub(super) fn signature(value: &str, label: &str) -> Result<Signature, String> {
    let decoded = decode_base64_text(value, label)?;
    Signature::decode(&decoded).map_err(|error| format!("invalid {label}: {error}"))
}

pub(crate) fn verify_minisign_bytes(
    bytes: &[u8],
    signature_value: &str,
    label: &str,
) -> Result<(), String> {
    verify_with_key(bytes, signature_value, MYAGENTS_MINISIGN_PUBKEY, label)
}

pub(crate) fn verify_with_key(bytes: &[u8], signature_value: &str, key: &str, label: &str) -> Result<(), String> {
    let key = decode_base64_text(key, "public key")?;
    let public_key = PublicKey::decode(&key).map_err(|e| format!("Invalid {label} public key: {e}"))?;
    let signature = signature(signature_value, label)?;
    public_key
        .verify(bytes, &signature, true)
        .map_err(|error| format!("{label} signature mismatch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    #[test]
    fn component_and_desktop_updates_use_separate_publishers() {
        let conf_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        let content = fs::read_to_string(conf_path).expect("tauri.conf.json");
        let json: serde_json::Value = serde_json::from_str(&content).expect("valid tauri config");
        let updater_pubkey = json
            .get("plugins")
            .and_then(|value| value.get("updater"))
            .and_then(|value| value.get("pubkey"))
            .and_then(|value| value.as_str())
            .expect("updater pubkey");
        assert_ne!(updater_pubkey, MYAGENTS_MINISIGN_PUBKEY.trim());
    }
}
