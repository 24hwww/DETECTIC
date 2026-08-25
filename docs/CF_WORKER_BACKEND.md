# Detectic Backend — Cloudflare Worker + D1

## Por qué Cloudflare Workers

| Aspecto | nf-compute-10 (VPS) | Cloudflare Workers |
|---------|--------------------|--------------------|
| Costo | ~$5-10/mes | **$0** (free tier) |
| TLS | Necesita nginx/caddy | **Incluido** |
| Mantenimiento | OS updates, backups | **Ninguno** |
| Escalabilidad | Limitada por RAM | **Automática** |
| Latencia global | 1 ubicación | **Edge global** |
| Storage | 1 GB (limitado) | **500 MB D1 free** |
| Requests | Limitado por CPU | **100K/día free** |
| Uptime | Depende del VPS | **99.99% SLA** |

## Arquitectura

```
┌─────────────────────────────────────────────────┐
│              EX520 Sensor                        │
│                                                  │
│  detectic binary → GTPR poll → pseudonymize      │
│                                                  │
│       ↓ HTTPS POST (cada 60s)                    │
│                                                  │
└──────────────────┬──────────────────────────────┘
                   │
              Internet (HTTPS)
                   │
                   ▼
┌─────────────────────────────────────────────────┐
│         Cloudflare Edge Network                  │
│                                                  │
│  ┌─────────────────────────────────────────┐    │
│  │  TLS termination (automático)            │    │
│  └────────────────────┬────────────────────┘    │
│                       │                          │
│  ┌────────────────────▼────────────────────┐    │
│  │  Detectic Worker (TypeScript)            │    │
│  │  - HMAC-SHA256 auth                      │    │
│  │  - Rate limiting (built-in)              │    │
│  │  - CORS headers                          │    │
│  │  - ~1 ms CPU por request                 │    │
│  └────────────────────┬────────────────────┘    │
│                       │                          │
│  ┌────────────────────▼────────────────────┐    │
│  │  D1 Database (SQLite cloud)              │    │
│  │  - snapshots, detections, events         │    │
│  │  - WAL, replication automática           │    │
│  │  - 500 MB free tier                      │    │
│  └─────────────────────────────────────────┘    │
│                                                  │
└─────────────────────────────────────────────────┘
```

## Step-by-step deployment

### 1. Instalar Wrangler CLI

```bash
npm install -g wrangler
wrangler login
```

### 2. Crear el Worker

```bash
cd backend/cf-worker
npm install
```

### 3. Crear D1 Database

```bash
npx wrangler d1 create detectic-db
```

Copia el `database_id` impreso y pégalo en `wrangler.toml`:

```toml
[[d1_databases]]
binding = "DB"
database_name = "detectic-db"
database_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

### 4. Inicializar schema

```bash
npx wrangler d1 execute detectic-db --file=schema.sql
```

### 5. Configurar secrets

```bash
# Generate secrets
MASTER_SECRET=$(openssl rand -hex 32)
SENSOR_SECRET=$(openssl rand -hex 16)

# Set secrets on Cloudflare
echo "$MASTER_SECRET" | npx wrangler secret put DETECTIC_MASTER_SECRET
echo "{\"ex520-001\":\"$SENSOR_SECRET\"}" | npx wrangler secret put DETECTIC_SENSORS

# SAVE THESE — you need them for the EX520!
echo "DETECTIC_MASTER_SECRET=$MASTER_SECRET"
echo "DETECTIC_SENSOR_SECRET=$SENSOR_SECRET"
```

### 6. Deploy

```bash
npx wrangler deploy
```

Output:
```
Uploaded detectic-backend (1.23 MB)
Published detectic-backend
  https://detectic-backend.your-subdomain.workers.dev
```

### 7. Custom domain (optional)

En Cloudflare Dashboard → Workers → detectic-backend → Settings → Triggers → Custom Domains:

```
detectic.yourdomain.com
```

### 8. Configurar el EX520

En `detectic.env`:

```bash
# Backend URL (tu Worker de Cloudflare)
DETECTIC_UPLOAD_URL=https://detectic-backend.your-subdomain.workers.dev/api/v1/events
# O con custom domain:
# DETECTIC_UPLOAD_URL=https://detectic.yourdomain.com/api/v1/events

# Secret (el que copiaste en paso 5)
DETECTIC_SECRET=<el-sensor-secret>

# Timeout (Workers responden rápido)
DETECTIC_BACKEND_TIMEOUT=10
```

## Testing

```bash
# Local development
npx wrangler dev --port 8787

# Test health
curl http://localhost:8787/api/v1/healthz

# Test ingest (con el secret correcto)
python3 backend/send_test.py \
  --url http://localhost:8787/api/v1/events \
  --secret <your-sensor-secret>

# Test en producción
curl https://detectic-backend.your-subdomain.workers.dev/api/v1/healthz
```

## Free Tier Limits

| Recurso | Free | Paid ($5/mo) |
|---------|------|-------------|
| Requests/day | 100,000 | 10,000,000 |
| CPU time/req | 10 ms | 30 ms |
| D1 rows read/day | 5,000,000 | 50,000,000 |
| D1 rows write/day | 100,000 | 25,000,000 |
| D1 storage | 500 MB | 10 GB |
| Worker size | 10 MB | 10 MB |

### ¿Alcanza para Detectic?

Con un sensor que envía 1 snapshot/minuto con ~5 dispositivos:

```
Requests:    1,440/día (100K free) ✅
D1 writes:   ~7,200/día (100K free) ✅
D1 reads:    ~14,400/día (5M free) ✅
Storage:     ~1 MB/día (500 MB free) ✅
```

**Free tier es suficiente para 1-5 sensores.**

## Monitoreo

```bash
# Wrangler tail (logs en tiempo real)
npx wrangler tail

# D1 queries
npx wrangler d1 execute detectic-db --command "SELECT COUNT(*) FROM snapshots"
npx wrangler d1 execute detectic-db --command "SELECT COUNT(*) FROM detections"
npx wrangler d1 execute detectic-db --command "SELECT COUNT(DISTINCT pseudonym) FROM detections"

# Metrics en Dashboard
# Cloudflare Dashboard → Workers → detectic-backend → Metrics
```

## Backup

```bash
# Export D1 a SQLite local
npx wrangler d1 export detectic-db --output backup.db
```

## Comparación final

| Opción | Costo | TLS | Mantenimiento | Escala |
|--------|-------|-----|---------------|--------|
| nf-compute-10 + nginx | $5-10/mes | Manual | OS + nginx + app | Limitada |
| Cloudflare Workers + D1 | **$0** | Automático | **Ninguna** | **Infinita** |
| Cloudflare Workers Paid | $5/mes | Automático | Ninguna | Masiva |

## Recomendación

**Cloudflare Workers es la opción óptima para Detectic:**

1. **$0/mes** — el backend no genera costos
2. **TLS automático** — sin configurar nginx/caddy
3. **Edge global** — baja latencia desde cualquier lugar
4. **Sin servidor** — no hay OS que mantener, patchear, o monitorear
5. **D1 gratis** — SQLite en la nube, sin gestionar
6. **El EX520 envía HTTPS directo** — sin proxies intermedios

El único requisito es que el EX520 tenga conexión a internet (que ya la tiene vía Wi-Fi/WAN).
