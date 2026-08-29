//! Transport layer — GDPR/GTPR encrypted HTTP.
//!
//! Owns the session state (RSA params, AES key/iv, JSESSIONID, TokenID) and
//! exposes raw `gl`/`go` operations. No knowledge of OID semantics or
//! `NetworkMap` — that lives in `collector`.

use crate::crypto::*;
use base64::Engine;
use rand::Rng;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Error & dialect
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum GtprError {
    Http(String),
    Crypto(String),
    Protocol(String),
}

impl From<std::io::Error> for GtprError {
    fn from(e: std::io::Error) -> Self {
        GtprError::Http(e.to_string())
    }
}

impl std::fmt::Display for GtprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GtprError::Http(e) => write!(f, "http error: {}", e),
            GtprError::Crypto(s) => write!(f, "crypto error: {}", s),
            GtprError::Protocol(s) => write!(f, "protocol error: {}", s),
        }
    }
}

impl std::error::Error for GtprError {}

// ---------------------------------------------------------------------------
// Debug logging (never emits passwords, key/iv, MACs, IPs, or session secrets)
// ---------------------------------------------------------------------------

fn log_response(endpoint: &str, resp: &crate::http::HttpResponse) -> Result<(), GtprError> {
    let status = resp.status;
    let content_type = resp
        .headers
        .get("Content-Type")
        .map(|s| s.as_str())
        .unwrap_or("");
    let has_cookie = resp.headers.contains_key("Set-Cookie");
    eprintln!(
        "[DEBUG {}] status={} content-type={} set-cookie-present={}",
        endpoint, status, content_type, has_cookie
    );
    Ok(())
}

fn log_body(endpoint: &str, body: &str) {
    let ct = body.chars().count().min(120);
    eprintln!(
        "[DEBUG {}] body_len={} body_prefix={:?}",
        endpoint,
        body.len(),
        &body[..body
            .char_indices()
            .nth(ct)
            .map(|(i, _)| i)
            .unwrap_or(body.len())]
    );
}

/// Firmware dialect: controls the login payload shape and the `sign` encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// GDPR-JSON (EX220-confirmed, EX-series family). `sign` is hex-encoded.
    GdprJson,
    /// Text-style login; `sign` is base64-encoded.
    GdprText,
}

impl Dialect {
    pub(crate) fn sign_encoding(self) -> SignEncoding {
        match self {
            Dialect::GdprJson => SignEncoding::Hex,
            Dialect::GdprText => SignEncoding::Base64,
        }
    }

    pub(crate) fn login_payload(self, username: &str, password: &str) -> String {
        match self {
            Dialect::GdprJson => {
                let u = base64::engine::general_purpose::STANDARD.encode(username.as_bytes());
                let p = base64::engine::general_purpose::STANDARD.encode(password.as_bytes());
                format!(
                    "{{\"data\":{{\"UserName\":\"{}\",\"Passwd\":\"{}\",\"Action\":\"1\",\
                     \"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}},\
                     \"operation\":\"cgi\",\"oid\":\"/cgi/login\"}}",
                    u, p
                )
            }
            Dialect::GdprText => format!("{}\n{}", username, password),
        }
    }
}

// ---------------------------------------------------------------------------
// Wire structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GdprParm {
    nn: String,
    ee: String,
    seq: u64,
}

/// Parse the /cgi/getGDPRParm response.
///
/// Newer/family firmware returns a JSON object, but the EX520V extracted rootfs
/// (`_rootfs/web/frame/login.htm` and `httpd` strings) returns a JavaScript
/// snippet that must be `eval`'d by the browser, e.g.:
///
/// ```text
/// var adminSetting=0;
/// var userSetting=2;
/// var logoUrl="";
/// var ee="010001";
/// var nn="...";
/// var seq=1;
/// ```
///
/// This helper tries JSON first, then falls back to simple JS assignment parsing.
fn parse_gdpr_parm(text: &str) -> Result<GdprParm, GtprError> {
    let text = text.trim();

    // Fast path: plain JSON
    if text.starts_with('{') {
        if let Ok(p) = serde_json::from_str::<GdprParm>(text) {
            return Ok(p);
        }
    }

    // Fallback: JavaScript `var key = value;` assignments
    fn js_value(text: &str, key: &str) -> Option<String> {
        let pattern = format!("var {}=", key);
        let start = text.find(&pattern)? + pattern.len();
        let end = text[start..].find(';')? + start;
        let raw = text[start..end].trim();
        // Strip surrounding quotes
        if (raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\''))
        {
            Some(raw[1..raw.len() - 1].to_string())
        } else {
            Some(raw.to_string())
        }
    }

    let nn = js_value(text, "nn")
        .ok_or_else(|| GtprError::Protocol("getGDPRParm: nn not found in JS response".into()))?;
    let ee = js_value(text, "ee")
        .ok_or_else(|| GtprError::Protocol("getGDPRParm: ee not found in JS response".into()))?;
    let seq = js_value(text, "seq")
        .ok_or_else(|| GtprError::Protocol("getGDPRParm: seq not found in JS response".into()))?;
    let seq = seq
        .parse::<u64>()
        .map_err(|e| GtprError::Protocol(format!("getGDPRParm: seq is not a number: {}", e)))?;

    Ok(GdprParm { nn, ee, seq })
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    #[serde(rename = "sessionKey", default)]
    session_key: Option<String>,
    #[serde(rename = "sessionIv", default)]
    session_iv: Option<String>,
    #[serde(rename = "key", default)]
    key: Option<String>,
    #[serde(rename = "iv", default)]
    iv: Option<String>,
}

// ---------------------------------------------------------------------------
// Transport trait — the abstraction boundary for collector
// ---------------------------------------------------------------------------

/// Minimal transport contract: fetch a decrypted `gl` response for an OID.
/// The collector depends only on this, not on `GtprClient`.
pub trait Transport {
    fn gl(&self, oid: &str) -> Result<String, GtprError>;
}

// ---------------------------------------------------------------------------
// GtprClient — concrete GDPR transport
// ---------------------------------------------------------------------------

pub struct GtprClient {
    base_url: String,
    username: String,
    password: String,
    http: crate::http::HttpClient,
    dialect: Dialect,

    rsa_n: Vec<u8>,
    rsa_e: Vec<u8>,
    seq: u64,
    session_key: Vec<u8>,
    session_iv: Vec<u8>,
    jsessionid: String,
    token: String,
}

impl GtprClient {
    pub fn new(base_url: &str, username: &str, password: &str) -> Self {
        Self::with_dialect(base_url, username, password, Dialect::GdprJson)
    }

    pub fn with_dialect(base_url: &str, username: &str, password: &str, dialect: Dialect) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: password.to_string(),
            http: crate::http::HttpClient::new(
                std::time::Duration::from_secs(10),
                std::time::Duration::from_secs(30),
            ),
            dialect,
            rsa_n: Vec::new(),
            rsa_e: Vec::new(),
            seq: 0,
            session_key: vec![0u8; 16],
            session_iv: vec![0u8; 16],
            jsessionid: String::new(),
            token: String::new(),
        }
    }

    /// Step 1-3: obtain RSA params, log in, and fetch the TokenID.
    pub fn connect(&mut self) -> Result<(), GtprError> {
        let resp = self
            .http
            .post(
                &format!("{}/cgi/getGDPRParm", self.base_url),
                &[
                    ("Referer", &format!("{}/", self.base_url)),
                    ("Origin", &self.base_url),
                    ("Accept", "*/*"),
                ],
                "",
            )
            .map_err(|e| GtprError::Http(e))?;

        log_response("getGDPRParm", &resp)?;
        let body = resp.body;

        let parm = parse_gdpr_parm(&body)
            .map_err(|e| GtprError::Protocol(format!("getGDPRParm parse: {}", e)))?;

        self.rsa_n = hex::decode(&parm.nn).map_err(|e| GtprError::Crypto(e.to_string()))?;
        self.rsa_e = hex::decode(&parm.ee).map_err(|e| GtprError::Crypto(e.to_string()))?;
        self.seq = parm.seq;

        self.login()?;
        self.fetch_token()?;
        Ok(())
    }

    fn login(&mut self) -> Result<(), GtprError> {
        let (lk, liv) = gen_login_aes_pair();
        self.session_key = lk.clone();
        self.session_iv = liv.clone();

        let auth_h = auth_hash(&self.username, &self.password);
        let payload = self.dialect.login_payload(&self.username, &self.password);
        let ct = aes128_cbc_encrypt(&lk, &liv, payload.as_bytes());
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(ct);

        let key_str = String::from_utf8(lk.clone()).unwrap_or_default();
        let iv_str = String::from_utf8(liv.clone()).unwrap_or_default();

        // The live EX520V browser reference uses a 512-bit RSA modulus and
        // "nopadding" mode with 64-byte chunking. The login signed message
        // MUST include key/iv so the server can decrypt the `data` field.
        let sign = build_sign(
            &self.rsa_n,
            &self.rsa_e,
            &auth_h,
            self.seq,
            data_b64.len(),
            Some((&key_str, &iv_str)),
            self.dialect.sign_encoding(),
        );
        let body = build_body(&sign, &data_b64);

        eprintln!(
            "[DEBUG login] sign_len={} data_b64_len={} chunks={}",
            sign.len(),
            data_b64.len(),
            (sign.len() / 2 + self.rsa_n.len() - 1) / self.rsa_n.len()
        );

        let resp = self
            .http
            .post(
                &format!("{}/cgi_gdpr?9", self.base_url),
                &[
                    ("Content-Type", "text/plain"),
                    ("Referer", &format!("{}/", self.base_url)),
                    ("Origin", &self.base_url),
                    ("Accept", "*/*"),
                    ("X-Requested-With", "XMLHttpRequest"),
                ],
                &body,
            )
            .map_err(|e| GtprError::Http(e))?;

        log_response("login", &resp)?;

        let set_cookie = resp.headers.get("Set-Cookie").cloned().unwrap_or_default();
        if let Some(idx) = set_cookie.find("JSESSIONID=") {
            let rest = &set_cookie[idx + "JSESSIONID=".len()..];
            self.jsessionid = rest.split(';').next().unwrap_or("").to_string();
        }

        let text = resp.body;
        log_body("login", &text);

        // Try to decrypt and inspect the login response. On success it is
        // `$.ret=0;`; on failure it is an error code such as `$.ret=71233;`.
        match decode_response(&self.session_key, &self.session_iv, &text) {
            Ok(plain) => {
                eprintln!("[DEBUG login] decrypted={:?}", plain.trim_end_matches('\0'));
                if !plain.contains("$.ret=0") {
                    return Err(GtprError::Protocol(format!(
                        "login failed: {}",
                        plain.trim_end_matches('\0').trim()
                    )));
                }
            }
            Err(e) => {
                eprintln!("[DEBUG login] decrypt_error={}", e);
            }
        }

        if self.jsessionid.is_empty() {
            return Err(GtprError::Protocol(
                "login refused: no JSESSIONID in response".into(),
            ));
        }

        if let Ok(lr) = serde_json::from_str::<LoginResponse>(&text) {
            if let (Some(k), Some(iv)) = (lr.key.or(lr.session_key), lr.iv.or(lr.session_iv)) {
                if let (Ok(kb), Ok(ivb)) = (hex::decode(&k), hex::decode(&iv)) {
                    if kb.len() == 16 && ivb.len() == 16 {
                        self.session_key = kb;
                        self.session_iv = ivb;
                    }
                }
            }
        }
        Ok(())
    }

    fn fetch_token(&mut self) -> Result<(), GtprError> {
        let resp = self
            .http
            .get(
                &self.base_url,
                &[
                    ("Cookie", &format!("JSESSIONID={}", self.jsessionid)),
                    ("Referer", &format!("{}/", self.base_url)),
                    ("Origin", &self.base_url),
                    ("Accept", "*/*"),
                ],
            )
            .map_err(|e| GtprError::Http(e))?;

        log_response("fetch_token", &resp)?;
        let html = resp.body;

        if let Some(idx) = html.find("var token=\"") {
            let rest = &html[idx + "var token=\"".len()..];
            self.token = rest.split('"').next().unwrap_or("").to_string();
        }

        if self.token.is_empty() {
            // Some sessions do not publish a token in the HTML. Generate a
            // client-side 32-hex token matching the observed browser format.
            let mut rng = rand::thread_rng();
            self.token = (0..32)
                .map(|_| format!("{:x}", rng.gen_range(0..16)))
                .collect();
        }

        eprintln!("[DEBUG fetch_token] token_len={}", self.token.len());
        Ok(())
    }

    /// Perform an encrypted `gl` (get-list) operation and return the decrypted JSON.
    pub fn gl(&self, oid: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{{\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}},\"operation\":\"gl\",\"oid\":\"{}\"}}\r\n",
            oid
        );
        self.operation(&raw)
    }

    /// Perform an encrypted `go` (get-single) operation and return the decrypted JSON.
    pub fn go(&self, oid: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{{\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}},\"operation\":\"go\",\"oid\":\"{}\"}}\r\n",
            oid
        );
        self.operation(&raw)
    }

    /// Perform an encrypted `so` (set-object) operation.
    /// `fields_json` is the JSON object body for the data field (e.g. `{"telnetLocalEnabled":1}`).
    pub fn so(&self, oid: &str, fields_json: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{},\"operation\":\"so\",\"oid\":\"{}\"}}\r\n",
            fields_json, oid
        );
        self.operation(&raw)
    }

    /// Perform an encrypted `op` (action/operation) call.
    pub fn op(&self, oid: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{{\"stack\":\"0,0,0,0,0,0\",\"pstack\":\"0,0,0,0,0,0\"}},\"operation\":\"op\",\"oid\":\"{}\"}}\r\n",
            oid
        );
        self.operation(&raw)
    }

    /// Perform an encrypted `op` with a caller-supplied data payload, matching
    /// the vendor web UI's `$.dm.op({oid, data:{...}})` wire format exactly
    /// (e.g. ACT_PPP_CONN / ACT_PPP_DISCONN with the connection stack).
    pub fn op_with_data(&self, oid: &str, data_json: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{},\"operation\":\"op\",\"oid\":\"{}\"}}\r\n",
            data_json, oid
        );
        self.operation(&raw)
    }

    /// Perform an encrypted `cgi` operation (e.g. `/cgi/auth`, `/cgi/setPwd`).
    /// `fields_json` is the JSON object body for the data field.
    pub fn cgi(&self, oid: &str, fields_json: &str) -> Result<String, GtprError> {
        let raw = format!(
            "{{\"data\":{},\"operation\":\"cgi\",\"oid\":\"{}\"}}\r\n",
            fields_json, oid
        );
        self.operation(&raw)
    }

    fn operation(&self, raw_json: &str) -> Result<String, GtprError> {
        let ct = aes128_cbc_encrypt(&self.session_key, &self.session_iv, raw_json.as_bytes());
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(ct);
        let auth_h = auth_hash(&self.username, &self.password);
        let sign = build_sign(
            &self.rsa_n,
            &self.rsa_e,
            &auth_h,
            self.seq,
            data_b64.len(),
            None,
            self.dialect.sign_encoding(),
        );
        let body = build_body(&sign, &data_b64);

        eprintln!(
            "[DEBUG gl] sign_len={} data_b64_len={} token_len={} jsessionid_len={}",
            sign.len(),
            data_b64.len(),
            self.token.len(),
            self.jsessionid.len()
        );

        let resp = self
            .http
            .post(
                &format!("{}/cgi_gdpr?9", self.base_url),
                &[
                    ("Content-Type", "text/plain"),
                    ("TokenID", &self.token),
                    ("Cookie", &format!("JSESSIONID={}", self.jsessionid)),
                    ("Referer", &format!("{}/", self.base_url)),
                    ("Origin", &self.base_url),
                    ("Accept", "*/*"),
                    ("X-Requested-With", "XMLHttpRequest"),
                ],
                &body,
            )
            .map_err(|e| GtprError::Http(e))?;

        log_response("gl", &resp)?;
        let text = resp.body;
        log_body("gl", &text);
        // Some so/cgi replies are bare numeric status lines with no encrypted body.
        if text.trim().is_empty() {
            return Ok(text);
        }
        decode_response(&self.session_key, &self.session_iv, &text).map_err(GtprError::Crypto)
    }

    /// Convenience: collect the full network map via the collector layer.
    /// Delegates to `crate::collector::collect` so the transport stays thin.
    pub fn network_map(&self) -> Result<crate::model::NetworkMap, GtprError> {
        crate::collector::collect(self)
    }
}

impl Transport for GtprClient {
    fn gl(&self, oid: &str) -> Result<String, GtprError> {
        GtprClient::gl(self, oid)
    }
}
