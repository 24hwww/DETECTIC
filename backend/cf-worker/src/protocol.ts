/**
 * Detectic Worker/DO shared protocol primitives.
 *
 * Pure, side-effect-free, dependency-free helpers used by both the Worker
 * (index.ts) and the RealtimeHub Durable Object (realtime.ts). They are kept
 * free of Cloudflare globals so they can be unit-tested with a plain Node
 * TypeScript-stripping runner (`node --experimental-strip-types`).
 *
 * Responsibilities:
 *   - constant-time credential comparison
 *   - sensor WSS handshake validation against the DETECTIC_SENSORS registry
 *   - CORS allow-origin resolution (no wildcard grant)
 *   - deterministic event-ACK selection by stable event ID (no positional
 *     inference when duplicates are present)
 */

export type SensorRegistry = Record<string, string>;

/**
 * Constant-time string comparison. Returns true only if the two strings are
 * byte-for-byte identical. Runtime is a function of the comparison length and
 * does not short-circuit on the first differing byte, so it leaks nothing
 * about the secret.
 */
export function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) {
    // Still consume a comparable amount of work, then fail.
    let acc = 0;
    for (let i = 0; i < a.length; i++) acc |= a.charCodeAt(i);
    void acc;
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

export interface SensorHandshakeResult {
  ok: boolean;
  reason?: string;
}

/**
 * Validate a sensor WSS handshake.
 *
 *   - registry is the decoded DETECTIC_SENSORS object: { sensor_id: secret }.
 *   - sensor_id must be a known key.
 *   - token must be present, non-empty, and exactly equal (constant time) to
 *     the registered secret for that sensor_id.
 *
 * A token is never returned or logged. sensor_id is never trusted on its own:
 * it is only ever used as a key into the registry.
 */
export function validateSensorToken(
  sensorId: string,
  token: string | undefined | null,
  registry: SensorRegistry
): SensorHandshakeResult {
  if (!sensorId || typeof sensorId !== "string") {
    return { ok: false, reason: "missing_sensor_id" };
  }
  if (!token || typeof token !== "string" || token.length === 0) {
    return { ok: false, reason: "missing_token" };
  }
  const registered = registry[sensorId];
  if (registered === undefined || registered === null) {
    return { ok: false, reason: "unknown_sensor" };
  }
  if (!constantTimeEqual(registered, token)) {
    return { ok: false, reason: "invalid_token" };
  }
  return { ok: true };
}

/**
 * Parse a comma-separated list of allowed origins into a trimmed Set.
 * Empty strings and whitespace-only entries are ignored. Returns an empty Set
 * when the raw value is absent.
 */
export function parseAllowedOrigins(raw: string | undefined | null): Set<string> {
  const out = new Set<string>();
  if (!raw) return out;
  for (const chunk of raw.split(",")) {
    const t = chunk.trim();
    if (t.length > 0) out.add(t);
  }
  return out;
}

/**
 * Decide which Origin (if any) to reflect in Access-Control-Allow-Origin.
 *
 * Rules:
 *   - Returns the request origin ONLY if it is an explicit allowed origin OR it
 *     equals the worker's own origin (same-origin dashboard).
 *   - Returns null for: absent Origin, disallowed origins, and any request that
 *     is not a legitimate dashboard origin. A null return means no ACAO header
 *     is emitted, so browsers block cross-origin reads.
 *   - It never returns "*".
 */
export function resolveCorsOrigin(
  requestOrigin: string | undefined | null,
  allowedOrigins: ReadonlySet<string>,
  selfOrigin: string | undefined | null
): string | null {
  if (!requestOrigin) return null;
  if (allowedOrigins.has(requestOrigin)) return requestOrigin;
  if (selfOrigin && requestOrigin === selfOrigin) return requestOrigin;
  return null;
}

export interface AckOutcome {
  accepted_ids: string[];
  duplicate_ids: string[];
  rejected_ids: string[];
  accepted: number;
  duplicates: number;
  rejected: number;
}

/**
 * Build the canonical event-batch ACK contract body.
 *
 * The three classes are distinguished and carried explicitly by stable event
 * ID (never by array position):
 *   - accepted_ids   — newly persisted events (resolved; remove from retry)
 *   - duplicate_ids  — already-known event IDs (resolved; remove from retry so
 *                      the sensor never re-sends forever)
 *   - rejected_ids   — malformed/un-insertable events (retain on the sensor for
 *                      diagnostics; kept distinguishable from the above)
 */
export function buildAckBody(
  acceptedIds: string[],
  duplicateIds: string[],
  rejectedIds: string[]
): AckOutcome {
  return {
    accepted_ids: acceptedIds,
    duplicate_ids: duplicateIds,
    rejected_ids: rejectedIds,
    accepted: acceptedIds.length,
    duplicates: duplicateIds.length,
    rejected: rejectedIds.length,
  };
}

/**
 * Select the events that were accepted, keyed strictly by stable event ID.
 *
 * This is the explicit-alternative to positional inference
 * (`.filter((_, i) => i < accepted)`), which breaks when the backend processes
 * duplicates or otherwise changes classification independent of array position.
 *
 *   - acceptedEvents must be identified by membership of their event_id in
 *     `acceptedIds`.
 *   - Order of the returned array is the original input order.
 *   - A duplicate (repeated event_id) is NOT returned unless its id is in the
 *     accepted set.
 */
export function selectAcceptedEvents<E extends { event_id?: string }>(
  events: ReadonlyArray<E>,
  acceptedIds: ReadonlySet<string>
): E[] {
  const out: E[] = [];
  for (const ev of events) {
    if (ev.event_id != null && acceptedIds.has(ev.event_id)) out.push(ev);
  }
  return out;
}

/**
 * Production-safe 500 response body. Contains only an opaque error class and
 * a correlation request ID. It must never leak stack frames, filesystem paths,
 * function names, secrets, environment variables or internal implementation
 * details — those are logged server-side only.
 */
export function buildOpaqueError(requestId: string): { error: string; request_id: string } {
  return { error: 'internal_error', request_id: requestId };
}
