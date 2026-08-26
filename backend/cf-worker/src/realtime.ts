import { DurableObject } from 'cloudflare:workers';

export interface Env {
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
}

type SocketMeta = { role?: string; sensor_id?: string };

export class RealtimeHub extends DurableObject {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
  }

  async fetch(request: Request): Promise<Response> {
    const upgrade = request.headers.get('Upgrade');
    if (upgrade !== 'websocket') {
      return new Response('expected websocket', { status: 400 });
    }

    const url = new URL(request.url);
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
      // Full-duplex probe: push a harmless diagnostic command
      // from Cloudflare -> EX520 over the same WSS, but only to sensors.
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
      // Sensor sent a real-time event over WSS. Echo ack and fan out.
      const sensorId = msg.sensor_id || 'unknown';
      const ack = JSON.stringify({
        type: 'event_ack',
        event_id: msg.event_id,
        received_at: now,
      });
      ws.send(ack);
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

  // RPC entry point for a frontend or command fan-out.
  async say(payload: { sensor_id: string; text: string }): Promise<void> {
    this.broadcastToFrontends(payload.sensor_id, payload, { kind: 'system' });
  }
}
