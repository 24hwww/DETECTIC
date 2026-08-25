# Detectic Backend — nf-compute-10 Deployment Guide

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                    EX520 Sensor                             │
│                                                             │
│  detectic binary → GTPR poll → pseudonymize → HTTPS POST    │
│                                                             │
└─────────────────────────┬───────────────────────────────────┘
                          │
                    Internet (HTTPS)
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                 nf-compute-10 Server                         │
│                                                             │
│  0.1 vCPU / 256 MB RAM / 1024 MB storage                   │
│                                                             │
│  ┌─────────────────────────────────────────────────┐        │
│  │  nginx/caddy (TLS termination, port 443)        │        │
│  └──────────────────────┬──────────────────────────┘        │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────┐        │
│  │  Detectic Backend (Python, port 8080)            │        │
│  │  - HMAC-SHA256 authentication                    │        │
│  │  - SQLite WAL (bounded)                          │        │
│  │  - Rate limiting (30 req/s per IP)               │        │
│  │  - Thread pool (8 max)                           │        │
│  │  - Memory limit: 96 MB                           │        │
│  └──────────────────────┬──────────────────────────┘        │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────┐        │
│  │  SQLite: /opt/detectic/data/backend.db            │        │
│  │  - snapshots, detections, events, presence        │        │
│  │  - WAL mode, 8 MB cache, 64 MB mmap              │        │
│  └─────────────────────────────────────────────────┘        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Recursos del servidor

| Recurso | Límite | Uso estimado |
|---------|--------|-------------|
| vCPU | 0.1 (100 mCPU) | ~50 mCPU (polling idle) |
| RAM | 256 MB | ~50-80 MB (Python + SQLite) |
| Storage | 1024 MB | ~10 MB DB + 5 MB binarios |
| Threads | 8 max | Memory-bounded |
| Rate | 30 req/s/IP | Token bucket |

## Optimizaciones vs backend estándar

| Aspecto | Original | Optimizado |
|---------|----------|------------|
| Server | ThreadingHTTPServer (unbounded) | BoundedThreadingHTTPServer (8 threads) |
| SQLite | WAL default | WAL + 8 MB cache + 64 MB mmap |
| Rate limiting | None | Token bucket 30 req/s/IP |
| Memory limit | None | systemd MemoryMax=128M |
| Checkpoint | None | WAL checkpoint cada 5 min |
| Thread creation | Unbounded | Semaphore-bounded, 503 on overflow |
| Logging | Verbose | Errors only |
| JSON | Default separators | Compact (`,:`) |

## Instalación rápida (sin Docker)

```bash
# 1. Copiar al servidor
scp backend/server.py backend/deploy_nf10/install.sh user@nf-compute-10:/tmp/

# 2. SSH al servidor
ssh user@nf-compute-10

# 3. Ejecutar instalador
sudo bash /tmp/install.sh

# 4. El instalador imprime los secrets generados
#    COPIA EL SENSOR_SECRET — lo necesitas para el EX520

# 5. Iniciar
sudo systemctl start detectic-backend

# 6. Verificar
curl http://localhost:8080/api/v1/healthz
```

## Instalación con Docker Compose

```bash
# 1. Copiar al servidor
scp -r backend/deploy_nf10/* user@nf-compute-10:/opt/detectic/

# 2. SSH al servidor
ssh user@nf-compute-10
cd /opt/detectic

# 3. Generar secrets
MASTER_SECRET=$(openssl rand -hex 32)
SENSOR_SECRET=$(openssl rand -hex 16)

# 4. Crear .env
cat > .env <<EOF
DETECTIC_MASTER_SECRET=${MASTER_SECRET}
DETECTIC_SENSORS={"ex520-001":"${SENSOR_SECRET}"}
EOF

# 5. Iniciar
docker compose up -d

# 6. Verificar
curl http://localhost:8080/api/v1/healthz
```

## TLS con nginx (recomendado para producción)

```nginx
# /etc/nginx/sites-available/detectic
server {
    listen 443 ssl http2;
    server_name detectic.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/detectic.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/detectic.yourdomain.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000" always;
    add_header X-Content-Type-Options nosniff;

    # Proxy to backend
    location /api/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Rate limiting
        limit_req zone=detectic burst=20 nodelay;
    }

    # Health check (no auth)
    location /api/v1/healthz {
        proxy_pass http://127.0.0.1:8080;
    }
}

server {
    listen 80;
    server_name detectic.yourdomain.com;
    return 301 https://$host$request_uri;
}
```

```bash
# Rate limit zone
echo 'limit_req_zone $binary_remote_addr zone=detectic:1m rate=10r/s;' > /etc/nginx/conf.d/detectic.conf

# Enable site
ln -s /etc/nginx/sites-available/detectic /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

## Configuración del sensor EX520

En `detectic.env` del router:

```bash
# Backend URL (apunta al servidor remoto)
DETECTIC_UPLOAD_URL=https://detectic.yourdomain.com/api/v1/events

# Secret (debe coincidir con DETECTIC_SENSORS["ex520-001"] en el backend)
DETECTIC_SECRET=<el-secret-que-copiaste>

# Timeout más alto para latencia internet
DETECTIC_BACKEND_TIMEOUT=15
```

## API Endpoints

| Método | Ruta | Descripción | Auth |
|--------|------|-------------|------|
| `POST` | `/api/v1/events` | Ingestar snapshot | HMAC |
| `POST` | `/api/v1/events/batch` | Batch ingest (≤100) | HMAC |
| `GET` | `/api/v1/devices` | Historial de dispositivos | No |
| `GET` | `/api/v1/presence?hours=24` | Presencia (últimas N horas) | No |
| `GET` | `/api/v1/sensors` | Lista de sensores | No |
| `GET` | `/api/v1/stats` | Estadísticas globales | No |
| `GET` | `/api/v1/healthz` | Health + memory stats | No |
| `GET` | `/api/v1/readyz` | Readiness check | No |

## Autenticación

Cada request del sensor incluye:

```
X-Detectic-Sensor: ex520-001
X-Detectic-Signature: hex(HMAC-SHA256(sensor_secret, body))
```

El backend verifica que el `sensor_secret` coincida con el registrado para ese `sensor_id`.

## Monitoreo

```bash
# Health check
curl -s http://localhost:8080/api/v1/healthz | python3 -m json.tool

# Statistics
curl -s http://localhost:8080/api/v1/stats | python3 -m json.tool

# Presence (últimas 24h)
curl -s http://localhost:8080/api/v1/presence?hours=24 | python3 -m json.tool

# Systemd logs
journalctl -u detectic-backend -f

# DB size
ls -lh /opt/detectic/data/backend.db
```

## Mantenimiento

### Rotación automática de DB
El cron diario (`/etc/cron.daily/detectic-maintenance`) ejecuta:
- Verifica tamaño de DB (límite: 500 MB)
- Si excede, renombra a `backend.db.YYYYMMDD.bak`
- Ejecuta `VACUUM` para reclaimar espacio

### Backup manual
```bash
sqlite3 /opt/detectic/data/backend.db ".backup /opt/detectic/data/backup.db"
```

### Actualización del backend
```bash
systemctl stop detectic-backend
cp /tmp/server.py /opt/detectic/bin/server.py
systemctl start detectic-backend
```

## Troubleshooting

| Problema | Causa | Solución |
|----------|-------|---------|
| `401 unauthorized` | Secret no coincide | Verificar `DETECTIC_SECRET` en EX520 == `DETECTIC_SENSORS["ex520-001"]` en backend |
| `503 service unavailable` | Too many threads | Aumentar `--max-threads` o reducir `DETECTIC_INTERVAL` |
| `429 rate limit exceeded` | Demasiados requests | Aumentar `--rate-burst` o reducir frecuencia |
| DB crece mucho | Sin vacuum | Verificar cron de mantenimiento |
| Backend no arranca | Puerto ocupado | `ss -tlnp | grep 8080` |
| Memory leak | SQLite cache | Verificar `PRAGMA cache_size` en logs |
