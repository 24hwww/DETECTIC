//! Cryptographic helpers for the TP-Link GTPR/GDPR protocol.
//!
//! The protocol requires every `go`/`gl` operation body to be:
//!   1. AES-128-CBC encrypted with the per-session key/iv (PKCS#7 padding)
//!   2. base64 encoded
//!   3. signed with the router's RSA public key (PKCS#1 v1.5 style, using the
//!      *public* key via `m^e mod n` over a PKCS#1 v1.5 *signature* padded block)
//!
//! Those details were reverse engineered from the EX520 firmware (see
//! `ex520-network-map-gdpr.md`).

use aes::Aes128;
use base64::Engine;
use block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};
use md5::{Digest, Md5};
use num_bigint::BigUint;

/// AES-128-CBC encrypt `plaintext` with `key`/`iv` (16 bytes each) using
/// PKCS#7 padding, returning raw ciphertext.
pub fn aes128_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Vec<u8> {
    assert_eq!(key.len(), 16);
    assert_eq!(iv.len(), 16);
    let mut buf = vec![0u8; plaintext.len() + 16];
    let pt = plaintext.to_vec();
    // cbc Encryptor requires exact block handling via the padding trait.
    let enc = Encryptor::<Aes128>::new_from_slices(key, iv).expect("valid key/iv");
    let ct = enc
        .encrypt_padded_b2b_mut::<Pkcs7>(&pt, &mut buf)
        .expect("encrypt");
    ct.to_vec()
}

/// AES-128-CBC decrypt `ciphertext` with `key`/`iv` (16 bytes each),
/// removing PKCS#7 padding.  Returns an error instead of panicking so callers
/// can fall back to treating the response as plaintext.
pub fn aes128_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != 16 || iv.len() != 16 {
        return Err("key/iv must be 16 bytes".into());
    }
    let dec = Decryptor::<Aes128>::new_from_slices(key, iv).map_err(|e| format!("key/iv: {e}"))?;
    let mut buf = vec![0u8; ciphertext.len() + 16];
    let pt = dec
        .decrypt_padded_b2b_mut::<Pkcs7>(ciphertext, &mut buf)
        .map_err(|_| "AES-CBC decrypt failed (wrong key/iv or bad padding)".to_string())?;
    Ok(pt.to_vec())
}

/// MD5 of `user` + `password`, returned as a lowercase hex string.
/// This is the `h` token used in the RSA signature.
pub fn md5_hex(input: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(input);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(32);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Convenience: `md5_hex((user + password).as_bytes())`.
pub fn auth_hash(user: &str, password: &str) -> String {
    let mut buf = Vec::with_capacity(user.len() + password.len());
    buf.extend_from_slice(user.as_bytes());
    buf.extend_from_slice(password.as_bytes());
    md5_hex(&buf)
}

/// TP-Link-style RSA "signature" with raw (nopadding) chunking.
///
/// The EX520V firmware uses a 512-bit (64-byte) modulus and `flag=0` in
/// `js/encrypt.js`, meaning the message is split into <=k-byte chunks, each
/// chunk is zero-padded to exactly k bytes, and each block is encrypted as
/// `m^e mod n`. The server decrypts, concatenates, and parses the resulting
/// null-terminated string.
///
/// For login, the signed message includes `key=<16>&iv=<16>&h=<md5>&s=<...>`,
/// which is ~87 bytes and therefore requires two 64-byte chunks.
/// Non-login operations (`gl`/`go`) use a shorter `h=<md5>&s=<...>` message
/// that fits in a single chunk.
pub fn rsa_sign_public(n: &[u8], e: &[u8], msg: &[u8]) -> Vec<u8> {
    let k = n.len(); // modulus byte length
    let nn = BigUint::from_bytes_be(n);
    let ee = BigUint::from_bytes_be(e);
    let mut out = Vec::new();

    for chunk in msg.chunks(k) {
        let mut block = Vec::with_capacity(k);
        block.extend_from_slice(chunk);
        block.extend(std::iter::repeat_n(0x00, k - chunk.len()));

        let m = BigUint::from_bytes_be(&block);
        let sig = m.modpow(&ee, &nn);
        let mut sig_bytes = sig.to_bytes_be();
        while sig_bytes.len() < k {
            sig_bytes.insert(0, 0x00);
        }
        out.extend(sig_bytes);
    }

    out
}

/// How the RSA `sign` is encoded for transport. The verified `@hertzg`
/// implementation uses hex; some firmware builds (e.g. the EX520 notes) use
/// base64. Expose it so it can be flipped per device without code changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignEncoding {
    Hex,
    Base64,
}

impl SignEncoding {
    fn encode(&self, bytes: &[u8]) -> String {
        match self {
            SignEncoding::Hex => {
                let mut s = String::with_capacity(bytes.len() * 2);
                for b in bytes {
                    s.push_str(&format!("{:02x}", b));
                }
                s
            }
            SignEncoding::Base64 => base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }
}

/// Generate the 16-byte AES key/IV used by TP-Link GDPR clients: the current
/// Unix time in milliseconds (13 digits) followed by 3 random base-10 digits.
/// Mirrors `generateKey()` in `@hertzg/tplink-api` (and the 0xf15h capture
/// analysis). Returns ASCII bytes (`key` and `iv` are independent).
pub fn gen_login_aes_pair() -> (Vec<u8>, Vec<u8>) {
    use rand::Rng;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut rng = rand::thread_rng();
    let r1: u32 = rng.gen_range(0..1000);
    let r2: u32 = rng.gen_range(0..1000);
    let key = format!("{}{:03}", ms, r1).into_bytes();
    let iv = format!("{}{:03}", ms, r2).into_bytes();
    (key, iv)
}

/// Build the RSA `sign` for an operation.
///
/// The signed payload is `h=<md5>&s=<seq+data.len()>`. For the **login**
/// operation the client must also embed its AES `key`/`iv` (both ASCII strings)
/// so the server can decrypt the request — `@hertzg/tplink-api` always passes
/// `key`/`iv` for login. `encoding` selects hex vs base64 transport.
pub fn build_sign(
    rsa_n: &[u8],
    rsa_e: &[u8],
    auth_h: &str,
    seq: u64,
    data_b64_len: usize,
    key_iv: Option<(&str, &str)>,
    encoding: SignEncoding,
) -> String {
    // Match tpEncrypt.js: for login the aesKeyString ("key=...&iv=...")
    // is the prefix of the signed message.
    let payload = if let Some((k, iv)) = key_iv {
        format!(
            "key={}&iv={}&h={}&s={}",
            k,
            iv,
            auth_h,
            seq + data_b64_len as u64
        )
    } else {
        format!("h={}&s={}", auth_h, seq + data_b64_len as u64)
    };
    let sig = rsa_sign_public(rsa_n, rsa_e, payload.as_bytes());
    encoding.encode(&sig)
}

/// Build the request body for an encrypted operation.
pub fn build_body(sign_b64: &str, data_b64: &str) -> String {
    format!("sign={}\r\ndata={}\r\n", sign_b64, data_b64)
}

/// Decode a (possibly chunked) base64 response: concatenate all base64 text
/// (ignoring whitespace/newlines), decode, and AES-CBC decrypt with session
/// key/iv.  Some `so`/`cgi` replies are plaintext (e.g. `$.ret=0;`); if
/// decryption fails, return the original text so the caller can inspect it.
pub fn decode_response(key: &[u8], iv: &[u8], base64_chunks: &str) -> Result<String, String> {
    let compact: String = base64_chunks
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let raw = match base64::engine::general_purpose::STANDARD.decode(compact.as_bytes()) {
        Ok(r) => r,
        Err(_) => return Ok(base64_chunks.to_string()),
    };
    match aes128_cbc_decrypt(key, iv, &raw) {
        Ok(pt) => Ok(String::from_utf8_lossy(&pt).to_string()),
        Err(_) => Ok(base64_chunks.to_string()),
    }
}

/// Encode an operation body and return its base64 (used both for login and for
/// `go`/`gl`).
pub fn encode_operation(key: &[u8], iv: &[u8], raw_json: &str) -> String {
    let pt = raw_json.as_bytes();
    let ct = aes128_cbc_encrypt(key, iv, pt);
    base64::engine::general_purpose::STANDARD.encode(ct)
}

/// Privacy layer: derive a stable, non-reversible device pseudonym from a raw
/// identifier (typically the MAC) using HMAC-SHA256 with a per-sensor secret.
/// Output is a 64-char hex string. Never log or transmit the raw MAC.
pub fn pseudonymize(secret: &[u8], identifier: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).expect("hmac key");
    mac.update(identifier.as_bytes());
    let result = mac.finalize().into_bytes();
    let mut s = String::with_capacity(64);
    for b in result {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// HMAC-SHA256 of `msg` keyed by `key`, returned as a hex string.
/// Used to authenticate sensor uploads to the Detectic backend (Milestone M5).
pub fn hmac_sha256_hex(key: &[u8], msg: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(msg);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes128_cbc_roundtrip() {
        let key = [0x01u8; 16];
        let iv = [0x02u8; 16];
        let plain = "hello detectic protocol";
        let ct = aes128_cbc_encrypt(&key, &iv, plain.as_bytes());
        assert_ne!(ct, plain.as_bytes());
        let pt = aes128_cbc_decrypt(&key, &iv, &ct).unwrap();
        assert_eq!(pt, plain.as_bytes());
    }

    #[test]
    fn md5_known_vector() {
        // MD5("") = d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        // MD5("abc") = 900150983cd24fb0d6963f7d28e17f72
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn pseudonymize_is_stable_and_secret_dependent() {
        let a = pseudonymize(b"secret", "AA:BB:CC:11:22:33");
        let b = pseudonymize(b"secret", "AA:BB:CC:11:22:33");
        let c = pseudonymize(b"other", "AA:BB:CC:11:22:33");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn rsa_sign_public_is_deterministic_and_fixed_size() {
        // 1024-bit modulus/exponent (exponent = 65537)
        let n = vec![0xFFu8; 128];
        let e = vec![0x01, 0x00, 0x01];
        let s1 = rsa_sign_public(&n, &e, b"h=x&s=123");
        let s2 = rsa_sign_public(&n, &e, b"h=x&s=123");
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 128);
    }

    #[test]
    fn hmac_sha256_hex_matches_rfc4231_vector() {
        // RFC 4231 test case 2: key="Jefe", data="what do ya want for nothing?"
        let sig = hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            sig,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_canonical_contract_v1_cross_language() {
        // Canonical HMAC Contract V1 — must match Python tests/hmac_contract.py
        // and Cloudflare Worker verifyAuth().
        // signed = "<timestamp>\n<body>", key = UTF-8 secret
        let secret = b"detectic-test-secret-v1-not-production";
        let body = r#"{"sensor_id":"test-sensor-001","devices":[{"pseudonym":"abc"}]}"#;
        let timestamp = 1700000000i64;
        let signed = format!("{}\n{}", timestamp, body);
        let sig = hmac_sha256_hex(secret, signed.as_bytes());
        // Must match the Python-computed EXPECTED_SIG
        assert_eq!(
            sig,
            "2c6e8db2c1d07111ea8525cb603416f037ce36a3bf234647bbf3f058db5b1be2"
        );
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;

    #[test]
    fn login_aes_pair_is_16_ascii_with_timestamp_prefix() {
        let (k, iv) = gen_login_aes_pair();
        assert_eq!(k.len(), 16);
        assert_eq!(iv.len(), 16);
        // First 13 chars must be a numeric (ms) timestamp; rest numeric random.
        let s = String::from_utf8(k.clone()).unwrap();
        assert!(
            s.chars().all(|c| c.is_ascii_digit()),
            "key must be digits: {}",
            s
        );
        assert_eq!(s.len(), 16);
        assert_ne!(k, iv, "key and iv should differ");
        // Key/IV survive a round-trip through build_sign (deterministic RSA).
        let sign = build_sign(
            &[0xFF; 128],
            &[0x01, 0x00, 0x01],
            "abc",
            7,
            10,
            Some((&s, &s)),
            SignEncoding::Hex,
        );
        assert!(!sign.is_empty());
    }
}
