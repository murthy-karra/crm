// node --test unit for the worker's one bug-prone pure computation
// (docs/specs/SLICE_007g.md criterion 7): corrupt base64 would be
// silently 200-accepted by the endpoint and file garbage into
// Unresolved — the failure mode a live walkthrough can miss.
//
// Run: node --test infra/email-worker/worker.test.mjs
// (wired into ./scripts/check's web section; node is already required.)
import { test } from 'node:test';
import assert from 'node:assert/strict';

// `btoa` exists in Node ≥16 on globalThis, matching the Workers runtime.
import { base64Encode, MAX_RAW_BYTES } from './worker.js';

function roundtrip(bytes) {
  const encoded = base64Encode(bytes);
  const decoded = Buffer.from(encoded, 'base64');
  return new Uint8Array(decoded);
}

test('round-trips exact bytes at every alignment', () => {
  for (const len of [0, 1, 2, 3, 4, 5, 8191, 8192, 8193, 98303, 98304, 98305]) {
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) bytes[i] = (i * 31 + 7) % 256;
    assert.deepEqual(roundtrip(bytes), bytes, `len ${len}`);
  }
});

test('handles full binary range including NUL and high bytes', () => {
  const bytes = new Uint8Array(512);
  for (let i = 0; i < 512; i++) bytes[i] = i % 256;
  assert.deepEqual(roundtrip(bytes), bytes);
});

test('a max-size message round-trips without corruption', () => {
  const bytes = new Uint8Array(MAX_RAW_BYTES);
  for (let i = 0; i < bytes.length; i++) bytes[i] = (i ^ (i >> 8)) % 256;
  const encoded = base64Encode(bytes);
  // No mid-stream padding: '=' may appear only at the very end.
  const firstPad = encoded.indexOf('=');
  assert.ok(firstPad === -1 || firstPad >= encoded.length - 2, 'padding only at the end');
  assert.deepEqual(roundtrip(bytes), bytes);
});

test('the size threshold fits the endpoint body cap after encoding', () => {
  // ceil(4/3 * raw) + JSON envelope must stay under 2 MiB.
  const encodedLen = Math.ceil(MAX_RAW_BYTES / 3) * 4;
  const envelopeOverhead = 4096;
  assert.ok(encodedLen + envelopeOverhead < 2 * 1024 * 1024);
});

// --- The email() response-handling matrix (adversarial M1): the logic
// whose regression would silently drop mail (a bad ok-check) or
// permanently bounce it during a bearer mismatch (401 must temp-fail,
// never reject). Stubbed message + mocked fetch; no network.
import worker from './worker.js';

function makeMessage(bytes, { to = 'acme-realty-k7f3q2wd@leads.elysianfeld.com' } = {}) {
  const rejected = [];
  return {
    to,
    rawSize: bytes.length,
    raw: new Response(bytes).body,
    setReject(reason) {
      rejected.push(reason);
    },
    rejected,
  };
}

const ENV = {
  CRM_INBOUND_API_URL: 'https://api.example.test/inbound/email',
  CRM_INBOUND_EMAIL_SECRET: 'test-secret-value',
};

async function withFetch(status, fn) {
  const calls = [];
  const original = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    calls.push({ url, init });
    return new Response('{}', { status });
  };
  try {
    return await fn(calls);
  } finally {
    globalThis.fetch = original;
  }
}

test('oversize mail is rejected without any fetch', async () => {
  const message = makeMessage(new Uint8Array(MAX_RAW_BYTES + 1));
  await withFetch(200, async (calls) => {
    await worker.email(message, ENV);
    assert.equal(calls.length, 0);
    assert.deepEqual(message.rejected, ['message too large']);
  });
});

test('a 200 relays the envelope recipient, bearer, and exact bytes', async () => {
  const bytes = new Uint8Array([0, 1, 2, 250, 251, 252]);
  const message = makeMessage(bytes);
  await withFetch(200, async (calls) => {
    await worker.email(message, ENV);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].url, ENV.CRM_INBOUND_API_URL);
    assert.equal(calls[0].init.headers.authorization, 'Bearer test-secret-value');
    const body = JSON.parse(calls[0].init.body);
    assert.equal(body.recipient, message.to);
    assert.deepEqual(new Uint8Array(Buffer.from(body.raw, 'base64')), bytes);
    assert.equal(message.rejected.length, 0);
  });
});

test('413 and 400 bounce honestly instead of retrying forever', async () => {
  for (const status of [413, 400]) {
    const message = makeMessage(new Uint8Array(16));
    await withFetch(status, async () => {
      await worker.email(message, ENV);
      assert.equal(message.rejected.length, 1, `status ${status}`);
    });
  }
});

test('401 and 5xx throw (temp-fail -> MTA retry), never reject', async () => {
  for (const status of [401, 403, 500, 503]) {
    const message = makeMessage(new Uint8Array(16));
    await withFetch(status, async () => {
      await assert.rejects(() => worker.email(message, ENV), /HTTP/, `status ${status}`);
      assert.equal(message.rejected.length, 0, `status ${status}`);
    });
  }
});

test('a network failure propagates as a throw', async () => {
  const message = makeMessage(new Uint8Array(16));
  const original = globalThis.fetch;
  globalThis.fetch = async () => {
    throw new Error('connection refused');
  };
  try {
    await assert.rejects(() => worker.email(message, ENV));
    assert.equal(message.rejected.length, 0);
  } finally {
    globalThis.fetch = original;
  }
});
