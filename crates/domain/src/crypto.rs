/// 端到端加密原语：AES-256-GCM 字段加密 + Argon2id 密钥派生。
///
/// 设计约定：
/// - DEK（数据加密密钥）：随机 256-bit，加密所有笔记字段，只存内存。
/// - KEK（密钥加密密钥）：由用户密码 + kek_salt 通过 Argon2id 派生，永不离开客户端。
/// - encrypted_dek：KEK 包裹后的 DEK，base64(nonce || ciphertext)，存服务端。
/// - 每次加密字段时生成独立随机 nonce，wire 格式同为 base64(nonce || ciphertext)。
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};

const NONCE_LEN: usize = 12;

/// 生成随机 256-bit 数据加密密钥。
pub fn generate_dek() -> [u8; 32] {
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    key
}

/// 生成随机 256-bit KEK 盐。
pub fn generate_kek_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// 从用户密码派生 KEK（Argon2id，与服务端密码哈希参数独立）。
pub fn derive_kek(password: &str, salt: &[u8; 32]) -> Result<[u8; 32]> {
    let params = Params::new(65536, 3, 4, Some(32)).map_err(|e| anyhow!("argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("kek derive: {e}"))?;
    Ok(key)
}

/// 用 KEK 包裹 DEK，返回 base64(nonce || ciphertext)。
pub fn wrap_dek(kek: &[u8; 32], dek: &[u8; 32]) -> Result<String> {
    Ok(B64.encode(seal(kek, dek)?))
}

/// 用 KEK 解包 DEK。
pub fn unwrap_dek(kek: &[u8; 32], wrapped: &str) -> Result<[u8; 32]> {
    let plaintext = open(kek, wrapped)?;
    plaintext
        .try_into()
        .map_err(|_| anyhow!("unwrapped dek has wrong length"))
}

/// 用 DEK 加密单个文本字段，返回 base64(nonce || ciphertext)。
pub fn encrypt_field(dek: &[u8; 32], plaintext: &str) -> Result<String> {
    Ok(B64.encode(seal(dek, plaintext.as_bytes())?))
}

/// 用 DEK 解密单个文本字段。
pub fn decrypt_field(dek: &[u8; 32], encoded: &str) -> Result<String> {
    let plaintext = open(dek, encoded)?;
    String::from_utf8(plaintext).map_err(|e| anyhow!("decrypt utf8: {e}"))
}

/// base64-encode a raw 32-byte salt for transport.
pub fn encode_salt(salt: &[u8; 32]) -> String {
    B64.encode(salt)
}

/// Decode a base64-encoded 32-byte salt.
pub fn decode_salt(encoded: &str) -> Result<[u8; 32]> {
    let bytes = B64
        .decode(encoded)
        .map_err(|e| anyhow!("decode salt: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("salt has wrong length"))
}

// ── internal helpers ──────────────────────────────────────────────────────────

fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut out = nonce_bytes.to_vec();
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow!("aes-gcm encrypt: {e}"))?;
    out.extend(ct);
    Ok(out)
}

fn open(key: &[u8; 32], encoded: &str) -> Result<Vec<u8>> {
    let blob = B64
        .decode(encoded)
        .map_err(|e| anyhow!("base64 decode: {e}"))?;
    if blob.len() < NONCE_LEN {
        return Err(anyhow!("ciphertext too short"));
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow!("aes-gcm decrypt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_encrypt_decrypt_round_trip() {
        let dek = generate_dek();
        let plaintext = "Hello, E2EE!";
        let encoded = encrypt_field(&dek, plaintext).unwrap();
        assert_ne!(encoded, plaintext);
        assert_eq!(decrypt_field(&dek, &encoded).unwrap(), plaintext);
    }

    #[test]
    fn wrap_unwrap_dek_round_trip() {
        let kek = derive_kek("password", &generate_kek_salt()).unwrap();
        let dek = generate_dek();
        let wrapped = wrap_dek(&kek, &dek).unwrap();
        let unwrapped = unwrap_dek(&kek, &wrapped).unwrap();
        assert_eq!(dek, unwrapped);
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let dek = generate_dek();
        let other_key = generate_dek();
        let encoded = encrypt_field(&dek, "secret").unwrap();
        assert!(decrypt_field(&other_key, &encoded).is_err());
    }

    #[test]
    fn salt_encode_decode_round_trip() {
        let salt = generate_kek_salt();
        assert_eq!(decode_salt(&encode_salt(&salt)).unwrap(), salt);
    }
}
