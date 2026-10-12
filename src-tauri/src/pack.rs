//! Encrypted connection pack import/export.

use crate::error::AppError;
use crate::models::{ConnectionDraft, ConnectionPack};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

fn key_from_passphrase(passphrase: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(passphrase.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

pub fn export_pack(
    drafts: &[ConnectionDraft],
    passphrase: &str,
) -> Result<ConnectionPack, AppError> {
    if passphrase.len() < 8 {
        return Err(AppError::coded(
            "PACK_PASSPHRASE",
            "Passphrase must be at least 8 characters",
        ));
    }
    let plaintext = serde_json::to_vec(drafts)?;
    let key = key_from_passphrase(passphrase);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| AppError::coded("PACK_CRYPTO", err.to_string()))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|err| AppError::coded("PACK_CRYPTO", err.to_string()))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend(ciphertext);
    Ok(ConnectionPack {
        version: 1,
        encrypted: true,
        payload: STANDARD.encode(blob),
    })
}

pub fn import_pack(
    pack: &ConnectionPack,
    passphrase: &str,
) -> Result<Vec<ConnectionDraft>, AppError> {
    if !pack.encrypted {
        let drafts: Vec<ConnectionDraft> = serde_json::from_str(&pack.payload)?;
        return Ok(drafts);
    }
    let blob = STANDARD
        .decode(&pack.payload)
        .map_err(|err| AppError::coded("PACK_DECODE", err.to_string()))?;
    if blob.len() < 13 {
        return Err(AppError::coded("PACK_DECODE", "Pack payload too short"));
    }
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let key = key_from_passphrase(passphrase);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| AppError::coded("PACK_CRYPTO", err.to_string()))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
        AppError::coded("PACK_DECRYPT", "Could not decrypt pack. Check passphrase.")
    })?;
    Ok(serde_json::from_slice(&plaintext)?)
}
