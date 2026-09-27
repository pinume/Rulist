use rulist::auth::{ARGON2_PREFIX, base32_decode, compute_totp, hash_password, verify_password};

#[test]
fn argon2_hash_round_trip_and_rejects_wrong_password() {
    let password = "SuperSecretPassword123!";
    let encoded = hash_password(password);

    assert!(encoded.starts_with(ARGON2_PREFIX));
    assert!(verify_password(password, &encoded));
    assert!(!verify_password("wrong_password", &encoded));
}

#[test]
fn base32_decoder_matches_rfc4648_examples() {
    assert_eq!(base32_decode("MY======").unwrap(), b"f");
    assert_eq!(base32_decode("MZXQ====").unwrap(), b"fo");
    assert_eq!(base32_decode("MZXW6===").unwrap(), b"foo");
    assert_eq!(base32_decode("MZXW6YTB").unwrap(), b"fooba");
    assert_eq!(base32_decode("MZXW6YTBOI======").unwrap(), b"foobar");
    assert_eq!(base32_decode("mzxw6ytboi").unwrap(), b"foobar");
}

#[test]
fn totp_matches_rfc6238_sha1_vectors() {
    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    assert_eq!(compute_totp(secret, 59 / 30).unwrap(), "287082");
    assert_eq!(compute_totp(secret, 1_111_111_109 / 30).unwrap(), "081804");
    assert_eq!(compute_totp(secret, 1_111_111_111 / 30).unwrap(), "050471");
    assert_eq!(compute_totp(secret, 1_234_567_890 / 30).unwrap(), "005924");
    assert_eq!(compute_totp(secret, 2_000_000_000 / 30).unwrap(), "279037");
}
