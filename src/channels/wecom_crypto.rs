use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

/// WeCom message cryptography (AES-256-CBC + PKCS7 padding)
pub struct WeComCrypto {
    aes_key: Vec<u8>,
    corpid: String,
}

impl WeComCrypto {
    /// Create a new crypto instance
    ///
    /// `encoding_aes_key`: 43-char base64 string from WeCom admin console
    /// `corpid`: Enterprise ID
    pub fn new(encoding_aes_key: &str, corpid: &str) -> Result<Self> {
        // Decode the base64 key (43 chars + '=' padding -> 32 bytes)
        // WeCom uses base64 without padding, so add it if needed
        let mut padded_key = encoding_aes_key.to_string();
        while padded_key.len() % 4 != 0 {
            padded_key.push('=');
        }

        let aes_key = BASE64.decode(padded_key.as_bytes())?;
        if aes_key.len() != 32 {
            bail!(
                "Invalid encoding_aes_key: must decode to 32 bytes (AES-256), got {} bytes",
                aes_key.len()
            );
        }

        Ok(Self {
            aes_key,
            corpid: corpid.to_string(),
        })
    }

    /// Decrypt encrypted message from WeCom callback
    ///
    /// Returns decrypted plaintext
    pub fn decrypt(&self, encrypted: &str) -> Result<String> {
        use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

        // Base64 decode the encrypted message
        let encrypted_bytes = BASE64.decode(encrypted.as_bytes())?;

        // First 16 bytes are IV, rest is ciphertext
        if encrypted_bytes.len() < 16 {
            bail!("Encrypted message too short (< 16 bytes)");
        }

        let iv = &encrypted_bytes[..16];
        let ciphertext = &encrypted_bytes[16..];

        // Decrypt using AES-256-CBC
        type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
        let cipher = Aes256CbcDec::new_from_slices(&self.aes_key, iv)?;

        let mut buffer = ciphertext.to_vec();
        let decrypted = cipher
            .decrypt_padded_mut::<Pkcs7>(&mut buffer)
            .map_err(|e| anyhow::anyhow!("AES decryption failed: {e}"))?;

        // Parse decrypted format: [random 16 bytes][4-byte msg_len][msg][corpid]
        if decrypted.len() < 20 {
            bail!("Decrypted message too short (< 20 bytes)");
        }

        // Skip random 16 bytes
        let content = &decrypted[16..];

        // Read msg_len (big-endian u32)
        let msg_len = u32::from_be_bytes([content[0], content[1], content[2], content[3]]) as usize;

        // Extract message
        if content.len() < 4 + msg_len {
            bail!("Message length exceeds decrypted data");
        }
        let msg = &content[4..4 + msg_len];

        // Verify corpid
        let corpid_start = 4 + msg_len;
        if content.len() < corpid_start {
            bail!("Missing corpid in decrypted message");
        }
        let corpid = String::from_utf8_lossy(&content[corpid_start..]);
        if corpid != self.corpid {
            bail!("Corpid mismatch: expected {}, got {corpid}", self.corpid);
        }

        Ok(String::from_utf8(msg.to_vec())?)
    }

    /// Encrypt message for WeCom reply
    ///
    /// Returns base64-encoded encrypted message
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        use rand::Rng;

        // Generate random 16-byte IV
        let mut rng = rand::thread_rng();
        let iv: [u8; 16] = rng.gen();

        // Build plaintext format: [random 16 bytes][4-byte msg_len][msg][corpid]
        let mut plaintext_bytes = Vec::new();

        // 1. Random 16 bytes
        let random_bytes: [u8; 16] = rng.gen();
        plaintext_bytes.extend_from_slice(&random_bytes);

        // 2. Message length (big-endian u32)
        let msg_bytes = plaintext.as_bytes();
        plaintext_bytes.extend_from_slice(&(msg_bytes.len() as u32).to_be_bytes());

        // 3. Message
        plaintext_bytes.extend_from_slice(msg_bytes);

        // 4. Corpid
        plaintext_bytes.extend_from_slice(self.corpid.as_bytes());

        // Allocate buffer with room for padding (up to 16 extra bytes)
        let mut buffer = vec![0u8; plaintext_bytes.len() + 16];
        buffer[..plaintext_bytes.len()].copy_from_slice(&plaintext_bytes);

        // Encrypt using AES-256-CBC with PKCS7 padding
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
        let ciphertext = Aes256CbcEnc::new_from_slices(&self.aes_key, &iv)?
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext_bytes.len())
            .map_err(|e| anyhow::anyhow!("AES encryption failed: {e}"))?;

        // Combine IV + ciphertext and base64 encode
        let mut result = Vec::new();
        result.extend_from_slice(&iv);
        result.extend_from_slice(ciphertext);

        Ok(BASE64.encode(&result))
    }

    /// Verify signature for callback URL verification
    ///
    /// Signature = SHA256(sort(token, timestamp, nonce, encrypted))
    pub fn verify_signature(
        token: &str,
        timestamp: &str,
        nonce: &str,
        encrypted: &str,
        signature: &str,
    ) -> Result<bool> {
        let mut items = vec![token, timestamp, nonce, encrypted];
        items.sort();

        let concat = items.join("");
        let mut hasher = Sha256::new();
        hasher.update(concat.as_bytes());
        let hash = hasher.finalize();

        let computed = hex::encode(hash);
        Ok(computed == signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_roundtrip() {
        // Valid 43-char base64 string (WeCom format - no padding)
        let key = "7oCvxzgCP3d3RLzzfhitAz2aiG3HyprpiVSDeH3W4bQ"; // Real base64, 43 chars
        let corpid = "ww1234567890abcdef";

        let crypto = WeComCrypto::new(key, corpid).unwrap();

        let plaintext = "Hello, WeCom!";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn verify_signature_success() {
        let result = WeComCrypto::verify_signature(
            "token123",
            "1234567890",
            "nonce456",
            "encrypted_data",
            // Pre-computed SHA256(sort(...))
            &hex::encode(sha2::Sha256::digest(
                b"1234567890encrypted_datanonce456token123",
            )),
        );

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn verify_signature_failure() {
        let result = WeComCrypto::verify_signature(
            "token123",
            "1234567890",
            "nonce456",
            "encrypted_data",
            "wrong_signature",
        );

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
