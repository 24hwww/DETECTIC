// DETECTIC RF Map — hardened spatial engine (CommonJS testable core)
// This is a reference implementation; map.html currently inlines the same logic.

const SOURCE_PRIORITY = {
  gps: 0,
  manual: 1,
  sensor_known_location: 2,
  rf_estimation: 3,
  estimated: 4,
  ip_geolocation: 5,
  unknown: 9
};

function isPrivateIp(ip) {
  if (!ip) return true;
  const v4 = ip.trim();
  if (v4.startsWith('127.')) return true;
  if (v4.startsWith('10.')) return true;
  if (v4.startsWith('192.168.')) return true;
  if (/^172\.(1[6-9]|2\d|3[01])\./.test(v4)) return true;
  if (v4.startsWith('fe80:') || v4.startsWith('fc') || v4.startsWith('fd') || v4 === '::1') return true;
  return false;
}

function normalizeIpGeo(raw) {
  if (!raw || typeof raw.latitude !== 'number' || typeof raw.longitude !== 'number') return null;
  return {
    latitude: Number(raw.latitude),
    longitude: Number(raw.longitude),
    accuracy_m: Number(raw.accuracy_m) || Number(raw.accuracy) || 10000,
    source: 'ip_geolocation',
    confidence: null,
    timestamp: Date.now()
  };
}

function makeLocation({ latitude, longitude, accuracy_m = 0, source = 'unknown', confidence = null, timestamp = Date.now(), method = null } = {}) {
  return { latitude, longitude, accuracy_m, source, confidence, timestamp, method };
}

function isValidLocation(loc) {
  return loc && typeof loc.latitude === 'number' && typeof loc.longitude === 'number';
}

function locationSourceRank(loc) {
  return SOURCE_PRIORITY[loc?.source] ?? 99;
}

function pickPrimaryLocation(locations) {
  const sorted = (locations || []).filter(isValidLocation).sort((a, b) => {
    const ra = locationSourceRank(a);
    const rb = locationSourceRank(b);
    if (ra !== rb) return ra - rb;
    return (a.accuracy_m ?? Infinity) - (b.accuracy_m ?? Infinity);
  });
  return sorted[0] || makeLocation({ source: 'unknown' });
}

function rssiWeight(rssi) {
  if (rssi == null || rssi < -100) return 0;
  return Math.pow(10, (rssi + 30) / 40);
}

function rssiToVisualizationRadius(rssi) {
  // NOT physical meters; visualization-only uncertainty radius.
  if (rssi == null) return 500;
  if (rssi > -50) return 25;
  if (rssi > -60) return 50;
  if (rssi > -70) return 120;
  if (rssi > -80) return 250;
  return 500;
}

function getSignalQuality(rssi) {
  if (rssi == null) return 'unknown';
  if (rssi > -50) return 'very-strong';
  if (rssi > -60) return 'strong';
  if (rssi > -70) return 'medium';
  if (rssi > -80) return 'weak';
  return 'very-weak';
}

function visualizationDistanceFromRssi(rssi, cfg = { pathLossExponent: 2.2, rssiAt1m: -40 }) {
  if (rssi == null || rssi >= cfg.rssiAt1m) return 1;
  const db = cfg.rssiAt1m - rssi;
  const meters = Math.pow(10, db / (10 * cfg.pathLossExponent));
  return Math.max(1, Math.min(1000, meters));
}

function distanceMeters(a, b) {
  const R = 6371000;
  const toRad = (x) => x * Math.PI / 180;
  const dLat = toRad(b.latitude - a.latitude);
  const dLon = toRad(b.longitude - a.longitude);
  const lat1 = toRad(a.latitude);
  const lat2 = toRad(b.latitude);
  const x = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2;
  const c = 2 * Math.atan2(Math.sqrt(x), Math.sqrt(1 - x));
  return R * c;
}

function weightedCentroid(observations, sensors) {
  let wLat = 0, wLon = 0, wDen = 0;
  for (const o of observations) {
    const s = sensors[o.sensorId];
    if (!s || !isValidLocation(s.location)) continue;
    const w = rssiWeight(o.rssi);
    if (w <= 0) continue;
    wLat += s.location.latitude * w;
    wLon += s.location.longitude * w;
    wDen += w;
  }
  if (wDen === 0) return null;
  const lat = wLat / wDen;
  const lon = wLon / wDen;
  let acc = 0, accDen = 0;
  for (const o of observations) {
    const s = sensors[o.sensorId];
    if (!s || !isValidLocation(s.location)) continue;
    const w = rssiWeight(o.rssi);
    if (w <= 0) continue;
    const visRadius = rssiToVisualizationRadius(o.rssi);
    const d = distanceMeters({ latitude: s.location.latitude, longitude: s.location.longitude }, { latitude: lat, longitude: lon });
    acc += (d + visRadius) * w;
    accDen += w;
  }
  const conf = Math.min(0.95, 0.35 + (observations.length - 1) * 0.15);
  return makeLocation({
    latitude: lat,
    longitude: lon,
    accuracy_m: accDen > 0 ? Math.round(acc / accDen) : 200,
    source: 'estimated',
    confidence: conf,
    method: 'weighted_centroid',
    timestamp: Date.now()
  });
}

function estimatePosition(deviceId, obs, sensors) {
  const bySensor = {};
  for (const o of obs) {
    if (o.deviceId !== deviceId) continue;
    if (!sensors[o.sensorId] || !isValidLocation(sensors[o.sensorId].location)) continue;
    if (o.rssi == null || o.rssi < -95) continue;
    bySensor[o.sensorId] = bySensor[o.sensorId] || [];
    bySensor[o.sensorId].push(o);
  }
  const ids = Object.keys(bySensor);
  if (ids.length === 0) return null;

  if (ids.length === 1) {
    const s = sensors[ids[0]];
    const lastObs = bySensor[ids[0]][bySensor[ids[0]].length - 1];
    const radius = rssiToVisualizationRadius(lastObs.rssi);
    return makeLocation({
      latitude: s.location.latitude,
      longitude: s.location.longitude,
      accuracy_m: radius,
      source: 'rf_estimation',
      confidence: 0.25,
      method: 'single_sensor_proximity',
      timestamp: lastObs.ts
    });
  }

  const latest = ids.map(sid => bySensor[sid][bySensor[sid].length - 1]);
  return weightedCentroid(latest, sensors);
}

module.exports = {
  isPrivateIp,
  normalizeIpGeo,
  pickPrimaryLocation,
  makeLocation,
  isValidLocation,
  rssiWeight,
  rssiToVisualizationRadius,
  getSignalQuality,
  visualizationDistanceFromRssi,
  weightedCentroid,
  estimatePosition,
  SOURCE_PRIORITY
};
