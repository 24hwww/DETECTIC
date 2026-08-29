import { DurableObject } from 'cloudflare:workers';
import { generateVapidKeys, serializeVapidKeys, deserializeVapidKeys, sendPushNotification } from 'web-push-browser';
import {
  validateSensorToken,
  parseAllowedOrigins,
  resolveCorsOrigin,
  type SensorRegistry,
} from './protocol.ts';
import { applyCanonicalEventToD1 } from './index.ts';

export interface Env {
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
  DB: D1Database;
  /** JSON: {"sensor_id": "secret", ...} — the sensor credential registry. */
  DETECTIC_SENSORS: string;
  /** Comma-separated list of allowed dashboard origins (optional). */
  DETECTIC_ALLOWED_ORIGINS?: string;
}

interface PushSubscription {
  endpoint: string;
  expirationTime?: number | null;
  keys: { p256dh: string; auth: string };
}

/**
 * Per-socket metadata. `sensor_authed` is set to true ONLY after a sensor
 * presents a valid credential for its declared sensor_id. Frontend sockets
 * never need it (they cannot inject events). `sensor_id` on the socket is
 * always the authenticated id for sensor sockets — never the message body.
 */
type SocketMeta = {
  role?: string;
  sensor_id?: string;
  sensor_authed?: boolean;
};

interface DeviceSummary {
  first_seen: number;
  last_seen: number;
  event_count: number;
  last_type: string;
  connected: boolean;
  state?: string;
  sensor_id?: string;
  last_signal?: number;
  band?: string;
  hostname?: string;
  proximity?: string;
}

interface NetworkSummary {
  first_seen: number;
  last_seen: number;
  event_count: number;
  last_type: string;
  status: 'ONLINE' | 'OFFLINE';
  sensor_id?: string;
  ssid?: string;
  band?: string;
  w_mode?: string;
  security?: string;
  last_signal?: number;
  online_since?: number;
  proximity?: string;
  proximity_detail?: string;
}

export class RealtimeHub extends DurableObject {
  private devices: Map<string, DeviceSummary> = new Map();
  private networks: Map<string, NetworkSummary> = new Map();
  private devicesLoaded = false;
  private networksLoaded = false;
  private pushSubs: PushSubscription[] = [];
  private pushSubsLoaded = false;
  private vapidKeys: CryptoKeyPair | null = null;
  private d1: D1Database | null = null;
  private _env: Env;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.d1 = env.DB || null;
    this._env = env;
    console.log('[RealtimeHub] constructor d1=', !!this.d1, 'db=', !!env.DB);
  }

  /** Decode the DETECTIC_SENSORS credential registry once per request. */
  private sensorRegistry(): SensorRegistry {
    try {
      return (JSON.parse(this._env.DETECTIC_SENSORS || '{}') || {}) as SensorRegistry;
    } catch {
      return {};
    }
  }

  /** CORS policy for the browser-facing REST endpoints (vapid/subscribe). */
  private corsFor(request: Request): Record<string, string> {
    const origin = request.headers.get('Origin') || undefined;
    const allowed = parseAllowedOrigins(this._env.DETECTIC_ALLOWED_ORIGINS);
    const self = request.headers.get('Host');
    const selfOrigin =
      self && (self.includes('localhost') || self.includes('127.0.0.1'))
        ? `http://${self}`
        : `https://${self}`;
    const reflect = resolveCorsOrigin(origin, allowed, selfOrigin);
    const headers: Record<string, string> = {
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, X-Detectic-Sensor, X-Detectic-Signature',
      'Access-Control-Max-Age': '86400',
    };
    if (reflect) headers['Access-Control-Allow-Origin'] = reflect;
    return headers;
  }

  private async loadDevices() {
    if (this.devicesLoaded) return;
    const stored = await this.ctx.storage.get<Record<string, DeviceSummary>>('rt-devices') || {};
    for (const [id, d] of Object.entries(stored)) {
      this.devices.set(id, d);
    }
    this.devicesLoaded = true;
  }

  private async persistDevices() {
    const obj: Record<string, DeviceSummary> = {};
    for (const [id, d] of this.devices) obj[id] = d;
    await this.ctx.storage.put('rt-devices', obj);
  }

  private async loadNetworks() {
    if (this.networksLoaded) return;
    const stored = await this.ctx.storage.get<Record<string, NetworkSummary>>('rt-networks') || {};
    for (const [id, n] of Object.entries(stored)) {
      this.networks.set(id, n);
    }
    this.networksLoaded = true;
  }

  private async persistNetworks() {
    const obj: Record<string, NetworkSummary> = {};
    for (const [id, n] of this.networks) obj[id] = n;
    await this.ctx.storage.put('rt-networks', obj);
  }

  private async loadPushSubs() {
    if (this.pushSubsLoaded) return;
    const stored = await this.ctx.storage.get<PushSubscription[]>('push-subs') || [];
    this.pushSubs = stored;
    this.pushSubsLoaded = true;
  }

  private async savePushSubs() {
    await this.ctx.storage.put('push-subs', this.pushSubs);
  }

  private async getVapidKeys(): Promise<CryptoKeyPair> {
    if (this.vapidKeys) return this.vapidKeys;
    const pub = await this.ctx.storage.get<string>('vapid-public');
    const priv = await this.ctx.storage.get<string>('vapid-private');
    if (pub && priv) {
      this.vapidKeys = await deserializeVapidKeys({ publicKey: pub, privateKey: priv });
      return this.vapidKeys;
    }
    const generated = await generateVapidKeys();
    const serialized = await serializeVapidKeys(generated);
    await this.ctx.storage.put('vapid-public', serialized.publicKey);
    await this.ctx.storage.put('vapid-private', serialized.privateKey);
    this.vapidKeys = generated;
    return generated;
  }

  async getVapidPublicKey(): Promise<string> {
    const keys = await this.getVapidKeys();
    const serialized = await serializeVapidKeys(keys);
    return serialized.publicKey;
  }

  async subscribePush(sub: PushSubscription) {
    await this.loadPushSubs();
    this.pushSubs = this.pushSubs.filter(s => s.endpoint !== sub.endpoint);
    this.pushSubs.push(sub);
    await this.savePushSubs();
  }

  async unsubscribePush(endpoint: string) {
    await this.loadPushSubs();
    this.pushSubs = this.pushSubs.filter(s => s.endpoint !== endpoint);
    await this.savePushSubs();
  }

  async pushEvent(title: string, body: string, tag: string, url = '/') {
    if (!this.pushSubs.length) await this.loadPushSubs();
    if (!this.pushSubs.length) return;
    const keys = await this.getVapidKeys();
    const payload = JSON.stringify({ title, body, tag, url, ts: Date.now() });
    const dead: string[] = [];
    await Promise.all(this.pushSubs.map(async (sub) => {
      try {
        const res = await sendPushNotification(keys, {
          endpoint: sub.endpoint,
          keys: { p256dh: sub.keys.p256dh, auth: sub.keys.auth },
        }, 'mailto:notify@detectic.local', payload, { algorithm: 'aes128gcm', ttl: 60 });
        if (res.status === 410 || res.status === 404) dead.push(sub.endpoint);
      } catch (e) {
        console.error('push send error:', e);
      }
    }));
    if (dead.length) {
      this.pushSubs = this.pushSubs.filter(s => !dead.includes(s.endpoint));
      await this.savePushSubs();
    }
  }

  async maybePushForEvent(sensorId: string, msg: any) {
    const p = msg.payload || {};
    const dev = p.payload || {};
    const eventType = String(p.type || p.event_type || '');
    const id = String(p.device_id || '—').slice(0, 16);

    if (eventType === 'network.detected') {
      const net = this.networks.get(id);
      const name = dev.ssid || net?.ssid || id;
      await this.pushEvent('Nueva red detectada', `red: ${name}`, `net-detected-${id}`, '/');
    } else if (eventType === 'network.disappeared') {
      const net = this.networks.get(id);
      const name = net?.ssid || id;
      await this.pushEvent('Red desaparecida', `red: ${name} perdió señal`, `net-gone-${id}`, '/');
    } else if (eventType === 'device.connected') {
      const device = this.devices.get(id);
      const name = device?.hostname || id;
      await this.pushEvent('Dispositivo conectado', `dispositivo: ${name} se conectó`, `dev-conn-${id}`, '/');
    } else if (eventType === 'device.disconnected') {
      const device = this.devices.get(id);
      const name = device?.hostname || id;
      await this.pushEvent('Dispositivo desconectado', `dispositivo: ${name} se desconectó`, `dev-disc-${id}`, '/');
    }
  }


  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/summary') {
      await this.loadDevices();
      const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
      const cutoff = Date.now() - hours * 3600 * 1000;
      const devices: any[] = [];
      for (const [id, d] of this.devices) {
        if (d.last_seen < cutoff) continue;
        devices.push({
          device_id: id,
          first_seen: d.first_seen,
          last_seen: d.last_seen,
          event_count: d.event_count,
          last_type: d.last_type,
          connected: d.connected,
          state: d.state,
          sensor_id: d.sensor_id,
          last_signal: d.last_signal,
          band: d.band,
          hostname: d.hostname,
          proximity: d.proximity,
        });
      }
      devices.sort((a, b) => b.last_seen - a.last_seen);
      return new Response(JSON.stringify({ devices, generated_at: Date.now() }), {
        headers: { 'Content-Type': 'application/json' },
      });
    }

    if (url.pathname === '/networks') {
      await this.loadNetworks();
      const hours = Math.min(parseInt(url.searchParams.get('hours') || '24'), 720);
      const cutoff = Date.now() - hours * 3600 * 1000;
      const networks: any[] = [];
      for (const [id, n] of this.networks) {
        if (n.last_seen < cutoff && n.last_type !== 'network.disappeared') continue;
        networks.push({
          ap_id: id,
          sensor_id: n.sensor_id,
          first_seen: n.first_seen,
          last_seen: n.last_seen,
          event_count: n.event_count,
          last_type: n.last_type,
          status: n.status,
          ssid: n.ssid,
          band: n.band,
          w_mode: n.w_mode,
          security: n.security,
          last_signal: n.last_signal,
          online_since: n.online_since,
          proximity: n.proximity,
          proximity_detail: n.proximity_detail,
        });
      }
      networks.sort((a, b) => b.last_seen - a.last_seen);
      return new Response(JSON.stringify({ networks, generated_at: Date.now() }), {
        headers: { 'Content-Type': 'application/json' },
      });
    }

    if (url.pathname === '/api/v1/vapid/public-key') {
      const publicKey = await this.getVapidPublicKey();
      return new Response(JSON.stringify({ publicKey }), {
        headers: { 'Content-Type': 'application/json', ...this.corsFor(request) },
      });
    }

    if (url.pathname === '/api/v1/subscribe' && request.method === 'POST') {
      const sub = await request.json() as PushSubscription;
      if (!sub?.endpoint || !sub?.keys?.p256dh || !sub?.keys?.auth) {
        return new Response(JSON.stringify({ ok: false, error: 'invalid subscription' }), { status: 400, headers: { 'Content-Type': 'application/json', ...this.corsFor(request) } });
      }
      await this.subscribePush(sub);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { 'Content-Type': 'application/json', ...this.corsFor(request) },
      });
    }

    if (url.pathname === '/api/v1/unsubscribe' && request.method === 'POST') {
      const body = await request.json() as { endpoint?: string };
      if (!body?.endpoint) return new Response(JSON.stringify({ ok: false, error: 'missing endpoint' }), { status: 400, headers: { 'Content-Type': 'application/json', ...this.corsFor(request) } });
      await this.unsubscribePush(body.endpoint);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { 'Content-Type': 'application/json', ...this.corsFor(request) },
      });
    }

    const upgrade = request.headers.get('Upgrade');
    if (upgrade !== 'websocket') {
      return new Response('expected websocket', { status: 400 });
    }

    const role = url.searchParams.get('role') || 'unknown';
    // sensor_id in the URL is NEVER trusted for authentication; it is only a
    // routing/identity hint. Sensor sockets must also present a valid credential
    // (via the `hello` message) before they can send `event` messages.
    const sensorId = url.searchParams.get('sensor_id') || 'unknown';

    const [client, server] = Object.values(new WebSocketPair());

    (server as any).serializeAttachment({
      role,
      sensor_id: sensorId,
      sensor_authed: false,
    } as SocketMeta);
    this.ctx.acceptWebSocket(server);

    // Non-sensitive greeting. A sensor is NOT considered authenticated by
    // receiving this; it must complete the credential handshake below.
    const hello = JSON.stringify({
      type: 'hello_ack',
      protocol: 1,
      role,
      sensor_id: sensorId,
      server_time: Date.now(),
      authed: role !== 'sensor', // sensors must authenticate before trusted
      message: 'DETECTIC-RT/1 ready',
    });
    server.send(hello);

    return new Response(null, {
      status: 101,
      webSocket: client,
    });
  }

  private meta(ws: WebSocket): SocketMeta {
    try {
      return ((ws as any).deserializeAttachment() ?? {}) as SocketMeta;
    } catch {
      return {};
    }
  }

  private async updateDevice(sensorId: string, msg: any) {
    const now = Date.now();
    const p = msg.payload || {};
    const dev = p.payload || {};
    await this.loadDevices();

    const deviceId = String(dev.device_id || p.device_id || dev.pseudonym || 'unknown');
    const eventType = String(dev.type || p.event_type || p.type || 'unknown');
    const observedAt = typeof msg.observed_at === 'number' ? msg.observed_at
      : (typeof p.observed_at === 'number' ? p.observed_at
        : (typeof p.event_timestamp === 'number' ? p.event_timestamp * 1000 : now));

    const existing = this.devices.get(deviceId);
    const summary: DeviceSummary = existing || {
      first_seen: observedAt,
      last_seen: observedAt,
      event_count: 0,
      last_type: eventType,
      connected: !eventType.includes('disconnected') && !eventType.includes('absent'),
      state: eventType === 'device.presence_changed' ? String(dev.to_state || p.to_state || 'UNKNOWN') : undefined,
      sensor_id: sensorId,
    };

    summary.sensor_id = sensorId;

    const incomingHostname = dev.hostname || p.hostname;
    if (incomingHostname && !summary.hostname) {
      summary.hostname = String(incomingHostname);
    }

    summary.last_seen = Math.max(summary.last_seen, observedAt);
    summary.first_seen = Math.min(summary.first_seen, observedAt);
    summary.event_count += 1;
    summary.last_type = eventType;

    const toState = dev.to_state || p.to_state;
    const stateValue = toState ? String(toState) : summary.state;
    if (stateValue) {
      summary.state = stateValue;
    }

    if (eventType === 'device.connected' || eventType === 'device.detected' || (eventType === 'device.presence_changed' && toState === 'RF_PRESENT')) {
      summary.connected = true;
    } else if (eventType === 'device.disconnected' || eventType === 'device.network_changed' || (eventType === 'device.presence_changed' && (toState === 'ABSENT' || toState === 'DISCONNECTED'))) {
      summary.connected = false;
    }

    if (dev.rssi != null) summary.last_signal = Number(dev.rssi);
    else if (dev.new_signal != null) summary.last_signal = Number(dev.new_signal);
    else if (p.rssi != null) summary.last_signal = Number(p.rssi);
    else if (dev.rssi_dbm != null) summary.last_signal = Number(dev.rssi_dbm);
    else if (p.rssi_dbm != null) summary.last_signal = Number(p.rssi_dbm);
    if (dev.band || p.band) summary.band = String(dev.band || p.band || '');

    if (dev.proximity != null) summary.proximity = String(dev.proximity);
    else if (p.proximity != null) summary.proximity = String(p.proximity);
    else if (dev.proximity_detail?.zone_label) summary.proximity = String(dev.proximity_detail.zone_label);

    this.devices.set(deviceId, summary);
    await this.persistDevices();
  }

  private newOrValue(v: unknown): unknown {
    if (v && typeof v === 'object' && !Array.isArray(v) && 'new' in (v as Record<string, unknown>)) {
      return (v as Record<string, unknown>).new;
    }
    return v;
  }

  private async updateNetwork(sensorId: string, msg: any) {
    const now = Date.now();
    const p = msg.payload || {};
    const dev = p.payload || {};
    await this.loadNetworks();

    const apId = String(p.device_id || dev.ap_id || dev.bssid_pseudonym || 'unknown');
    const eventType = String(p.type || p.event_type || 'unknown');
    const observedAt = typeof msg.observed_at === 'number' ? msg.observed_at
      : (typeof p.observed_at === 'number' ? p.observed_at
        : (typeof p.timestamp === 'number' ? p.timestamp * 1000 : now));

    const existing = this.networks.get(apId);
    const summary: NetworkSummary = existing || {
      first_seen: observedAt,
      last_seen: observedAt,
      event_count: 0,
      last_type: eventType,
      status: 'ONLINE',
      sensor_id: sensorId,
    };

    summary.sensor_id = sensorId;

    summary.last_seen = Math.max(summary.last_seen, observedAt);
    summary.first_seen = Math.min(summary.first_seen, observedAt);
    summary.event_count += 1;
    summary.last_type = eventType;

    if (eventType === 'network.disappeared') {
      summary.status = 'OFFLINE';
      summary.online_since = undefined;
    } else {
      summary.status = 'ONLINE';
      if (!summary.online_since) summary.online_since = observedAt;
    }

    const ssid = this.newOrValue(dev.ssid);
    const band = this.newOrValue(dev.band);
    const wMode = this.newOrValue(dev.w_mode);
    const security = this.newOrValue(dev.security);
    const signal = this.newOrValue(dev.signal ?? dev.current_signal);
    const proximity = this.newOrValue(dev.proximity);

    if (ssid != null) summary.ssid = String(ssid);
    if (band != null) summary.band = String(band);
    if (wMode != null) summary.w_mode = String(wMode);
    if (security != null) summary.security = String(security);
    if (signal != null) summary.last_signal = Number(signal);

    if (proximity != null) summary.proximity = String(proximity);
    if (dev.proximity_detail && typeof dev.proximity_detail === 'object') {
      summary.proximity_detail = JSON.stringify(dev.proximity_detail);
    }

    this.networks.set(apId, summary);
    await this.persistNetworks();
  }

  private broadcastToFrontends(sensorId: string, payload: unknown, extras?: Record<string, unknown>) {
    const message = JSON.stringify({
      type: 'broadcast',
      sensor_id: sensorId,
      payload,
      server_time: Date.now(),
      ...extras,
    });
    for (const s of this.ctx.getWebSockets()) {
      const m = this.meta(s);
      if (m.role === 'frontend' && (m.sensor_id === sensorId || m.sensor_id === '*')) {
        s.send(message);
      }
    }
  }

  async webSocketMessage(ws: WebSocket, message: ArrayBuffer | string) {
    const text = typeof message === 'string' ? message : new TextDecoder().decode(message);
    let msg: any;
    try {
      msg = JSON.parse(text);
    } catch {
      ws.send(JSON.stringify({ type: 'error', message: 'expected JSON' }));
      return;
    }

    const type = msg?.type;
    const now = Date.now();
    console.log('[RealtimeHub] ws message type=', type, 'role=', this.meta(ws).role, 'sensor_id=', this.meta(ws).sensor_id);

    if (type === 'ping') {
      ws.send(JSON.stringify({
        type: 'pong',
        client_time: msg.client_time,
        server_time: now,
        rtt_ms: msg.client_time ? now - msg.client_time : null,
      }));
    } else if (type === 'test') {
      ws.send(JSON.stringify({
        type: 'test_ack',
        protocol: 1,
        server_time: now,
      }));
    } else if (type === 'hello') {
      // A sensor socket must authenticate here by presenting a credential for
      // its declared sensor_id. Frontend sockets do not authenticate.
      const meta = this.meta(ws);
      if (meta.role === 'sensor') {
        const registry = this.sensorRegistry();
        const token = typeof msg.token === 'string' && msg.token.length > 0 ? msg.token : null;
        const verdict = validateSensorToken(meta.sensor_id || '', token, registry);
        if (!verdict.ok) {
          // Reject: never log the token or the secret.
          ws.send(JSON.stringify({
            type: 'auth_error',
            reason: verdict.reason || 'invalid_credentials',
            server_time: now,
          }));
          ws.close(1008, 'auth_required');
          return;
        }
        (ws as any).serializeAttachment({
          ...meta,
          sensor_id: meta.sensor_id,
          sensor_authed: true,
        } as SocketMeta);
      }

      ws.send(JSON.stringify({
        type: 'hello_ack',
        protocol: 1,
        server_time: now,
      }));
      if (this.meta(ws).role === 'sensor') {
        ws.send(JSON.stringify({
          type: 'command',
          command: 'GET_STATUS',
          protocol: 1,
          server_time: now,
        }));
      }
    } else if (type === 'command_ack') {
      ws.send(JSON.stringify({
        type: 'command_ack_ok',
        command: msg.command,
        server_time: now,
      }));
    } else if (type === 'event') {
      // Only authenticated sensor sockets may push events. The sensor_id used
      // for attribution is the one bound at handshake time, never the message
      // body, so a sensor cannot impersonate another sensor_id.
      console.log('[RealtimeHub] event received sensor_id=', this.meta(ws).sensor_id, 'authed=', this.meta(ws).sensor_authed);
      const meta = this.meta(ws);
      const authedSensor = meta.role === 'sensor' && meta.sensor_authed === true && typeof meta.sensor_id === 'string' && meta.sensor_id.length > 0;
      if (!authedSensor) {
        ws.send(JSON.stringify({
          type: 'auth_error',
          reason: 'unauthorized_sender',
          server_time: now,
        }));
        ws.close(1008, 'unauthorized');
        return;
      }
      const sensorId = meta.sensor_id!;
      const p = msg.payload || {};
      const eventType = String(p.type || p.event_type || '');
      if (eventType.startsWith('network.')) {
        try {
          await this.updateNetwork(sensorId, msg);
        } catch (e: any) {
          console.error('updateNetwork error:', e?.message || e);
        }
      } else {
        try {
          await this.updateDevice(sensorId, msg);
        } catch (e: any) {
          console.error('updateDevice error:', e?.message || e);
        }
      }

      // Persist event to D1 so historical queries work for the dashboard,
      // then apply the same side effects the HTTP batch path uses
      // (device_state, ap_state, rf_environment_snapshots, device_aliases).
      try {
        const inserted = await this.persistEventToD1(sensorId, msg);
        if (inserted && this._env.DB) {
          // applyCanonicalEventToD1 only needs env.DB; the realtime Env is a subset.
          await applyCanonicalEventToD1(this._env as any, sensorId, msg.payload, Math.floor(Date.now() / 1000));
        }
      } catch (e: any) {
        console.error('D1 persist/side-effect error:', e?.message || e);
      }

      const eventPayload = msg.payload || {};
      const observedAt =
        typeof eventPayload.observed_at === 'number'
          ? eventPayload.observed_at
          : msg.observed_at;
      this.broadcastToFrontends(sensorId, eventPayload, { observed_at: observedAt });
      await this.maybePushForEvent(sensorId, msg);
      const ack = JSON.stringify({
        type: 'event_ack',
        event_id: msg.event_id,
        received_at: now,
      });
      ws.send(ack);
    } else if (type === 'subscribe') {
      (ws as any).serializeAttachment({
        role: 'frontend',
        sensor_id: msg.sensor_id || '*',
      } as SocketMeta);
      ws.send(JSON.stringify({
        type: 'subscribe_ack',
        sensor_id: msg.sensor_id || '*',
        server_time: now,
      }));
    } else {
      ws.send(JSON.stringify({ type: 'error', message: `unknown type: ${type}` }));
    }
  }

  async webSocketClose(ws: WebSocket, code: number, reason: string, wasClean: boolean) {
    try {
      // 1006 is reserved and may not be used as a close code we send.
      const closeCode = code === 1006 ? 1000 : (code || 1000);
      ws.close(closeCode, reason);
    } catch {
      // Already closed or invalid code; nothing more to do.
    }
  }

  // RPC entry point for the Worker to push ingested HTTP events into the
  // real-time fan-out without waiting for the response to the sensor.
  async notify(events: any[], sensorId: string): Promise<void> {
    for (const e of events) {
      const eventType = String(e.type || e.event_type || '');
      const msg = { payload: e, observed_at: Date.now() };
      if (eventType.startsWith('network.')) {
        try { await this.updateNetwork(sensorId, msg); } catch (e: any) { console.error('notify updateNetwork error:', e?.message || e); }
      } else if (eventType.startsWith('device.')) {
        try { await this.updateDevice(sensorId, msg); } catch (e: any) { console.error('notify updateDevice error:', e?.message || e); }
      }
      this.broadcastToFrontends(sensorId, e, { via: 'http_ingest', persisted: true });
    }
  }

  /**
   * Persist a WSS event to D1 so historical dashboard queries work.
   * Uses INSERT OR IGNORE to handle duplicates (event_id is UNIQUE).
   * Returns true if a new row was actually inserted.
   */
  private async persistEventToD1(sensorId: string, msg: any): Promise<boolean> {
    if (!this.d1) {
      console.error('[RealtimeHub] persistEventToD1 skipped: no D1 binding');
      return false;
    }

    const p = msg.payload || {};
    const dev = p.payload || p;
    const eventId = msg.event_id || p.event_id || '';
    if (!eventId) return false;

    const eventType = String(p.type || p.event_type || '');
    const ts = typeof p.timestamp === 'number'
      ? p.timestamp
      : typeof p.event_timestamp === 'number'
        ? p.event_timestamp
        : Math.floor(Date.now() / 1000);
    const deviceId = p.device_id ?? null;
    const payloadJson = JSON.stringify(dev);
    const seq = typeof p.sequence === 'number' ? p.sequence : null;
    const now = Math.floor(Date.now() / 1000);

    const result = await this.d1.prepare(
      "INSERT OR IGNORE INTO events (sensor_id, event_id, event_type, event_timestamp, device_id, snapshot_json, payload_json, sequence, schema_version, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
      .bind(
        sensorId,
        eventId,
        eventType,
        ts,
        deviceId,
        null,
        payloadJson,
        seq,
        '3.0',
        now
      )
      .run();

    const inserted = (result.meta?.changes ?? 0) > 0;
    console.log('[RealtimeHub] persistEventToD1 event_id=', eventId, 'event_type=', eventType, 'inserted=', inserted, 'changes=', result.meta?.changes);

    // Update sensor last_seen
    await this.d1.prepare(
      "UPDATE sensors SET last_seen = ? WHERE id = ?"
    ).bind(now, sensorId).run();

    return inserted;
  }
}
