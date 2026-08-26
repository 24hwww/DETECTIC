import { DurableObject } from 'cloudflare:workers';

export interface Env {
  REALTIME_HUB: DurableObjectNamespace<RealtimeHub>;
}

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

    this.ctx.acceptWebSocket(server);

    const hello = JSON.stringify({
      type: 'hello_ack',
      protocol: 1,
      role,
      server_time: Date.now(),
      message: 'DETECTIC-RT/1 ready',
    });
    server.send(hello);

    return new Response(null, {
      status: 101,
      webSocket: client,
    });
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
      // from Cloudflare → EX520 over the same WSS.
      ws.send(JSON.stringify({
        type: 'command',
        command: 'GET_STATUS',
        protocol: 1,
        server_time: now,
      }));
    } else if (type === 'command_ack') {
      ws.send(JSON.stringify({
        type: 'command_ack_ok',
        command: msg.command,
        server_time: now,
      }));
    } else if (type === 'event') {
      // For the real-time fan-out test: echo back an ack and broadcast to all
      // connected clients (including frontends subscribed to this sensor).
      const sensorId = msg.sensor_id || 'unknown';
      const ack = JSON.stringify({
        type: 'event_ack',
        event_id: msg.event_id,
        received_at: now,
      });
      ws.send(ack);

      const broadcast = JSON.stringify({
        type: 'broadcast',
        sensor_id: sensorId,
        payload: msg.payload,
        received_at: now,
      });
      for (const s of this.ctx.getWebSockets()) {
        if (s !== ws) s.send(broadcast);
      }
    } else {
      ws.send(JSON.stringify({ type: 'error', message: `unknown type: ${type}` }));
    }
  }

  async webSocketClose(ws: WebSocket, code: number, reason: string, wasClean: boolean) {
    ws.close(code, reason);
  }
}
