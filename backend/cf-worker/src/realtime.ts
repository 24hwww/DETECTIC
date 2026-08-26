import { DurableObject } from 'cloudflare:workers';

export interface Env {
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
}

type SocketMeta = { role?: string; sensor_id?: string };

interface DeviceSummary {
  first_seen: number;
  last_seen: number;
  event_count: number;
  last_type: string;
  connected: boolean;
  last_signal?: number;
  band?: string;
}

export class RealtimeHub extends DurableObject {
  private devices: Map<string, DeviceSummary> = new Map();
  private loaded = false;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
  }

  private async loadDevices() {
    if (this.loaded) return;
    const stored = await this.ctx.storage.get<Record<string, DeviceSummary>>('rt-devices') || {};
    for (const [id, d] of Object.entries(stored)) {
      this.devices.set(id, d);
    }
    this.loaded = true;
  }

  private async persistDevices() {
    const obj: Record<string, DeviceSummary> = {};
    for (const [id, d] of this.devices) obj[id] = d;
    await this.ctx.storage.put('rt-devices', obj);
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    await this.loadDevices();

    if (url.pathname === '/summary') {
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
          last_signal: d.last_signal,
          band: d.band,
        });
      }
      devices.sort((a, b) => b.last_seen - a.last_seen);
      return new Response(JSON.stringify({ devices, generated_at: Date.now() }), {
        headers: { 'Content-Type': 'application/json' },
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
    };

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
      const ack = JSON.stringify({
        type: 'event_ack',
        event_id: msg.event_id,
        received_at: now,
      });
      ws.send(ack);
      await this.updateDevice(sensorId, msg);
      this.broadcastToFrontends(sensorId, msg, { observed_at: msg.observed_at });
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
    ws.close(code, reason);
  }

  // RPC entry point for the Worker to push ingested HTTP events into the
  // real-time fan-out without waiting for the response to the sensor.
  async notify(events: any[], sensorId: string): Promise<void> {
    for (const e of events) {
      this.broadcastToFrontends(sensorId, e, { via: 'http_ingest', persisted: true });
    }
  }
}
