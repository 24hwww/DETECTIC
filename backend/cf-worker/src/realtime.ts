import { DurableObject } from 'cloudflare:workers';
import { generateVapidKeys, serializeVapidKeys, deserializeVapidKeys, sendPushNotification } from 'web-push-browser';

export interface Env {
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
}

interface PushSubscription {
  endpoint: string;
  expirationTime?: number | null;
  keys: { p256dh: string; auth: string };
}

type SocketMeta = { role?: string; sensor_id?: string };

interface DeviceSummary {
  first_seen: number;
  last_seen: number;
  event_count: number;
  last_type: string;
  connected: boolean;
  sensor_id?: string;
  last_signal?: number;
  band?: string;
  hostname?: string;
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
}

export class RealtimeHub extends DurableObject {
  private devices: Map<string, DeviceSummary> = new Map();
  private networks: Map<string, NetworkSummary> = new Map();
  private devicesLoaded = false;
  private networksLoaded = false;
  private pushSubs: PushSubscription[] = [];
  private pushSubsLoaded = false;
  private vapidKeys: CryptoKeyPair | null = null;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
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
          sensor_id: d.sensor_id,
          last_signal: d.last_signal,
          band: d.band,
          hostname: d.hostname,
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
        headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
      });
    }

    if (url.pathname === '/api/v1/subscribe' && request.method === 'POST') {
      const sub = await request.json() as PushSubscription;
      if (!sub?.endpoint || !sub?.keys?.p256dh || !sub?.keys?.auth) {
        return new Response(JSON.stringify({ ok: false, error: 'invalid subscription' }), { status: 400, headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' } });
      }
      await this.subscribePush(sub);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
      });
    }

    if (url.pathname === '/api/v1/unsubscribe' && request.method === 'POST') {
      const body = await request.json() as { endpoint?: string };
      if (!body?.endpoint) return new Response(JSON.stringify({ ok: false, error: 'missing endpoint' }), { status: 400, headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' } });
      await this.unsubscribePush(body.endpoint);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
      });
    }

    const upgrade = request.headers.get('Upgrade');
    if (upgrade !== 'websocket') {
      return new Response('expected websocket', { status: 400 });
    }

    const role = url.searchParams.get('role') || 'unknown';
    const sensorId = url.searchParams.get('sensor_id') || 'unknown';

    const [client, server] = Object.values(new WebSocketPair());

    (server as any).serializeAttachment({ role, sensor_id: sensorId } as SocketMeta);
    this.ctx.acceptWebSocket(server);

    const hello = JSON.stringify({
      type: 'hello_ack',
      protocol: 1,
      role,
      sensor_id: sensorId,
      server_time: Date.now(),
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
      connected: !eventType.includes('disconnected'),
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

    if (eventType === 'device.connected' || eventType === 'device.detected') {
      summary.connected = true;
    } else if (eventType === 'device.disconnected' || eventType === 'device.network_changed') {
      summary.connected = false;
    }

    if (dev.rssi != null) summary.last_signal = Number(dev.rssi);
    else if (dev.new_signal != null) summary.last_signal = Number(dev.new_signal);
    else if (p.rssi != null) summary.last_signal = Number(p.rssi);
    if (dev.band || p.band) summary.band = String(dev.band || p.band || '');

    this.devices.set(deviceId, summary);
    await this.persistDevices();
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
        : (typeof p.event_timestamp === 'number' ? p.event_timestamp * 1000 : now));

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

    if (dev.ssid != null) summary.ssid = String(dev.ssid);
    if (dev.band != null) summary.band = String(dev.band);
    if (dev.w_mode != null) summary.w_mode = String(dev.w_mode);
    if (dev.security != null) summary.security = String(dev.security);
    if (dev.signal != null) summary.last_signal = Number(dev.signal);
    else if (dev.current_signal != null) summary.last_signal = Number(dev.current_signal);

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
      const sensorId = msg.sensor_id || 'unknown';
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
      this.broadcastToFrontends(sensorId, msg, { observed_at: msg.observed_at });
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
      this.broadcastToFrontends(sensorId, e, { via: 'http_ingest', persisted: true });
    }
  }
}
