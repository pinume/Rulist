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

pub fn valid_password(password: &str) -> bool {
    (8..=128).contains(&password.len())
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

pub fn rand_token() -> String {
    let mut bytes = [0u8; 48];
    thread_rng().fill(&mut bytes[..]);
    format!("rulist-{}", hex_encode(bytes))
}

pub fn static_hash(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}", password, STATIC_HASH_SALT).as_bytes());
    hex_encode(hasher.finalize())
}

pub fn legacy_hash(pwd_static_hash: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}", pwd_static_hash, salt).as_bytes());
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

    let expected_legacy = legacy_hash(static_h, salt);
    pwd_hash.as_bytes().ct_eq(expected_legacy.as_bytes()).into()
}

pub fn verify_password(raw_password: &str, pwd_hash: &str, salt: &str) -> bool {
    let static_h = static_hash(raw_password);
    verify_password_static_hash(&static_h, pwd_hash, salt)
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

    Ok(jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )?)
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

fn url_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric()
            || byte == b'-'
            || byte == b'_'
            || byte == b'.'
            || byte == b'~'
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn generate_totp_qr(issuer: &str, username: &str, secret: &str) -> anyhow::Result<String> {
    let enc_issuer = url_encode_component(issuer);
    let enc_username = url_encode_component(username);
    let label = format!("{enc_issuer}:{enc_username}");
    let otpauth_url = format!(
        "otpauth://totp/{label}?secret={secret}&issuer={enc_issuer}&algorithm=SHA1&digits=6&period=30"
    );

    let code = qrcode::QrCode::new(otpauth_url.as_bytes())?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .build();
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{b64}"))
}
