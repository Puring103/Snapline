use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use zeroize::Zeroize;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 16;
const PASSWORD_AAD: &[u8] = b"snapline:umk:password:v1";
const RECOVERY_AAD: &[u8] = b"snapline:umk:recovery:v1";
const ATTACHMENT_NONCE_PREFIX_LEN: usize = 16;
const ATTACHMENT_CHUNK_BYTES: usize = 1024 * 1024;
const ATTACHMENT_TAG_BYTES: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("encrypted data is malformed")]
    Malformed,
    #[error("key derivation failed")]
    Derivation,
    #[error("authentication failed")]
    Authentication,
    #[error("attachment input or output failed")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyEnvelope {
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedRecord {
    pub nonce: String,
    pub ciphertext: String,
    pub key_nonce: String,
    pub wrapped_key: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedAttachmentHeader {
    pub nonce_prefix: String,
    pub key_nonce: String,
    pub wrapped_key: String,
    pub chunk_bytes: u32,
}

pub struct MasterKey([u8; KEY_LEN]);

pub struct RecoveryKey([u8; KEY_LEN]);

impl Drop for MasterKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for RecoveryKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl MasterKey {
    pub fn generate() -> Self {
        let mut bytes = [0_u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn wrap_with_password(&self, password: &str) -> Result<KeyEnvelope, CryptoError> {
        let mut salt = [0_u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut key = derive_password_key(password, &salt)?;
        let result = encrypt_key(&key, &self.0, &salt, PASSWORD_AAD);
        key.zeroize();
        result
    }

    pub fn unwrap_with_password(
        password: &str,
        envelope: &KeyEnvelope,
    ) -> Result<Self, CryptoError> {
        let salt = decode_exact::<SALT_LEN>(&envelope.salt)?;
        let mut key = derive_password_key(password, &salt)?;
        let result = decrypt_key(&key, envelope, PASSWORD_AAD).map(Self);
        key.zeroize();
        result
    }

    pub fn wrap_with_recovery(&self, recovery: &RecoveryKey) -> Result<KeyEnvelope, CryptoError> {
        encrypt_key(&recovery.0, &self.0, &[], RECOVERY_AAD)
    }

    pub fn unwrap_with_recovery(
        recovery: &RecoveryKey,
        envelope: &KeyEnvelope,
    ) -> Result<Self, CryptoError> {
        decrypt_key(&recovery.0, envelope, RECOVERY_AAD).map(Self)
    }

    pub fn encrypt(
        &self,
        object_id: &[u8],
        plaintext: &[u8],
    ) -> Result<EncryptedRecord, CryptoError> {
        let mut data_key = [0_u8; KEY_LEN];
        let mut nonce = [0_u8; NONCE_LEN];
        let mut key_nonce = [0_u8; NONCE_LEN];
        OsRng.fill_bytes(&mut data_key);
        OsRng.fill_bytes(&mut nonce);
        OsRng.fill_bytes(&mut key_nonce);
        let ciphertext = cipher(&data_key)
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication)?;
        let wrapped_key = cipher(&self.0)
            .encrypt(
                XNonce::from_slice(&key_nonce),
                Payload {
                    msg: &data_key,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication)?;
        data_key.zeroize();
        Ok(EncryptedRecord {
            nonce: encode(nonce),
            ciphertext: encode(ciphertext),
            key_nonce: encode(key_nonce),
            wrapped_key: encode(wrapped_key),
        })
    }

    pub fn decrypt(
        &self,
        object_id: &[u8],
        record: &EncryptedRecord,
    ) -> Result<Vec<u8>, CryptoError> {
        let nonce = decode_exact::<NONCE_LEN>(&record.nonce)?;
        let key_nonce = decode_exact::<NONCE_LEN>(&record.key_nonce)?;
        let wrapped_key = decode(&record.wrapped_key)?;
        let mut data_key = cipher(&self.0)
            .decrypt(
                XNonce::from_slice(&key_nonce),
                Payload {
                    msg: &wrapped_key,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication)?;
        if data_key.len() != KEY_LEN {
            data_key.zeroize();
            return Err(CryptoError::Malformed);
        }
        let ciphertext = decode(&record.ciphertext)?;
        let plaintext = cipher(&data_key)
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication);
        data_key.zeroize();
        plaintext
    }

    pub fn encrypt_attachment(
        &self,
        object_id: &[u8],
        mut reader: impl Read,
        mut writer: impl Write,
    ) -> Result<EncryptedAttachmentHeader, CryptoError> {
        let mut data_key = [0_u8; KEY_LEN];
        let mut nonce_prefix = [0_u8; ATTACHMENT_NONCE_PREFIX_LEN];
        let mut key_nonce = [0_u8; NONCE_LEN];
        OsRng.fill_bytes(&mut data_key);
        OsRng.fill_bytes(&mut nonce_prefix);
        OsRng.fill_bytes(&mut key_nonce);
        let wrapped_key = cipher(&self.0)
            .encrypt(
                XNonce::from_slice(&key_nonce),
                Payload {
                    msg: &data_key,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication)?;

        let result = encrypt_chunks(
            &data_key,
            &nonce_prefix,
            object_id,
            &mut reader,
            &mut writer,
        );
        data_key.zeroize();
        result?;
        Ok(EncryptedAttachmentHeader {
            nonce_prefix: encode(nonce_prefix),
            key_nonce: encode(key_nonce),
            wrapped_key: encode(wrapped_key),
            chunk_bytes: ATTACHMENT_CHUNK_BYTES as u32,
        })
    }

    pub fn decrypt_attachment(
        &self,
        object_id: &[u8],
        header: &EncryptedAttachmentHeader,
        mut reader: impl Read,
        mut writer: impl Write,
    ) -> Result<(), CryptoError> {
        if header.chunk_bytes as usize != ATTACHMENT_CHUNK_BYTES {
            return Err(CryptoError::Malformed);
        }
        let nonce_prefix = decode_exact::<ATTACHMENT_NONCE_PREFIX_LEN>(&header.nonce_prefix)?;
        let key_nonce = decode_exact::<NONCE_LEN>(&header.key_nonce)?;
        let wrapped_key = decode(&header.wrapped_key)?;
        let mut data_key = cipher(&self.0)
            .decrypt(
                XNonce::from_slice(&key_nonce),
                Payload {
                    msg: &wrapped_key,
                    aad: object_id,
                },
            )
            .map_err(|_| CryptoError::Authentication)?;
        if data_key.len() != KEY_LEN {
            data_key.zeroize();
            return Err(CryptoError::Malformed);
        }
        let result = decrypt_chunks(
            &data_key,
            &nonce_prefix,
            object_id,
            &mut reader,
            &mut writer,
        );
        data_key.zeroize();
        result
    }
}

fn encrypt_chunks(
    data_key: &[u8],
    nonce_prefix: &[u8; ATTACHMENT_NONCE_PREFIX_LEN],
    object_id: &[u8],
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<(), CryptoError> {
    let mut index = 0_u64;
    let mut buffer = vec![0_u8; ATTACHMENT_CHUNK_BYTES];
    loop {
        let read = read_chunk(reader, &mut buffer)?;
        if read == 0 {
            let ciphertext =
                encrypt_attachment_chunk(data_key, nonce_prefix, object_id, index, true, &[])?;
            write_frame(writer, &ciphertext)?;
            return Ok(());
        }
        let ciphertext = encrypt_attachment_chunk(
            data_key,
            nonce_prefix,
            object_id,
            index,
            false,
            &buffer[..read],
        )?;
        write_frame(writer, &ciphertext)?;
        index = index.checked_add(1).ok_or(CryptoError::Malformed)?;
    }
}

fn decrypt_chunks(
    data_key: &[u8],
    nonce_prefix: &[u8; ATTACHMENT_NONCE_PREFIX_LEN],
    object_id: &[u8],
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<(), CryptoError> {
    let mut index = 0_u64;
    loop {
        let frame = read_frame(reader)?;
        if frame.len() < ATTACHMENT_TAG_BYTES
            || frame.len() > ATTACHMENT_CHUNK_BYTES + ATTACHMENT_TAG_BYTES
        {
            return Err(CryptoError::Malformed);
        }
        let final_frame = frame.len() == ATTACHMENT_TAG_BYTES;
        let plaintext = cipher(data_key)
            .decrypt(
                XNonce::from_slice(&attachment_nonce(nonce_prefix, index)),
                Payload {
                    msg: &frame,
                    aad: &attachment_aad(object_id, index, final_frame),
                },
            )
            .map_err(|_| CryptoError::Authentication)?;
        if final_frame {
            if !plaintext.is_empty() || has_trailing_bytes(reader)? {
                return Err(CryptoError::Malformed);
            }
            return Ok(());
        }
        writer.write_all(&plaintext)?;
        index = index.checked_add(1).ok_or(CryptoError::Malformed)?;
    }
}

fn read_chunk(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
    let mut offset = 0;
    while offset < buffer.len() {
        match reader.read(&mut buffer[offset..])? {
            0 => break,
            read => offset += read,
        }
    }
    Ok(offset)
}

fn encrypt_attachment_chunk(
    data_key: &[u8],
    nonce_prefix: &[u8; ATTACHMENT_NONCE_PREFIX_LEN],
    object_id: &[u8],
    index: u64,
    final_frame: bool,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    cipher(data_key)
        .encrypt(
            XNonce::from_slice(&attachment_nonce(nonce_prefix, index)),
            Payload {
                msg: plaintext,
                aad: &attachment_aad(object_id, index, final_frame),
            },
        )
        .map_err(|_| CryptoError::Authentication)
}

fn attachment_nonce(prefix: &[u8; ATTACHMENT_NONCE_PREFIX_LEN], index: u64) -> [u8; NONCE_LEN] {
    let mut nonce = [0_u8; NONCE_LEN];
    nonce[..ATTACHMENT_NONCE_PREFIX_LEN].copy_from_slice(prefix);
    nonce[ATTACHMENT_NONCE_PREFIX_LEN..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn attachment_aad(object_id: &[u8], index: u64, final_frame: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(object_id.len() + 9);
    aad.extend_from_slice(object_id);
    aad.extend_from_slice(&index.to_be_bytes());
    aad.push(u8::from(final_frame));
    aad
}

fn write_frame(writer: &mut impl Write, ciphertext: &[u8]) -> Result<(), std::io::Error> {
    let length = u32::try_from(ciphertext.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "frame too large"))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(ciphertext)
}

fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, CryptoError> {
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if !(ATTACHMENT_TAG_BYTES..=ATTACHMENT_CHUNK_BYTES + ATTACHMENT_TAG_BYTES).contains(&length) {
        return Err(CryptoError::Malformed);
    }
    let mut frame = vec![0_u8; length];
    reader.read_exact(&mut frame)?;
    Ok(frame)
}

fn has_trailing_bytes(reader: &mut impl Read) -> Result<bool, std::io::Error> {
    let mut byte = [0_u8; 1];
    Ok(reader.read(&mut byte)? != 0)
}

impl RecoveryKey {
    pub fn generate() -> Self {
        let mut bytes = [0_u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn expose_once(&self) -> String {
        encode(self.0)
    }

    pub fn parse(value: &str) -> Result<Self, CryptoError> {
        decode_exact::<KEY_LEN>(value).map(Self)
    }
}

fn derive_password_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], CryptoError> {
    let params =
        Params::new(64 * 1024, 3, 1, Some(KEY_LEN)).map_err(|_| CryptoError::Derivation)?;
    let mut output = [0_u8; KEY_LEN];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut output)
        .map_err(|_| CryptoError::Derivation)?;
    Ok(output)
}

fn encrypt_key(
    wrapping_key: &[u8; KEY_LEN],
    key: &[u8; KEY_LEN],
    salt: &[u8],
    aad: &[u8],
) -> Result<KeyEnvelope, CryptoError> {
    let mut nonce = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher(wrapping_key)
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: key, aad })
        .map_err(|_| CryptoError::Authentication)?;
    Ok(KeyEnvelope {
        salt: encode(salt),
        nonce: encode(nonce),
        ciphertext: encode(ciphertext),
    })
}

fn decrypt_key(
    wrapping_key: &[u8; KEY_LEN],
    envelope: &KeyEnvelope,
    aad: &[u8],
) -> Result<[u8; KEY_LEN], CryptoError> {
    let nonce = decode_exact::<NONCE_LEN>(&envelope.nonce)?;
    let ciphertext = decode(&envelope.ciphertext)?;
    let plaintext = cipher(wrapping_key)
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Authentication)?;
    plaintext.try_into().map_err(|_| CryptoError::Malformed)
}

fn cipher(key: &[u8]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new_from_slice(key).expect("fixed key length")
}

fn encode(value: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(value)
}

fn decode(value: &str) -> Result<Vec<u8>, CryptoError> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CryptoError::Malformed)
}

fn decode_exact<const N: usize>(value: &str) -> Result<[u8; N], CryptoError> {
    decode(value)?
        .try_into()
        .map_err(|_| CryptoError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_and_recovery_unlock_same_master_key() {
        let master = MasterKey::generate();
        let recovery = RecoveryKey::generate();
        let password_envelope = master.wrap_with_password("a long local password").unwrap();
        let recovery_envelope = master.wrap_with_recovery(&recovery).unwrap();
        let from_password =
            MasterKey::unwrap_with_password("a long local password", &password_envelope).unwrap();
        let from_recovery = MasterKey::unwrap_with_recovery(&recovery, &recovery_envelope).unwrap();
        let record = master.encrypt(b"item-id", b"private markdown").unwrap();
        assert_eq!(
            from_password.decrypt(b"item-id", &record).unwrap(),
            b"private markdown"
        );
        assert_eq!(
            from_recovery.decrypt(b"item-id", &record).unwrap(),
            b"private markdown"
        );
    }

    #[test]
    fn wrong_password_and_tampering_are_rejected() {
        let master = MasterKey::generate();
        let envelope = master.wrap_with_password("correct password").unwrap();
        assert!(MasterKey::unwrap_with_password("wrong password", &envelope).is_err());
        let mut record = master.encrypt(b"item-id", b"private markdown").unwrap();
        record.ciphertext.replace_range(..1, "A");
        assert!(master.decrypt(b"item-id", &record).is_err());
    }

    #[test]
    fn associated_object_id_prevents_record_swapping() {
        let master = MasterKey::generate();
        let record = master.encrypt(b"first-id", b"private markdown").unwrap();
        assert!(master.decrypt(b"second-id", &record).is_err());
    }

    #[test]
    fn attachment_stream_round_trip_crosses_chunk_boundaries() {
        let master = MasterKey::generate();
        let plaintext = (0..ATTACHMENT_CHUNK_BYTES * 2 + 913)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let mut ciphertext = Vec::new();
        let header = master
            .encrypt_attachment(b"attachment-id", plaintext.as_slice(), &mut ciphertext)
            .unwrap();
        assert!(
            !ciphertext
                .windows(64)
                .any(|window| window == &plaintext[..64])
        );
        let mut restored = Vec::new();
        master
            .decrypt_attachment(
                b"attachment-id",
                &header,
                ciphertext.as_slice(),
                &mut restored,
            )
            .unwrap();
        assert_eq!(restored, plaintext);
    }

    #[test]
    fn attachment_stream_rejects_tampering_swapping_and_truncation() {
        let master = MasterKey::generate();
        let mut ciphertext = Vec::new();
        let header = master
            .encrypt_attachment(
                b"first-id",
                b"private attachment".as_slice(),
                &mut ciphertext,
            )
            .unwrap();
        assert!(
            master
                .decrypt_attachment(b"second-id", &header, ciphertext.as_slice(), Vec::new(),)
                .is_err()
        );
        let mut tampered = ciphertext.clone();
        tampered[8] ^= 1;
        assert!(
            master
                .decrypt_attachment(b"first-id", &header, tampered.as_slice(), Vec::new())
                .is_err()
        );
        ciphertext.pop();
        assert!(
            master
                .decrypt_attachment(b"first-id", &header, ciphertext.as_slice(), Vec::new(),)
                .is_err()
        );
    }
}
