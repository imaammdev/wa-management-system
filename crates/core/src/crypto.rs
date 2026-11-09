//! Password hashing, API-key minting, token sealing and webhook signatures.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::password_hash::{rand_core::OsRng as ArgonRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use hmac::{Hmac, Mac};
use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------- passwords

pub fn hash_password(plain: &str) -> Result<String> {
    let salt = SaltString::generate(&mut ArgonRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| Error::Crypto(e.to_string()))
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

// ------------------------------------------------------------ random tokens

pub fn random_string(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

/// SHA-256 hex. Used for session tokens, invite tokens and API keys — values
/// we only ever need to look up by exact match, never to reverse.
pub fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// A freshly minted API key. The plaintext is shown to the user exactly once.
pub struct NewApiKey {
    /// e.g. `mms_live_a1b2c3d4` — stored, and used to find the row on auth.
    pub prefix: String,
    /// Full key the caller sends as `Authorization: Bearer ...`.
    pub plaintext: String,
    pub hash: String,
}

pub fn mint_api_key(live: bool) -> NewApiKey {
    let env_tag = if live { "live" } else { "test" };
    let id = random_string(8).to_lowercase();
    let secret = random_string(32);
    let prefix = format!("mms_{env_tag}_{id}");
    let plaintext = format!("{prefix}_{secret}");
    NewApiKey {
        hash: sha256_hex(&plaintext),
        prefix,
        plaintext,
    }
}

/// Splits `mms_live_abc12345_<secret>` back into its prefix for lookup.
pub fn api_key_prefix(plaintext: &str) -> Option<String> {
    let parts: Vec<&str> = plaintext.split('_').collect();
    if parts.len() < 4 || parts[0] != "mms" {
        return None;
    }
    Some(format!("{}_{}_{}", parts[0], parts[1], parts[2]))
}

// -------------------------------------------------------------- AES-256-GCM

/// Seals a provider access token. Output is `hex(nonce || ciphertext)`.
pub fn seal(key: &[u8; 32], plaintext: &str) -> Result<String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| Error::Crypto(format!("seal failed: {e}")))?;
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(hex::encode(out))
}

pub fn open(key: &[u8; 32], sealed_hex: &str) -> Result<String> {
    let raw = hex::decode(sealed_hex).map_err(|_| Error::Crypto("bad sealed hex".into()))?;
    if raw.len() < 13 {
        return Err(Error::Crypto("sealed value too short".into()));
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| Error::Crypto("open failed (wrong key or tampered)".into()))?;
    String::from_utf8(pt).map_err(|_| Error::Crypto("sealed value not utf8".into()))
}

// -------------------------------------------------- webhook signatures (out)

/// `sha256=<hex>` over the exact bytes we POST, same scheme Meta uses, so
/// customers can reuse any Meta-compatible verification helper.
pub fn sign_payload(secret: &str, body: &[u8]) -> String {
    // Fully qualified: `KeyInit` (aes-gcm) and `Mac` (hmac) both offer this.
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes())
        .expect("hmac accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Verifies an inbound `X-Hub-Signature-256` header from Meta.
pub fn verify_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let expected = sign_payload(secret, body);
    expected.as_bytes().ct_eq(header.as_bytes()).into()
}

pub fn new_webhook_secret() -> String {
    format!("whsec_{}", random_string(40))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("rahasia123").unwrap();
        assert!(verify_password("rahasia123", &h));
        assert!(!verify_password("salah", &h));
    }

    #[test]
    fn seal_roundtrip() {
        let key = [7u8; 32];
        let sealed = seal(&key, "EAAG_meta_token").unwrap();
        assert_ne!(sealed, "EAAG_meta_token");
        assert_eq!(open(&key, &sealed).unwrap(), "EAAG_meta_token");
        assert!(open(&[8u8; 32], &sealed).is_err());
    }

    #[test]
    fn api_key_prefix_matches_mint() {
        let k = mint_api_key(true);
        assert_eq!(
            api_key_prefix(&k.plaintext).as_deref(),
            Some(k.prefix.as_str())
        );
        assert_eq!(sha256_hex(&k.plaintext), k.hash);
    }

    #[test]
    fn signature_verifies() {
        let sig = sign_payload("whsec_abc", b"{\"a\":1}");
        assert!(verify_signature("whsec_abc", b"{\"a\":1}", &sig));
        assert!(!verify_signature("whsec_abc", b"{\"a\":2}", &sig));
    }
}
