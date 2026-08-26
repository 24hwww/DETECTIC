const assert = require('assert');
const {
  isPrivateIp,
  normalizeIpGeo,
  pickPrimaryLocation,
  makeLocation,
  rssiWeight,
  rssiToVisualizationRadius,
  estimatePosition
} = require('../spatial-engine.cjs');

function run(name, fn) { try { fn(); console.log('✓', name); } catch (e) { console.error('✗', name, e.message); process.exitCode = 1; } }

run('isPrivateIp rejects private IPv4', () => {
  assert.strictEqual(isPrivateIp('192.168.1.23'), true);
  assert.strictEqual(isPrivateIp('10.0.0.5'), true);
  assert.strictEqual(isPrivateIp('172.16.0.1'), true);
  assert.strictEqual(isPrivateIp('172.31.255.255'), true);
  assert.strictEqual(isPrivateIp('127.0.0.1'), true);
});

run('isPrivateIp accepts public IPv4', () => {
  assert.strictEqual(isPrivateIp('45.239.185.202'), false);
  assert.strictEqual(isPrivateIp('8.8.8.8'), false);
});

run('isPrivateIp handles IPv6 link-local', () => {
  assert.strictEqual(isPrivateIp('fe80::1'), true);
  assert.strictEqual(isPrivateIp('::1'), true);
  assert.strictEqual(isPrivateIp('2001:4860:4860::8888'), false);
});

run('isPrivateIp treats missing/empty as private', () => {
  assert.strictEqual(isPrivateIp(''), true);
  assert.strictEqual(isPrivateIp(null), true);
});

run('normalizeIpGeo returns Location', () => {
  const loc = normalizeIpGeo({ latitude: -28.68, longitude: -49.35, accuracy: 10000 });
  assert.strictEqual(loc.source, 'ip_geolocation');
  assert.strictEqual(loc.accuracy_m, 10000);
  assert.strictEqual(typeof loc.timestamp, 'number');
});

run('normalizeIpGeo rejects missing coordinates', () => {
  assert.strictEqual(normalizeIpGeo({ latitude: null, longitude: -49.35 }), null);
  assert.strictEqual(normalizeIpGeo(null), null);
});

run('pickPrimaryLocation: GPS beats GeoIP', () => {
  const gps = makeLocation({ latitude: -28.68, longitude: -49.35, source: 'gps', accuracy_m: 5 });
  const geo = makeLocation({ latitude: -28.70, longitude: -49.40, source: 'ip_geolocation', accuracy_m: 10000 });
  const chosen = pickPrimaryLocation([geo, gps]);
  assert.strictEqual(chosen.source, 'gps');
});

run('pickPrimaryLocation: manual beats GeoIP', () => {
  const manual = makeLocation({ latitude: -28.68, longitude: -49.35, source: 'manual', accuracy_m: 10 });
  const geo = makeLocation({ latitude: -28.70, longitude: -49.40, source: 'ip_geolocation', accuracy_m: 10000 });
  const chosen = pickPrimaryLocation([geo, manual]);
  assert.strictEqual(chosen.source, 'manual');
});

run('pickPrimaryLocation: lower accuracy wins within same source', () => {
  const a = makeLocation({ latitude: 0, longitude: 0, source: 'manual', accuracy_m: 100 });
  const b = makeLocation({ latitude: 0, longitude: 0, source: 'manual', accuracy_m: 5 });
  const chosen = pickPrimaryLocation([a, b]);
  assert.strictEqual(chosen.accuracy_m, 5);
});

run('rssiWeight: stronger signal has higher weight', () => {
  assert.ok(rssiWeight(-40) > rssiWeight(-70));
  assert.strictEqual(rssiWeight(-120), 0);
});

run('rssiToVisualizationRadius returns visualization radii', () => {
  assert.strictEqual(rssiToVisualizationRadius(-40), 25);
  assert.strictEqual(rssiToVisualizationRadius(-55), 50);
  assert.strictEqual(rssiToVisualizationRadius(-65), 120);
  assert.strictEqual(rssiToVisualizationRadius(-75), 250);
  assert.strictEqual(rssiToVisualizationRadius(-90), 500);
});

run('estimatePosition single sensor: rf_estimation', () => {
  const sensors = {
    s1: { location: makeLocation({ latitude: -28.68, longitude: -49.35, source: 'manual', accuracy_m: 10 }) }
  };
  const obs = [{ deviceId: 'd1', sensorId: 's1', rssi: -60, ts: Date.now() }];
  const est = estimatePosition('d1', obs, sensors);
  assert.strictEqual(est.source, 'rf_estimation');
  assert.strictEqual(est.method, 'single_sensor_proximity');
  assert.strictEqual(est.latitude, -28.68);
  assert.strictEqual(est.confidence, 0.25);
  assert.ok(est.accuracy_m >= 50);
});

run('estimatePosition multi sensor: weighted_centroid', () => {
  const sensors = {
    s1: { location: makeLocation({ latitude: -28.68, longitude: -49.35, source: 'manual', accuracy_m: 10 }) },
    s2: { location: makeLocation({ latitude: -28.68, longitude: -49.36, source: 'manual', accuracy_m: 10 }) }
  };
  const obs = [
    { deviceId: 'd1', sensorId: 's1', rssi: -50, ts: Date.now() },
    { deviceId: 'd1', sensorId: 's2', rssi: -70, ts: Date.now() }
  ];
  const est = estimatePosition('d1', obs, sensors);
  assert.strictEqual(est.source, 'estimated');
  assert.strictEqual(est.method, 'weighted_centroid');
  assert.ok(est.confidence > 0.3);
  // Stronger s1 should pull estimate closer to s1
  assert.ok(Math.abs(est.longitude - (-49.35)) < Math.abs(est.longitude - (-49.36)));
});

run('estimatePosition ignores weak RSSI', () => {
  const sensors = {
    s1: { location: makeLocation({ latitude: -28.68, longitude: -49.35, source: 'manual', accuracy_m: 10 }) }
  };
  const obs = [{ deviceId: 'd1', sensorId: 's1', rssi: -100, ts: Date.now() }];
  assert.strictEqual(estimatePosition('d1', obs, sensors), null);
});

run('estimatePosition no sensor with valid location', () => {
  const sensors = { s1: { location: makeLocation({ source: 'unknown' }) } };
  const obs = [{ deviceId: 'd1', sensorId: 's1', rssi: -60, ts: Date.now() }];
  assert.strictEqual(estimatePosition('d1', obs, sensors), null);
});

console.log('Done.');
