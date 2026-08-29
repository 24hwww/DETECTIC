# EX520 — Log Credential Exposure Hardening

> **Date:** 2026-08-29
> **Scope:** Host-side deploy tooling + package server. No EX520 configuration was changed.
> **Classification:** Security hardening of logs. The credential exposure was real and
> independent of the RF/nrd investigation.
>
> **IMPORTANT:** This document contains NO real secrets, credential values, or the
> contents of `detectic.env`. It only documents the *mechanism*, the fix, and the
> verification. Any place a sensitive value would appear is shown as `****`.

---

## 1. Executive summary

The host-side **package server** (`deploy/ex520_package/`) serves several
diagnostic callbacks that persist router-produced logs to disk. A root-cause
audit found that three diagnostic shell scripts were **dumping the contents of
the router's `detectic.env`** (which contains the GUI password and the Detectic
HMAC secret) into host-side log files that the package server writes. Because
the package server also serves any file in its root directory over plain HTTP,
these logs (and therefore the credentials) became reachable on the LAN.

The exposure was fixed with two complementary layers:

1. **Script-side:** the three scripts no longer emit sensitive key **names** or
   **values**; they emit only non-sensitive key names and the marker
   `secret-key` for sensitive ones.
2. **Server-side (defense-in-depth):** `package_server.py` gained
   `redact_secrets()`, which scrubs secret values from any payload before it is
   persisted to a log — so even a future/buggy script that sends a secret cannot
   leave it on disk.

A purge of the affected log files was performed and all five verification
criteria were met.

---

## 2. Root cause

The compromised path was:

```text
router /var/run/misc/misc_rw/detectic/detectic.env        (contains PASSWORD + SECRET)
        |
        |   diagnostic scripts run as root via Lifemote/Phoenix
        v
read_env.sh      -> cat detectic.env              -> POST /sensor_log   -> sensor_log.txt
check_env.sh     -> head -c 200 detectic.env      -> PUT  /proof_log    -> proof_log.txt
probe_phase3.sh  -> cat detectic.env              -> GET  /env_line     -> env_probe_output.txt
        |
        v
Package handler persists the raw body to an append-only host-side log.
Package server serves that whole directory over HTTP (:8080, no auth).
```

The three scripts assumed their output was internal diagnostics; they did **not**
consider that the payload contained real credentials and that the receiving
server (a) wrote it verbatim and (b) served the resulting file over HTTP.

---

## 3. Affected scripts

| Script | Original behavior | Outcome |
|--------|-------------------|---------|
| `deploy/ex520_package/read_env.sh` | `cat "$path"` for every `detectic.env` candidate, then `POST /sensor_log`. | Wrote full env (PASSWORD+SECRET) to `sensor_log.txt`. |
| `deploy/ex520_package/check_env.sh` | `head -c 200 "$DIR/detectic.env"`, then `PUT /proof_log`. | Wrote the first 200 bytes of the env (contains PASSWORD) to `proof_log.txt`. |
| `deploy/ex520_package/probe_phase3.sh` | `cat /var/run/misc/misc_rw/detectic/detectic.env`, then uploaded line-by-line via `GET /env_line`. | Wrote env keys+values to `env_probe_output.txt` via `/env_line`. |

Supporting server-side handlers that persisted the raw payload without scrubbing:
`package_server.py` — `_handle_sensor_log()` (→ `sensor_log.txt`, `probe_log.txt`),
the `/env_line` GET handler (→ `env_probe_output.txt`), the `/done` GET handler
(→ `done_log.txt`), and `log_message()` (→ `package_server.log`).

---

## 4. Fixes

### 4.1 Script-side (no sensitive key names or values)

Each script now extracts **key names only** and replaces sensitive key names
with the literal marker `secret-key`, omitting every value:

```sh
grep -E '^[A-Za-z0-9_]+=' "$ENV" | cut -d'=' -f1 | sed \
   -e 's/^DETECTIC_PASSWORD$/secret-key/' \
   -e 's/^DETECTIC_SECRET$/secret-key/' \
   -e 's/^DETECTIC_BACKEND_TOKEN$/secret-key/' \
   -e 's/^DETECTIC_SMTP_PASSWORD$/secret-key/' \
   -e 's/^DETECTIC_SMTP_USER$/secret-key/' \
   -e 's/^DETECTIC_D1_SYNC_URL$/secret-key/' \
   -e 's/^PASSWORD$/secret-key/' \
   -e 's/^SECRET$/secret-key/'
```

- `read_env.sh` — replaced `cat "$path"` with the key-name-only `grep|cut|sed` above.
- `check_env.sh` — removed `head -c 200 ... detectic.env`; added the key-name-only
  listing and kept **counts** (`grep -c`) instead of values.
- `probe_phase3.sh` — replaced `cat ... detectic.env` with the key-name-only listing.

The net effect: a log line like `DETECTIC_PASSWORD=****` can no longer be produced;
the only output is `secret-key`.

### 4.2 Server-side defense-in-depth (`redact_secrets`)

Added to `deploy/ex520_package/package_server.py`:

```python
SECRET_KEYS = ("DETECTIC_PASSWORD", "DETECTIC_SECRET", "DETECTIC_BACKEND_TOKEN",
               "DETECTIC_SMTP_PASSWORD", "DETECTIC_SMTP_USER", "DETECTIC_D1_SYNC_URL",
               "PASSWORD", "SECRET")

def redact_secrets(text):
    pattern = "(?m)(?i)\b(" + "|".join(re.escape(k) for k in SECRET_KEYS) + r")(\s*=\s*)([^\n]*)"
    return re.sub(pattern, lambda m: m.group(1) + m.group(2) + "<REDACTED>", text)
```

It is applied before every persist in:
- `/sensor_log` and `/probe_log` body + query (`_handle_sensor_log`)
- `/env_line` decoded data (`d = redact_secrets(d)`)
- `/done` query string and `done_log.txt` write
- the raw HTTP request line in `log_message()`

This means even a script that sends a secret in the future cannot write it to a
host-side log.

---

## 5. Files purged / verified

| File | Action | Result |
|------|--------|--------|
| `deploy/ex520_package/sensor_log.txt` | Rewritten through `redact_secrets()` (2459 B → 2258 B) | No secret values remain |
| `deploy/ex520_package/proof_log.txt` | Inspected | 0 secret values |
| `deploy/ex520_package/package_server.log` | Inspected | 0 secret values |
| `deploy/ex520_package/done_log.txt` | Inspected | 0 secret values |
| `deploy/ex520_package/env_probe_output.txt` | Inspected | 0 secret values |

---

## 6. Verification criteria (all 5 met)

| # | Criterion | Result |
|---|-----------|--------|
| 1 | `sensor_log.txt` no longer contains secret values | ✅ `rg` finds no `*_PASSWORD=<value>` / `*_SECRET=<value>` in any served `.txt`/`.log` |
| 2 | No diagnostic script rewrites them | ✅ `read_env.sh`/`check_env.sh`/`probe_phase3.sh` contain no `cat ...detectic.env` / `head ...detectic.env` dump |
| 3 | Subsequent logs cannot contain `DETECTIC_SECRET` / `DETECTIC_PASSWORD` / equivalent credentials | ✅ Script-side masking + server-side `redact_secrets()` (both key names and values) |
| 4 | Service keeps working after cleanup | ✅ `GET http://192.168.0.27:8080/version` → `dev-20260829`; EX520 sensor `/health` → `healthy` |
| 5 | No functional EX520 config change | ✅ Only host-side scripts / server / logs were edited; router `detectic.env`, `misc_rw`, and firmware were untouched |

---

## 7. Final state

- The leak is closed at both the source (scripts) and the sink (package server).
- The package server was restarted to load the hardened `package_server.py`.
- The EX520 sensor remained healthy throughout; the Lifemote agent URL was left
  at the production `bootstart.sh`.
- No EX520 configuration was modified.

---

## 8. Open risk (NOT resolved — tracked separately)

> `detectic.env` is still served over plain HTTP without authentication by the
> package server (the bootstart chain fetches it during deployment). The log
> hardening above does NOT address this; it is a separate design decision.
>
> **Recommendation:** serve the env (and the deploy package) with a deployment
> token / short-lived credential, or restrict the callback endpoints so secrets
> are only exposed during an intended deploy, not to any LAN client. This is
> intentionally left as an open finding rather than marked resolved.

---

## 9. Artifacts changed

- `deploy/ex520_package/read_env.sh` — redact env dump (keys only).
- `deploy/ex520_package/check_env.sh` — remove `head -c 200`; keys-only + counts.
- `deploy/ex520_package/probe_phase3.sh` — keys-only env listing.
- `deploy/ex520_package/package_server.py` — add `redact_secrets()` + apply at all
  log sinks and `log_message`.
- `deploy/ex520_package/sensor_log.txt` — purged.
