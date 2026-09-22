#![allow(dead_code)]

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64;
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const STATIC_HASH_SALT: &str = "https://github.com/alist-org/alist";
pub const ARGON2_PREFIX: &str = "$argon2id$v=19$m=19456,t=2,p=1$";
const ARGON2_MEMORY_KB: u32 = 19 * 1024;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;
const ARGON2_KEY_LEN: usize = 32;

/// Generate random alphanumeric string of length n
pub fn rand_string(n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// Generate random token formatted as openlist-<hex>
pub fn rand_token() -> String {
    let mut bytes = [0u8; 48];
    thread_rng().fill(&mut bytes[..]);
    format!("openlist-{}", hex::encode(bytes))
}

/// Compute initial static hash: SHA256(password + "-" + STATIC_HASH_SALT)
pub fn static_hash(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}", password, STATIC_HASH_SALT).as_bytes());
    hex::encode(hasher.finalize())
}

/// Compute legacy password hash: SHA256(static_hash + "-" + salt)
pub fn legacy_hash(pwd_static_hash: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}", pwd_static_hash, salt).as_bytes());
    hex::encode(hasher.finalize())
}

/// Create Argon2id instance with TinyList's exact parameter configuration
fn get_argon2_instance() -> Argon2<'static> {
    let params = Params::new(
        ARGON2_MEMORY_KB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(ARGON2_KEY_LEN),
    )
    .expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Encode password using Argon2id with raw base64 salt and output hash
pub fn encode_argon2_hash(pwd_static_hash: &str, salt: &str) -> String {
    let argon2 = get_argon2_instance();
    let mut output_key = [0u8; ARGON2_KEY_LEN];
    argon2
        .hash_password_into(
            pwd_static_hash.as_bytes(),
            salt.as_bytes(),
            &mut output_key,
        )
        .expect("argon2 hash computation");

    let salt_b64 = BASE64.encode(salt.as_bytes());
    let hash_b64 = BASE64.encode(output_key);
    format!("{}{}${}", ARGON2_PREFIX, salt_b64, hash_b64)
}

/// Verify password hash against either modern Argon2id or legacy SHA256 hash
pub fn verify_password(raw_password: &str, pwd_hash: &str, salt: &str) -> bool {
    let static_h = static_hash(raw_password);

    if let Some(payload) = pwd_hash.strip_prefix(ARGON2_PREFIX) {
        if let Some((salt_b64, expected_hash_b64)) = payload.split_once('$') {
            if let (Ok(decoded_salt), Ok(expected_hash)) = (
                BASE64.decode(salt_b64),
                BASE64.decode(expected_hash_b64),
            ) {
                if expected_hash.len() == ARGON2_KEY_LEN {
                    let argon2 = get_argon2_instance();
                    let mut actual_hash = [0u8; ARGON2_KEY_LEN];
                    if argon2
                        .hash_password_into(
                            static_h.as_bytes(),
                            &decoded_salt,
                            &mut actual_hash,
                        )
                        .is_ok()
                    {
                        return actual_hash.as_slice().ct_eq(expected_hash.as_slice()).into();
                    }
                }
            }
        }
        return false;
    }

    // Fallback: Legacy SHA256 hash comparison
    let expected_legacy = legacy_hash(&static_h, salt);
    pwd_hash.as_bytes().ct_eq(expected_legacy.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rand_string() {
        let s1 = rand_string(16);
        let s2 = rand_string(16);
        assert_eq!(s1.len(), 16);
        assert_eq!(s2.len(), 16);
        assert_ne!(s1, s2);
        assert_eq!(rand_string(0), "");
    }

    #[test]
    fn test_rand_token() {
        let t1 = rand_token();
        let t2 = rand_token();
        assert!(t1.starts_with("openlist-"));
        assert!(t2.starts_with("openlist-"));
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_argon2_hash_and_verify() {
        let password = "SuperSecretPassword123!";
        let salt = rand_string(16);
        let static_h = static_hash(password);
        let encoded_hash = encode_argon2_hash(&static_h, &salt);

        assert!(encoded_hash.starts_with(ARGON2_PREFIX));
        assert!(verify_password(password, &encoded_hash, &salt));
        assert!(!verify_password("wrong_password", &encoded_hash, &salt));
    }

    #[test]
    fn test_legacy_hash_verify() {
        let password = "LegacyPassword";
        let salt = "mysalt1234567890";
        let static_h = static_hash(password);
        let legacy_h = legacy_hash(&static_h, salt);

        assert!(verify_password(password, &legacy_h, salt));
        assert!(!verify_password("wrong", &legacy_h, salt));
    }
}
