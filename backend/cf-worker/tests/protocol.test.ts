/**
 * Unit tests for the Worker/DO protocol primitives in src/protocol.ts.
 *
 * Run with Node's built-in TypeScript stripping (no build step required):
 *   node --experimental-strip-types tests/protocol.test.ts
 *
 * These cover the WSS sensor-authentication handshake, the CORS policy, and
 * the deterministic ID-keyed event-ACK selection. They are intentionally
 * free of Cloudflare globals so they can be exercised on a plain Node runtime.
 */
import {
  constantTimeEqual,
  validateSensorToken,
  parseAllowedOrigins,
  resolveCorsOrigin,
  buildAckBody,
  buildOpaqueError,
  selectAcceptedEvents,
} from '../src/protocol.ts';

let failures = 0;
function check(name: string, cond: boolean, detail?: unknown) {
  if (cond) {
    console.log(`  ok   ${name}`);
  } else {
    failures++;
    console.error(`FAIL ${name}${detail !== undefined ? ` -> ${JSON.stringify(detail)}` : ''}`);
  }
}

const REGISTRY = { 'ex520-001': 'sensor-secret-001', 'ex520-002': 'sensor-secret-002' };

function testConstantTimeEqual() {
  console.log('# constantTimeEqual');
  check('equal strings', constantTimeEqual('abc', 'abc'));
  check('different strings', !constantTimeEqual('abc', 'abd'));
  check('different lengths', !constantTimeEqual('abc', 'abcd'));
  check('empty equal', constantTimeEqual('', ''));
}

function testWssAuth() {
  console.log('# WSS sensor authentication');

  check(
    'authenticated sensor accepted',
    validateSensorToken('ex520-001', 'sensor-secret-001', REGISTRY).ok
  );
  check(
    'second sensor accepted with its own credential',
    validateSensorToken('ex520-002', 'sensor-secret-002', REGISTRY).ok
  );
  check(
    'missing token rejected',
    !validateSensorToken('ex520-001', '', REGISTRY).ok &&
      validateSensorToken('ex520-001', '', REGISTRY).reason === 'missing_token'
  );
  check(
    'null token rejected',
    !validateSensorToken('ex520-001', null, REGISTRY).ok &&
      validateSensorToken('ex520-001', null, REGISTRY).reason === 'missing_token'
  );
  check(
    'invalid token rejected',
    !validateSensorToken('ex520-001', 'wrong-token', REGISTRY).ok &&
      validateSensorToken('ex520-001', 'wrong-token', REGISTRY).reason === 'invalid_token'
  );
  check(
    'unknown sensor rejected',
    !validateSensorToken('ex520-999', 'anything', REGISTRY).ok &&
      validateSensorToken('ex520-999', 'anything', REGISTRY).reason === 'unknown_sensor'
  );
  check(
    'malformed (no sensor_id) rejected',
    !validateSensorToken('', 'sensor-secret-001', REGISTRY).ok ||
      validateSensorToken('', 'sensor-secret-001', REGISTRY).reason === 'missing_sensor_id'
  );
  check(
    'sensor A cannot authenticate as sensor B',
    (() => {
      // Present sensor_id = A but the credential of B.
      const res = validateSensorToken('ex520-001', 'sensor-secret-002', REGISTRY);
      return !res.ok && res.reason === 'invalid_token';
    })()
  );
  check(
    'sensor B cannot authenticate as sensor A',
    !validateSensorToken('ex520-002', 'sensor-secret-001', REGISTRY).ok
  );
}

function testCors() {
  console.log('# CORS policy');
  const allowed = parseAllowedOrigins('https://detectic.24hwww.workers.dev');

  check(
    'allowed dashboard origin reflected',
    resolveCorsOrigin('https://detectic.24hwww.workers.dev', allowed, 'https://detectic.24hwww.workers.dev') ===
      'https://detectic.24hwww.workers.dev'
  );
  check(
    'disallowed origin -> null (no ACAO)',
    resolveCorsOrigin('https://evil.example', allowed, 'https://detectic.24hwww.workers.dev') === null
  );
  check(
    'absent Origin -> null (no wildcard)',
    resolveCorsOrigin(null, allowed, 'https://detectic.24hwww.workers.dev') === null &&
      resolveCorsOrigin(undefined, allowed, 'https://detectic.24hwww.workers.dev') === null
  );
  check(
    'self origin reflected even without explicit allow-list',
    resolveCorsOrigin('https://detectic.24hwww.workers.dev', parseAllowedOrigins(undefined), 'https://detectic.24hwww.workers.dev') ===
      'https://detectic.24hwww.workers.dev'
  );
  check('parseAllowedOrigins trims/ignores empties', parseAllowedOrigins(' a , , b ').size === 2);
  check('parseAllowedOrigins empty for absent', parseAllowedOrigins(undefined).size === 0);
}

function testAckContract() {
  console.log('# event ACK contract');
  const ack = buildAckBody(['a', 'b'], ['c'], ['d']);
  check('accepted count', ack.accepted === 2);
  check('duplicates count', ack.duplicates === 1);
  check('rejected count', ack.rejected === 1);
  check('ids preserved by class', ack.accepted_ids.join(',') === 'a,b' && ack.duplicate_ids.join(',') === 'c' && ack.rejected_ids.join(',') === 'd');
}

function testSelectAcceptedEvents() {
  console.log('# ID-keyed accepted-event selection (no positional inference)');
  const id = (e: string) => ({ event_id: e });

  // all unique
  check(
    'all unique events accepted by id',
    selectAcceptedEvents([id('A'), id('B'), id('C')], new Set(['A', 'B', 'C'])).map((e) => e.event_id).join(',') === 'A,B,C'
  );

  // duplicate in first position: never selected because its id is not accepted
  check(
    'duplicate in first position excluded',
    selectAcceptedEvents([id('DUP'), id('A'), id('B')], new Set(['A', 'B'])).map((e) => e.event_id).join(',') === 'A,B'
  );

  // duplicate in middle
  check(
    'duplicate in middle excluded',
    selectAcceptedEvents([id('A'), id('DUP'), id('B')], new Set(['A', 'B'])).map((e) => e.event_id).join(',') === 'A,B'
  );

  // duplicate at end
  check(
    'duplicate at end excluded',
    selectAcceptedEvents([id('A'), id('B'), id('DUP')], new Set(['A', 'B'])).map((e) => e.event_id).join(',') === 'A,B'
  );

  // mixed accepted/duplicate/rejected
  check(
    'mixed classed events: accepted kept, duplicates/rejected excluded',
    selectAcceptedEvents([id('A'), id('X'), id('X'), id('B'), id('R')], new Set(['A', 'B'])).map((e) => e.event_id).join(',') === 'A,B'
  );

  // ordering is preserved and only-by-id
  check(
    'selection preserves original order and is id-keyed',
    (() => {
      const inEvents = [id('A'), id('B'), id('A'), id('R')];
      const selected = selectAcceptedEvents(inEvents, new Set(['A']));
      return selected.map((e) => e.event_id).join(',') === 'A,A' && selected.length === 2;
    })()
  );
}

function testOpaqueError() {
  console.log('# production-safe 500 error body');
  const body = buildOpaqueError('req-abc');
  check('marks internal_error', body.error === 'internal_error');
  check('carries request_id for correlation', body.request_id === 'req-abc');
  const json = JSON.stringify(body);
  check('no stack trace leaked', !json.includes('stack') && !json.includes('at '));
  check('no filesystem path leaked', !json.includes('/home/') && !json.includes('/var/') && !json.includes('src/'));
  check('no secret/credential leaked', !json.toLowerCase().includes('secret') && !json.includes('password') && !/[\w-]{20,}/.test(json));
  check('no internal impl detail (function names)', !json.includes('handleIngest') && !json.includes('fetch'));
}

testConstantTimeEqual();
testWssAuth();
testCors();
testAckContract();
testSelectAcceptedEvents();
testOpaqueError();

if (failures === 0) {
  console.log('\nALL PROTOCOL TESTS PASSED');
  process.exit(0);
} else {
  console.error(`\n${failures} TEST(S) FAILED`);
  process.exit(1);
}
