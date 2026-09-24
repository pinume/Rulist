use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

const SIGN_SALT: &str = ":rulist-link-signer";
const DEFAULT_LIFETIME_SECS: i64 = 300; // 5 minutes

fn derive_key(token: &str) -> [u8; 32] {
    Sha256::digest(format!("{}{}", token, SIGN_SALT).as_bytes()).into()
}

/// Sign a path with expiration (5 minutes by default)
pub fn sign_path(token: &str, path: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let expires = now + DEFAULT_LIFETIME_SECS;
    sign_path_with_expire(token, path, expires)
}

/// Sign path with explicit expiration timestamp
pub fn sign_path_with_expire(token: &str, path: &str, expires: i64) -> String {
    let key = derive_key(token);
    let mut mac = HmacSha256::new_from_slice(&key).expect("valid hmac key");
    let payload = format!("{}:{}", path, expires);
    mac.update(payload.as_bytes());
    let hash = mac.finalize().into_bytes();
    let b64 = BASE64_URL.encode(hash);
    format!("{}:{}", b64, expires)
}

/// Verify signature for a given path
pub fn verify_sign(token: &str, path: &str, sign: &str) -> Result<()> {
    let sep = sign
        .rfind(':')
        .ok_or_else(|| anyhow!("expire parameter missing from sign"))?;

    let exp_str = &sign[sep + 1..];
    let expires: i64 = exp_str
        .parse()
        .map_err(|_| anyhow!("invalid expire timestamp in sign"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if expires != 0 && expires < now {
        return Err(anyhow!("signature expired"));
    }

    let expected = sign_path_with_expire(token, path, expires);
    if sign.as_bytes().ct_eq(expected.as_bytes()).into() {
        Ok(())
    } else {
        Err(anyhow!("invalid signature"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let token = "rulist-abcdef123456";
        let path = "/Local/test.mp4";
        let s = sign_path(token, path);

        assert!(verify_sign(token, path, &s).is_ok());
        assert!(verify_sign(token, "/Local/other.mp4", &s).is_err());
        assert!(verify_sign("wrong_token", path, &s).is_err());

        // Expired signature
        let expired_sign = sign_path_with_expire(token, path, 1000);
        assert!(verify_sign(token, path, &expired_sign).is_err());
    }
}
