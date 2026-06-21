use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};

const NONCE_LENGTH: usize = 12;
const TAG_LENGTH: usize = 16;
const KEY_LENGTH: usize = 32;
const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT: &[u8] = b"memsnapshot_salt_v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedData {
    pub nonce: String,
    pub tag: String,
    pub ciphertext: String,
}

pub fn derive_key_from_password(password: &str) -> Vec<u8> {
    let mut key = vec![0u8; KEY_LENGTH];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), SALT, PBKDF2_ITERATIONS, &mut key);
    key
}

pub fn encrypt_data<T: Serialize>(data: &T, key: &[u8]) -> crate::error::AppResult<String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    
    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    let plaintext = serde_json::to_vec(data)?;
    let ciphertext_with_tag = cipher.encrypt(nonce, plaintext.as_ref())
        .map_err(|e| crate::error::AppError::Other(format!("加密失败: {}", e)))?;
    
    let tag = &ciphertext_with_tag[ciphertext_with_tag.len() - TAG_LENGTH..];
    let ciphertext = &ciphertext_with_tag[..ciphertext_with_tag.len() - TAG_LENGTH];
    
    let mut result = Vec::with_capacity(NONCE_LENGTH + TAG_LENGTH + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(tag);
    result.extend_from_slice(ciphertext);
    
    Ok(BASE64.encode(result))
}

pub fn decrypt_data<T: for<'de> Deserialize<'de>>(encrypted_b64: &str, key: &[u8]) -> crate::error::AppResult<T> {
    let buf = BASE64.decode(encrypted_b64)
        .map_err(|e| crate::error::AppError::Other(format!("Base64 解码失败: {}", e)))?;
    
    if buf.len() < NONCE_LENGTH + TAG_LENGTH {
        return Err(crate::error::AppError::Other("加密数据格式错误".into()));
    }
    
    let nonce_bytes = &buf[..NONCE_LENGTH];
    let tag = &buf[NONCE_LENGTH..NONCE_LENGTH + TAG_LENGTH];
    let ciphertext = &buf[NONCE_LENGTH + TAG_LENGTH..];
    
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    
    let mut ciphertext_with_tag = Vec::with_capacity(ciphertext.len() + TAG_LENGTH);
    ciphertext_with_tag.extend_from_slice(ciphertext);
    ciphertext_with_tag.extend_from_slice(tag);
    
    let plaintext = cipher.decrypt(nonce, ciphertext_with_tag.as_ref())
        .map_err(|e| crate::error::AppError::Other(format!("解密失败: {}", e)))?;
    
    Ok(serde_json::from_slice(&plaintext)?)
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

pub fn generate_device_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn generate_hex_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
