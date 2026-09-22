#![allow(dead_code)]

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64;
use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const STATIC_HASH_SALT: &str = "https://github.com/alist-org/alist";
pub const ARGON2_PREFIX: &str = "$argon2id$v=19$m=19456,t=2,p=1$";
const ARGON2_MEMORY_KB: u32 = 19 * 1024;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;
const ARGON2_KEY_LEN: usize = 32;

/// Validate password length requirements (8 to 128 characters)
pub fn valid_password(password: &str) -> bool {
    (8..=128).contains(&password.len())
}

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

/// Generate random token formatted as rulist-<hex>
pub fn rand_token() -> String {
    let mut bytes = [0u8; 48];
    thread_rng().fill(&mut bytes[..]);
    format!("rulist-{}", hex::encode(bytes))
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

/// Create Argon2id instance with Rulist's exact parameter configuration
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
        .hash_password_into(pwd_static_hash.as_bytes(), salt.as_bytes(), &mut output_key)
        .expect("argon2 hash computation");

    let salt_b64 = BASE64.encode(salt.as_bytes());
    let hash_b64 = BASE64.encode(output_key);
    format!("{}{}${}", ARGON2_PREFIX, salt_b64, hash_b64)
}

/// Verify password when the client already sent the static hash (SHA256(pwd + "-https://github.com/alist-org/alist"))
fn verify_password_static_hash(static_h: &str, pwd_hash: &str, salt: &str) -> bool {
    if let Some(payload) = pwd_hash.strip_prefix(ARGON2_PREFIX) {
        if let Some((salt_b64, expected_hash_b64)) = payload.split_once('$')
            && let (Ok(decoded_salt), Ok(expected_hash)) =
                (BASE64.decode(salt_b64), BASE64.decode(expected_hash_b64))
            && expected_hash.len() == ARGON2_KEY_LEN
        {
            let argon2 = get_argon2_instance();
            let mut actual_hash = [0u8; ARGON2_KEY_LEN];
            if argon2
                .hash_password_into(static_h.as_bytes(), &decoded_salt, &mut actual_hash)
                .is_ok()
            {
                return actual_hash
                    .as_slice()
                    .ct_eq(expected_hash.as_slice())
                    .into();
            }
        }
        return false;
    }

    // Fallback: Legacy SHA256 hash comparison
    let expected_legacy = legacy_hash(static_h, salt);
    pwd_hash.as_bytes().ct_eq(expected_legacy.as_bytes()).into()
}

/// Verify password hash against either modern Argon2id or legacy SHA256 hash
pub fn verify_password(raw_password: &str, pwd_hash: &str, salt: &str) -> bool {
    let static_h = static_hash(raw_password);
    verify_password_static_hash(&static_h, pwd_hash, salt)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    pub username: String,
    pub pwd_ts: i64,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub nbf: usize,
}

pub fn generate_jwt(
    username: &str,
    pwd_ts: i64,
    secret: &str,
    expires_in_hours: u32,
) -> anyhow::Result<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize;

    let mut nonce = [0u8; 16];
    thread_rng().fill(&mut nonce[..]);
    let jti = hex::encode(nonce);

    let exp = now + (expires_in_hours as usize * 3600);

    let claims = UserClaims {
        username: username.to_string(),
        pwd_ts,
        jti,
        exp,
        iat: now,
        nbf: now,
    };

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn parse_jwt(token_str: &str, secret: &str) -> anyhow::Result<UserClaims> {
    let token_data = jsonwebtoken::decode::<UserClaims>(
        token_str,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )?;
    Ok(token_data.claims)
}

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Generate random 32-char Base32 TOTP secret (160 bits entropy)
pub fn generate_otp_secret() -> String {
    let mut rng = thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..BASE32_ALPHABET.len());
            BASE32_ALPHABET[idx] as char
        })
        .collect()
}

/// Decode RFC 4648 Base32 string into bytes (ignores padding, hyphens and whitespace, case-insensitive)
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut buffer: u64 = 0;
    let mut bits_in_buffer: usize = 0;
    let mut result = Vec::new();

    for ch in s.chars() {
        if ch == '=' || ch.is_whitespace() || ch == '-' {
            continue;
        }
        let val = match ch.to_ascii_uppercase() {
            'A'..='Z' => (ch.to_ascii_uppercase() as u8 - b'A') as u64,
            '2'..='7' => (ch as u8 - b'2' + 26) as u64,
            _ => return None,
        };
        buffer = (buffer << 5) | val;
        bits_in_buffer += 5;
        while bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            result.push((buffer >> bits_in_buffer) as u8);
            buffer &= (1 << bits_in_buffer) - 1;
        }
    }
    Some(result)
}

/// Compute 6-digit TOTP for a given time step counter (RFC 6238 / RFC 4226)
pub fn compute_totp(secret: &str, time_step: u64) -> Option<String> {
    let key = base32_decode(secret)?;
    let mut mac = HmacSha1::new_from_slice(&key).ok()?;
    mac.update(&time_step.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let binary_code = ((result[offset] & 0x7f) as u32) << 24
        | ((result[offset + 1] as u32) << 16)
        | ((result[offset + 2] as u32) << 8)
        | (result[offset + 3] as u32);
    let otp = binary_code % 1_000_000;
    Some(format!("{:06}", otp))
}

/// Verify 6-digit TOTP code with ±1 step (±30 seconds) tolerance
pub fn verify_totp(secret: &str, code: &str) -> bool {
    let clean_code = code.trim();
    if clean_code.len() != 6 || !clean_code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => return false,
    };
    let step = now / 30;
    for s in [step.saturating_sub(1), step, step + 1] {
        if let Some(expected) = compute_totp(secret, s)
            && expected.as_bytes().ct_eq(clean_code.as_bytes()).into()
        {
            return true;
        }
    }
    false
}

fn url_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Generate SVG Data URI QR Code for TOTP authenticator app
pub fn generate_totp_qr(issuer: &str, username: &str, secret: &str) -> anyhow::Result<String> {
    let enc_issuer = url_encode_component(issuer);
    let enc_username = url_encode_component(username);
    let label = format!("{}:{}", enc_issuer, enc_username);
    let otpauth_url = format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        label, secret, enc_issuer
    );

    let code = qrcode::QrCode::new(otpauth_url.as_bytes())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .build();
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{}", b64))
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
        assert!(t1.starts_with("rulist-"));
        assert!(t2.starts_with("rulist-"));
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

    #[test]
    fn test_base32_decode() {
        assert_eq!(base32_decode("").unwrap(), b"");
        assert_eq!(base32_decode("MY======").unwrap(), b"f");
        assert_eq!(base32_decode("MZXQ====").unwrap(), b"fo");
        assert_eq!(base32_decode("MZXW6===").unwrap(), b"foo");
        assert_eq!(base32_decode("MZXW6YQ=").unwrap(), b"foob");
        assert_eq!(base32_decode("MZXW6YTB").unwrap(), b"fooba");
        assert_eq!(base32_decode("MZXW6YTBOI======").unwrap(), b"foobar");
        assert_eq!(base32_decode("mzxw6ytboi").unwrap(), b"foobar");
    }

    #[test]
    fn test_rfc6238_totp_vectors() {
        // RFC 6238 Appendix B test secret: "12345678901234567890" in ASCII
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        assert_eq!(compute_totp(secret, 59 / 30).unwrap(), "287082");
        assert_eq!(compute_totp(secret, 1111111109 / 30).unwrap(), "081804");
        assert_eq!(compute_totp(secret, 1111111111 / 30).unwrap(), "050471");
        assert_eq!(compute_totp(secret, 1234567890 / 30).unwrap(), "005924");
        assert_eq!(compute_totp(secret, 2000000000 / 30).unwrap(), "279037");
    }

    #[test]
    fn test_totp_verify_and_qr() {
        let secret = generate_otp_secret();
        let now_step = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 30;
        let code = compute_totp(&secret, now_step).unwrap();
        assert!(verify_totp(&secret, &code));
        assert!(!verify_totp(&secret, "000000"));
        assert!(!verify_totp(&secret, "invalid"));

        let qr = generate_totp_qr("Rulist", "admin", &secret).unwrap();
        assert!(qr.starts_with("data:image/svg+xml;base64,"));
    }
}
