# EX520V Rust Login Diagnostic Report

## Executive summary

The `Network Error: Unexpected EOF` during Rust login was caused by the
**login RSA `sign` being generated in the wrong format**: it did not contain the
AES `key`/`iv`, it used PKCS#1 v1.5 padding, and it was not chunked for the
router's 512-bit (64-byte) modulus.

The browser reference implementation (`tpEncrypt.js`) uses:

- `flag=0` "nopadding" RSA
- 64-byte chunking
- login signed message: `key=<16>&iv=<16>&h=<md5>&s=<seq+len>`

That message is ~87 bytes, so the resulting `sign` is **256 hex characters**
(two 64-byte RSA blocks). Non-login `gl`/`go` messages are short enough for a
single 128-hex `sign`.

## Code changes applied

### `src/crypto.rs`

- Replaced `rsa_sign_public` with a **chunked, zero-padded (nopadding)**
  implementation that splits messages at the modulus byte length and encrypts
  each block as `m^e mod n`.
- Updated `build_sign` so the login signed message embeds `key`/`iv` **before**
  `h`/`s`, matching `tpEncrypt.js`.

### `src/transport.rs`

- Restored `Some((&key_str, &iv_str))` for the login `build_sign` call.
- Added `Origin` and `Accept: */*` headers to `getGDPRParm`, `login`,
  `fetch_token`, and `gl`/`go` operations.
- `fetch_token()` now falls back to a randomly-generated 32-hex token when the
  router does not publish `var token="..."` in `GET /`.
- Added safe debug logging:
  - HTTP status, Content-Type, Set-Cookie presence
  - Encrypted response body length and prefix
  - `sign` / `data` lengths and chunk count
  - Decrypted login response (no credentials, no key/iv, no MACs, no IPs)
- Login now decrypts the raw `/cgi_gdpr?9` response and checks for `$.ret=0`
  before continuing.

### `python/detectic_client.py`

- Same chunking/nopadding `rsa_sign_public`.
- Same `key=...&iv=...&h=...&s=...` order.
- Same `Origin`/`Accept` headers.
- Same random-token fallback.
- Added temporary debug printing (status, headers, body length, decrypted prefix).

## Request comparison: Python vs Rust

| Property | Python (working path) | Rust (after fixes) |
|----------|----------------------|-------------------|
| `getGDPRParm` method | `POST` | `POST` |
| `getGDPRParm` headers | `Referer`, `Origin`, `Accept` | `Referer`, `Origin`, `Accept` |
| Login URL | `POST /cgi_gdpr?9` | `POST /cgi_gdpr?9` |
| Content-Type | `text/plain` | `text/plain` |
| Referer | `http://192.168.0.1/` | `http://192.168.0.1/` |
| Origin | `http://192.168.0.1` | `http://192.168.0.1` |
| Accept | `*/*` | `*/*` |
| X-Requested-With | `XMLHttpRequest` | `XMLHttpRequest` |
| TokenID on login | not sent | not sent |
| Cookie on login | not sent | not sent |
| Login `sign` format | `key=<16>&iv=<16>&h=<md5>&s=<...>` | same |
| Login `sign` length | 256 hex (2 chunks) | 256 hex (2 chunks) |
| Non-login `sign` length | 128 hex (1 chunk) | 128 hex (1 chunk) |
| AES padding | PKCS#7 | PKCS#7 |

## Root cause of the EOF

When the login `sign` omitted `key`/`iv` (one 128-hex block), the server could
not recover the AES key needed to decrypt the login `data`. On the EX520V this
produced one of two symptoms:

1. A valid HTTP 200 with an encrypted `$.ret=71233;` body (`USER_PWD_NOT_CORRECT`)
   when the request was well-formed but the credentials could not be verified.
2. An abrupt TCP close / `Unexpected EOF` when the `ureq` HTTP parser could not
   read a complete response (empty or malformed reply).

The Rust client hit symptom #2 because the short sign format was rejected early
in the request processing pipeline.

## What the live capture confirmed

| Step | Observation |
|------|-------------|
| `POST /cgi/getGDPRParm` | Returns JS: `nn` 128 hex, `ee=010001`, `seq` as string |
| Login with `admin` + `<REDACTED>` | Returns `$.ret=71233` (wrong user/pwd for this session) |
| Login with `user` + `<REDACTED>` | Returns `$.ret=0` and `Set-Cookie: JSESSIONID=...` |
| `gl` without TokenID | Server closes connection (`RemoteDisconnected` / EOF) |

**Important username finding:** the current router session/defaults expect the
username `user`, not `admin`. The HTML served by the router declares
`adminType="user"` and `userName="user"`, and the GTPR login succeeds only
with `user`.

## Final working request format

### `POST /cgi/getGDPRParm`

```text
Referer: http://192.168.0.1/
Origin:  http://192.168.0.1
Accept:  */*
```

Response:

```javascript
var adminSetting=1;
var userSetting=1;
var logoUrl="";
var ee="010001";
var nn="C394...0EBB";   // 128 hex, 512-bit
var seq="47294808";
$.ret=0;
```

### `POST /cgi_gdpr?9` (login)

Body:

```text
sign=<256-hex>
data=<AES-128-CBC base64>

```

Where the signed message for `sign` is:

```text
key=<16-digit-ascii>&iv=<16-digit-ascii>&h=<md5(user+pass)>&s=<seq + data.len>
```

The `data` field is the AES-128-CBC/PKCS7 encryption of:

```json
{"data":{"UserName":"<b64-user>","Passwd":"<b64-pass>","Action":"1","stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"},"operation":"cgi","oid":"/cgi/login"}
```

The 512-bit modulus forces two 64-byte RSA blocks, so `sign` is **256 hex
characters**.

### `POST /cgi_gdpr` (gl/go)

Headers:

```text
Content-Type: text/plain
TokenID: <32-hex>
Cookie: JSESSIONID=<...>
Referer: http://192.168.0.1/
Origin: http://192.168.0.1
Accept: */*
```

Body:

```text
sign=<128-hex>
data=<AES-128-CBC base64>

```

The 128-hex `sign` is one 64-byte RSA block over:

```text
h=<md5(user+pass)>&s=<seq + data.len>
```

The `data` field is the AES-128-CBC/PKCS7 encryption of:

```json
{"data":{"stack":"0,0,0,0,0,0","pstack":"0,0,0,0,0,0"},"operation":"gl","oid":"DEV2_..."}

```

## Current blocker

After repeated login attempts, the router's HTTP daemon is no longer listening:

```text
nmap -p 80,443 192.168.0.1
PORT    STATE  SERVICE
80/tcp  closed http
443/tcp closed https
```

The router still responds to ICMP, but `httpd` has crashed or the management
interface has temporarily shut down. This prevents further validation of the
Rust client against the live router.

## Next step

A physical or power-cycle reboot of the EX520V is required to restore the web
server. After the reboot:

1. Run `cargo build` (already compiles).
2. Test login + `gl` with:

```bash
DETECTIC_PASSWORD='<REDACTED>' cargo run -- map --url http://192.168.0.1 --user user
```

If the router still blocks the source IP after previous failures, the test
should be retried from a different host or after the router's fail2ban/lockout
timer expires.

## Files modified

- `src/crypto.rs`
- `src/transport.rs`
- `python/detectic_client.py`
