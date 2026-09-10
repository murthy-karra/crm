// node --test unit for the worker's bug-prone pure computations
// (docs/specs/SLICE_007g.md criterion 7; docs/specs/SLICE_017.md §5):
// corrupt base64 would be silently 200-accepted by the endpoint and file
// garbage into Unresolved — the failure mode a live walkthrough can miss.
//
// Run: node --test infra/email-worker/worker.test.mjs
// (wired into ./scripts/check's web section; node is already required.
// Node 24 has the TransformStream/ReadableStream/Response globals this
// file and worker.js both rely on.)
import { test } from 'node:test';
import assert from 'node:assert/strict';

import { base64Transform, MAX_RAW_BYTES } from './worker.js';

function fixtureBytes(len) {
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = (i * 31 + 7) % 256;
  return bytes;
}

// Feeds `bytes` into a fresh `base64Transform()` as a sequence of chunks
// sized from `sizes` (a fixed chunk size, or an array of sizes cycled
// until `bytes` is exhausted — the "mixed split" case), asserting every
// encoded chunk is ASCII `Uint8Array` bytes (never a string, spec
// criterion: "ASCII Uint8Array chunks out"), and returns the
// concatenated encoded text.
async function encodeThroughTransform(bytes, sizes) {
  const sizeList = Array.isArray(sizes) ? sizes : [sizes];
  const chunks = [];
  let offset = 0;
  let s = 0;
  while (offset < bytes.length) {
    const size = sizeList[s % sizeList.length];
    chunks.push(bytes.subarray(offset, offset + size));
    offset += size;
    s++;
  }

  const source = new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(chunk);
      controller.close();
    },
  });

  const reader = source.pipeThrough(base64Transform()).getReader();
  const outChunks = [];
  let totalLen = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    assert.ok(
      value instanceof Uint8Array,
      'encoded chunks must be ASCII bytes, never a string',
    );
    outChunks.push(value);
    totalLen += value.length;
  }
  const out = new Uint8Array(totalLen);
  let o = 0;
  for (const c of outChunks) {
    out.set(c, o);
    o += c.length;
  }
  return Buffer.from(out).toString('ascii');
}

test('base64Transform round-trips exact bytes at every chunk alignment, padding only at the end', async () => {
  const sizes = [1, 2, 3, 4, 5, 8191, 8192, 8193, 98303, 98304, 98305];
  for (const size of sizes) {
    // A couple of same-size chunks plus a short final remainder already
    // exercises the carry recurrence at this exact chunk size (carry-in
    // -> combined -> carry-out, repeated, then flushed); it needs no
    // megabytes of fixture data, unlike feeding one huge shared buffer
    // one byte at a time for the size-1 case.
    const bytes = fixtureBytes(size * 2 + 5);
    const encodedText = await encodeThroughTransform(bytes, size);
    const firstPad = encodedText.indexOf('=');
    assert.ok(
      firstPad === -1 || firstPad >= encodedText.length - 2,
      `padding only at the end for chunk size ${size}`,
    );
    const decoded = new Uint8Array(Buffer.from(encodedText, 'base64'));
    assert.deepEqual(decoded, bytes, `chunk size ${size}`);
  }
});

test('base64Transform round-trips exact bytes across a mixed chunk-size split', async () => {
  const mixedSizes = [1, 8191, 2, 98304, 3, 5, 4];
  const total = mixedSizes.reduce((sum, n) => sum + n, 0);
  const bytes = fixtureBytes(total);
  const encodedText = await encodeThroughTransform(bytes, mixedSizes);
  const firstPad = encodedText.indexOf('=');
  assert.ok(firstPad === -1 || firstPad >= encodedText.length - 2, 'padding only at the end');
  const decoded = new Uint8Array(Buffer.from(encodedText, 'base64'));
  assert.deepEqual(decoded, bytes);
});

test('the derivation pin: encoded MAX_RAW_BYTES plus envelope overhead fits the endpoint 34 MiB cap', () => {
  // 4 * ceil(MAX_RAW_BYTES / 3) + 4096 <= 34 MiB (the endpoint's
  // MAX_INBOUND_EMAIL_BODY_BYTES, mirrored — docs/specs/SLICE_017.md §5.4).
  const encodedLen = Math.ceil(MAX_RAW_BYTES / 3) * 4;
  const envelopeOverhead = 4096;
  assert.ok(encodedLen + envelopeOverhead <= 34 * 1024 * 1024);
});

// --- The email() response-handling matrix (adversarial M1): the logic
// whose regression would silently drop mail (a bad ok-check) or
// permanently bounce it during a bearer mismatch (401 must temp-fail,
// never reject). Stubbed message + mocked fetch; no network.
import worker from './worker.js';

// A genuine multi-chunk `ReadableStream` (unlike `new Response(bytes).body`,
// which hands the whole buffer to the runtime as one piece): used to prove
// the base64 carry crosses `email()`'s own stream chunks, not only the
// chunks `base64Transform()` is fed directly in the unit tests above.
function chunkedStream(bytes, chunkSize) {
  let offset = 0;
  return new ReadableStream({
    pull(controller) {
      if (offset >= bytes.length) {
        controller.close();
        return;
      }
      const end = Math.min(offset + chunkSize, bytes.length);
      controller.enqueue(bytes.subarray(offset, end));
      offset = end;
    },
  });
}

function makeMessage(
  bytes,
  { to = 'acme-realty-k7f3q2wd@leads.elysianfeld.com', chunkSize } = {},
) {
  const rejected = [];
  return {
    to,
    rawSize: bytes.length,
    raw: chunkSize ? chunkedStream(bytes, chunkSize) : new Response(bytes).body,
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
    assert.equal(calls[0].init.duplex, 'half');
    const text = await new Response(calls[0].init.body).text();
    const body = JSON.parse(text);
    assert.equal(body.recipient, message.to);
    assert.deepEqual(new Uint8Array(Buffer.from(body.raw, 'base64')), bytes);
    assert.equal(message.rejected.length, 0);
  });
});

test('a 25 MiB message relays as a streamed body and decodes to the exact bytes', async () => {
  const bytes = fixtureBytes(MAX_RAW_BYTES);
  // A chunk size that is not a multiple of 3, so the base64 carry crosses
  // chunk boundaries through email() itself (spec §5.2), not only inside
  // base64Transform()'s own directly-fed unit tests above.
  const message = makeMessage(bytes, { chunkSize: 65_537 });
  await withFetch(200, async (calls) => {
    await worker.email(message, ENV);
    assert.equal(calls.length, 1);
    const { init } = calls[0];
    assert.ok(
      init.body instanceof ReadableStream,
      'the request body must be a stream, never a string',
    );
    assert.equal(init.duplex, 'half');
    const text = await new Response(init.body).text();
    const parsed = JSON.parse(text);
    assert.equal(parsed.recipient, message.to);
    const decoded = new Uint8Array(Buffer.from(parsed.raw, 'base64'));
    assert.deepEqual(decoded, bytes);
    assert.equal(message.rejected.length, 0);
  });
});

test('a recipient with a quoted local part containing " and \\ produces valid JSON with the exact recipient', async () => {
  // RFC 5321 quoted local part carrying an escaped embedded quote and a
  // literal backslash — JSON.stringify must escape both correctly.
  const trickyRecipient = String.raw`"weird\"quoted"@example.com`;
  const message = makeMessage(new Uint8Array([9, 8, 7]), { to: trickyRecipient });
  await withFetch(200, async (calls) => {
    await worker.email(message, ENV);
    const text = await new Response(calls[0].init.body).text();
    const parsed = JSON.parse(text); // throws if the JSON is malformed
    assert.equal(parsed.recipient, trickyRecipient);
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
