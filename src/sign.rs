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

fn derive_key(secret: &str) -> [u8; 32] {
    Sha256::digest(format!("{}{}", secret, SIGN_SALT).as_bytes()).into()
}

/// Sign a path with expiration (5 minutes by default) and user context.
pub fn sign_path(secret: &str, path: &str, context: &str) -> Result<String> {
    if secret.trim().is_empty() {
        return Err(anyhow!("signing token is missing"));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let expires = now + DEFAULT_LIFETIME_SECS;
    Ok(sign_path_with_expire(secret, path, context, expires))
}

/// Sign a path with explicit expiration timestamp and user context.
pub fn sign_path_with_expire(secret: &str, path: &str, context: &str, expires: i64) -> String {
    let key = derive_key(secret);
    let mut mac = HmacSha256::new_from_slice(&key).expect("valid hmac key");
    let payload = if context.is_empty() {
        format!("{}:{}", path, expires)
    } else {
        format!("{}:{}:{}", path, context, expires)
    };
    mac.update(payload.as_bytes());
    let hash = mac.finalize().into_bytes();
    let b64 = BASE64_URL.encode(hash);
    format!("{}:{}", b64, expires)
}

/// Verify signature for a given path and user context.
pub fn verify_sign(secret: &str, path: &str, context: &str, sign: &str) -> Result<()> {
    if secret.trim().is_empty() {
        return Err(anyhow!("signing token is missing"));
    }
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

    let expected = sign_path_with_expire(secret, path, context, expires);
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
        let secret = "rulist-abcdef123456";
        let path = "/Local/test.mp4";
        let ctx = "uid=1:pwd_ts=123:root=/tmp/local";
        let s = sign_path(secret, path, ctx).unwrap();

        assert!(verify_sign(secret, path, ctx, &s).is_ok());
        assert!(verify_sign(secret, "/Local/other.mp4", ctx, &s).is_err());
        assert!(verify_sign("wrong_secret", path, ctx, &s).is_err());
        assert!(verify_sign(secret, path, "uid=2:pwd_ts=123:root=/tmp/local", &s).is_err());
        assert!(verify_sign(secret, path, "uid=1:pwd_ts=124:root=/tmp/local", &s).is_err());
        assert!(verify_sign(secret, path, "uid=1:pwd_ts=123:root=/tmp/other", &s).is_err());

        // Expired signature
        let expired_sign = sign_path_with_expire(secret, path, ctx, 1000);
        assert!(verify_sign(secret, path, ctx, &expired_sign).is_err());
    }
}
