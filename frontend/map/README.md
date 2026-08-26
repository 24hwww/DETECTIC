# DETECTIC RF Map

Módulo de visualización geoespacial para DETECTIC.

## Stack

- `backend/cf-worker/src/map.html` — página completa servida por Cloudflare Workers.
- `MapLibre GL JS` — renderizado del mapa.
- `Turf.js` — utilidades geoespaciales (círculos, centroides, etc.).
- Sin framework adicional: se reutiliza el worker existente como entrega estática.

## Arquitectura

```
Data Adapters (mock | REST | WebSocket)
        ↓
  Normalizers (IP geolocation, RSSI, location)
        ↓
     Store (sensors, APs, devices, observations)
        ↓
  Spatial Engine (filters, estimates, heatmap)
        ↓
  MapLibre Layers (sources/layers)
        ↓
  UI (legend, sidebar, timeline, popups)
```

## Contratos de datos

### Ubicación

```js
{
  latitude,
  longitude,
  source,   // 'gps' | 'manual' | 'sensor_known_location' | 'ip_geolocation' | 'rf_estimation' | 'estimated' | 'unknown'
  accuracy, // metros
  confidence,
  timestamp
}
```

### Sensor

```js
{
  id,
  sensorId,
  location,
  network: { publicIp, privateIp },
  status,
  firstSeenAt,
  lastSeenAt,
  observations
}
```

### Access Point

```js
{
  id,
  bssidPseudonym,
  ssid,
  band,
  channel,
  firstSeenAt,
  lastSeenAt,
  location
}
```

### Dispositivo

```js
{
  id,
  vendor,
  deviceType,
  firstSeenAt,
  lastSeenAt,
  status,
  observations, // IDs
  sensors,      // sensor IDs
  bands,
  channels
}
```

### Observación

```js
{
  id,
  sensorId,
  deviceId,
  accessPointId,
  timestamp,
  rssi,
  channel,
  band,
  position: null,
  positionType: 'unknown'
}
```

## Fuentes de ubicación y prioridad

1. `gps`
2. `manual`
3. `sensor_known_location`
4. `rf_estimation`
5. `ip_geolocation`
6. `unknown`

GPS y manual siempre ganan sobre GeoIP.

## Reglas críticas

- **No se inventan coordenadas.**
- **GeoIP es una ubicación aproximada de red/ISP, no una coordenada física exacta del dispositivo.**
- **Las IPs privadas no se convierten a coordenadas.**
- **RSSI no se convierte directamente a metros**; se usa como proximidad/estimación con incertidumbre.
- **Las posiciones estimadas se distinguen visualmente de las observadas o GPS.**
- **No se exponen MAC/BSSID reales**; se usan pseudónimos.

## Capas del mapa

| Layer | Color | Significado |
|-------|-------|-------------|
| `sensors-known` | verde | Sensor con ubicación conocida (GPS/manual/known) |
| `sensors-ip-ring` | naranja punteado | Área aproximada de IP geolocation |
| `aps` | azul | Access Points |
| `devices-observed` | violeta | Dispositivo observado (sin posición propia) |
| `devices-estimated` | rojo | Dispositivo con posición estimada |
| `rf-links` | degradado | Enlaces sensor-dispositivo con color según RSSI |
| `heatmap` | - | Mapa de calor de observaciones RF |

## Estimación de posición

Métodos preparados:

- `unknown`
- `single_sensor_estimate`
- `weighted_centroid`

Actualmente implementado:

- Si un dispositivo es visto por más de un sensor con coordenadas conocidas, se calcula un **centroide ponderado** usando la intensidad de señal como peso.
- Si solo un sensor lo ve, se devuelve una posición con `source: 'rf_estimation'`, `method: 'single_sensor_estimate'` y un radio de búsqueda basado en RSSI (visualización, no distancia real).

Multilateración y triangulación requieren más de un sensor con ubicación conocida y un modelo de pérdida de trayectoria calibrado.

## Filtros

- Sensores
- APs
- Dispositivos
- Heatmap
- Enlaces RF
- Banda 2.4 GHz
- Banda 5 GHz
- RSSI mínimo
- Rango de tiempo (15m, 1h, 6h, 24h, todo)

## Datos reales vs Mock

- `Mock` — dataset demostrativo con 3 sensores, 2 APs y 3 dispositivos.
- `REST` — consume `/api/v1/sensors`, `/api/v1/devices`, `/api/v1/reports/networks`, `/api/v1/reports/devices`.
- `WebSocket` — carga REST inicial y luego escucha `/ws` para observaciones en tiempo real.

## Límites actuales

- Los sensores reales aún **no tienen coordenadas almacenadas**; por eso en modo REST aparecen como `unknown` a menos que se configuren manualmente o se agregue GeoIP del sensor en el backend.
- GeoIP del sensor requiere que el backend almacene la IP pública del sensor y/o su ubicación.
- El cálculo de distancia por RSSI no es un modelo físico; es una aproximación visual con radios de calidad de señal.
- La multilateración está preparada en la interfaz pero no es producción todavía.

## URLs

- `/` — Dashboard principal.
- `/map` — RF Map.

## Archivos

- `backend/cf-worker/src/map.html` — implementación del mapa.
- `backend/cf-worker/src/index.ts` — ruta `/map`, captura de IP/GeoIP y actualización de ubicación de sensores.
- `frontend/map/spatial-engine.cjs` — motor espacial endurecido (CommonJS testable).
- `frontend/map/tests/spatial-engine.test.cjs` — pruebas del motor espacial.
- `frontend/map/README.md` — este documento.

## Hardening — cambios recientes

- Se eliminó `turf.rhumbDestination([0,0], 0, 0)` y cualquier cálculo de coordenadas de relleno.
- `rssiToApproxRadius` se renombró a `rssiToVisualizationRadius` para dejar claro que no son metros físicos.
- Se centraliza la prioridad de fuentes: `gps > manual > sensor_known_location > rf_estimation > estimated > ip_geolocation > unknown`.
- Se implementa `single_sensor_proximity` y `weighted_centroid` correctamente.
- El backend ahora captura `public_ip` y `ip_geolocation` desde `request.cf` y las almacena en `sensors.location`.
- El backend expone `POST /api/v1/sensors/:id/location` para ubicación manual.
- `GET /api/v1/sensors` ahora devuelve `location` resuelta y `public_ip`.
- Se agregaron tests en `frontend/map/tests/spatial-engine.test.cjs` y pasan con `node`.
