use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64;
use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const ARGON2_PREFIX: &str = "$argon2id$v=19$m=19456,t=2,p=1$";
const ARGON2_MEMORY_KB: u32 = 19 * 1024;
const ARGON2_ITERATIONS: u32 = 2;
const ARGON2_PARALLELISM: u32 = 1;
const ARGON2_KEY_LEN: usize = 32;

pub fn valid_password(password: &str) -> bool {
    (8..=128).contains(&password.len())
}

pub fn validate_password(
    password: &str,
    is_admin: bool,
    permission: i32,
) -> Result<(), &'static str> {
    if password.is_empty() {
        return if !is_admin && permission & (1 << crate::db::PERM_ALLOW_EMPTY_PASSWORD) != 0 {
            Ok(())
        } else {
            Err("Password cannot be empty unless passwordless login is enabled")
        };
    }
    valid_password(password)
        .then_some(())
        .ok_or("Password length must be between 8 and 128 characters")
}

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

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_identifier(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex_encode(hasher.finalize())
}

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

pub fn hash_password(password: &str) -> String {
    let salt = rand_string(16);
    let argon2 = get_argon2_instance();
    let mut output_key = [0u8; ARGON2_KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut output_key)
        .expect("argon2 hash computation");

    let salt_b64 = BASE64.encode(salt.as_bytes());
    let hash_b64 = BASE64.encode(output_key);
    format!("{}{}${}", ARGON2_PREFIX, salt_b64, hash_b64)
}

pub fn verify_password(password: &str, pwd_hash: &str) -> bool {
    let Some(payload) = pwd_hash.strip_prefix(ARGON2_PREFIX) else {
        return false;
    };
    let Some((salt_b64, expected_hash_b64)) = payload.split_once('$') else {
        return false;
    };
    let (Ok(decoded_salt), Ok(expected_hash)) =
        (BASE64.decode(salt_b64), BASE64.decode(expected_hash_b64))
    else {
        return false;
    };
    if expected_hash.len() != ARGON2_KEY_LEN {
        return false;
    }

    let argon2 = get_argon2_instance();
    let mut actual_hash = [0u8; ARGON2_KEY_LEN];
    if argon2
        .hash_password_into(password.as_bytes(), &decoded_salt, &mut actual_hash)
        .is_err()
    {
        return false;
    }

    actual_hash
        .as_slice()
        .ct_eq(expected_hash.as_slice())
        .into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    #[serde(default)]
    pub user_id: i64,
    pub username: String,
    pub pwd_ts: i64,
    pub jti: String,
    pub exp: usize,
    pub iat: usize,
    pub nbf: usize,
}

pub fn generate_jwt(
    user_id: i64,
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
    let jti = hex_encode(nonce);
    let exp = now + (expires_in_hours as usize * 3600);

    let claims = UserClaims {
        user_id,
        username: username.to_string(),
        pwd_ts,
        jti,
        exp,
        iat: now,
        nbf: now,
    };

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    Ok(jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

pub fn parse_jwt(token_str: &str, secret: &str) -> anyhow::Result<UserClaims> {
    let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    let token_data = jsonwebtoken::decode::<UserClaims>(
        token_str,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(token_data.claims)
}

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn generate_otp_secret() -> String {
    let mut rng = thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..BASE32_ALPHABET.len());
            BASE32_ALPHABET[idx] as char
        })
        .collect()
}

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
    Some(format!("{:06}", binary_code % 1_000_000))
}

pub fn matching_totp_step(secret: &str, code: &str) -> Option<i64> {
    let clean_code = code.trim();
    if clean_code.len() != 6 || !clean_code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let step = now / 30;
    for candidate in [step.saturating_sub(1), step, step + 1] {
        if let Some(expected) = compute_totp(secret, candidate)
            && expected.as_bytes().ct_eq(clean_code.as_bytes()).into()
        {
            return i64::try_from(candidate).ok();
        }
    }
    None
}
