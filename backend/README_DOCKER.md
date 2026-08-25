# Detectic backend — deployment notes

## Build

```bash
docker build -t detectic-backend ./backend
```

## Run (SQLite data on a volume)

```bash
docker run -d --name detectic-backend \
  -p 8080:8080 \
  -v detectic-data:/data \
  -e DETECTIC_SENSORS='{"ex520-001":"<sensor-secret>"}' \
  -e DETECTIC_MASTER_SECRET='<master-secret>' \
  --restart unless-stopped \
  detectic-backend
```

- `DETECTIC_SENSORS` — JSON map `<sensor_id> -> <secret>` (same secret as
  `DETECTIC_SECRET` on the router).
- `DETECTIC_MASTER_SECRET` — used to re-pseudonymize legacy identifiers.
  Generate with `openssl rand -hex 32`.

## HTTPS (production)

The container speaks plain HTTP on 8080. Put a TLS-terminating reverse proxy
in front:

```bash
# Caddy example
caddy reverse-proxy --from https://detectic.example.com --to 127.0.0.1:8080
```

Then set on the router:

```
DETECTIC_UPLOAD_URL=https://detectic.example.com/api/v1/events
```

The sensor's HMAC auth (`X-Detectic-Sensor` + `X-Detectic-Signature`) is
applied to the body before TLS, so the proxy adds transport security without
touching payload integrity.

### Why the router sends HTTP, not HTTPS

The on-router binary has a hard size budget: `misc_rw` + `misc_rw_bak` fit a
~1.32 MB split binary. Adding rustls/ring grows it to ~2.12 MB, which does not
fit. Two deployment options exist:

1. **Host/proxy bridge (recommended, default):** the router POSTs over HTTP to
   a host on the LAN (e.g. `http://192.168.0.27:8082`), and that host — or a
   reverse proxy — forwards to the HTTPS backend. The router keeps its spool
   + retry, so the bridge can be down without data loss.
2. **TLS in the router (experimental):** build with
   `cargo build --release --no-default-features --features tls --target aarch64-unknown-linux-musl`
   (needs `CC_aarch64_unknown_linux_musl=clang --target=aarch64-linux-musl`).
   Only viable if the storage budget grows (larger flash / different model).

## Endpoints

- `POST /api/v1/events` — ingest (HMAC-authenticated)
- `GET  /api/v1/devices` — device aggregates
- `GET  /api/v1/healthz` — liveness

## Push to GitHub

```bash
git init backend && cd backend
git add Dockerfile server.py sensors.json.example
git commit -m "feat: Detectic backend ingestion API container"
# create a repo on GitHub, then:
git remote add origin git@github.com:<user>/detectic-backend.git
git push -u origin main
```
