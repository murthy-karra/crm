// The elysianfeld.com inbound relay (docs/specs/SLICE_007g.md §3):
// Cloudflare Email Routing (catch-all on leads.elysianfeld.com) delivers
// here; this worker relays the raw RFC 822 bytes to the CRM's frozen
// POST /inbound/email endpoint and nothing else. No parsing, no
// filtering, no content logging — token validation and every judgement
// belong to the endpoint.
//
// Deploy: `wrangler deploy`, then
//   wrangler secret put CRM_INBOUND_EMAIL_SECRET   (same value as .env)
// Vars: CRM_INBOUND_API_URL (wrangler.toml), e.g.
//   https://api.tarams.org/inbound/email

// Cloudflare's own Email Routing inbound ceiling (D-056/docs/specs/
// SLICE_017.md §2): Email Routing rejects messages over 25 MiB before any
// worker runs, so that is the ceiling at which our own bounce disappears
// entirely — no message Cloudflare accepts is ever bounced by us for
// size. Encoded: 4 * ceil(26_214_400 / 3) = 34_952_536 bytes, which plus
// the JSON envelope fits under the endpoint's 34 MiB body cap
// (`MAX_INBOUND_EMAIL_BODY_BYTES`, crm-api/src/routes/inbound_email.rs).
// Over it → an honest bounce beats a silent 413 (spec §3).
export const MAX_RAW_BYTES = 25 * 1024 * 1024;

// Table-driven base64 encoder, streaming (docs/specs/SLICE_017.md §2):
// buffering a 25 MiB message plus its base64 plus a JSON copy would need
// most of a Worker's 128 MB isolate, so bytes are encoded chunk by chunk
// as they arrive and never joined into one string. `base64Transform()` is
// an exported pure `TransformStream` factory — Uint8Array chunks in,
// ASCII Uint8Array chunks out, unit-tested with node --test (spec
// criterion 7: corrupt base64 would be silently 200-accepted and file
// garbage, the one failure mode a live walkthrough can miss).
const B64_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const B64_TABLE = new Uint8Array(64);
for (let i = 0; i < 64; i++) B64_TABLE[i] = B64_ALPHABET.charCodeAt(i);
const B64_PAD = 61; // '='.charCodeAt(0)

// Encodes a byte length that is an exact multiple of 3 (no padding).
function encodeAligned(bytes) {
  const groups = bytes.length / 3;
  const out = new Uint8Array(groups * 4);
  let oi = 0;
  for (let i = 0; i < bytes.length; i += 3) {
    const b0 = bytes[i];
    const b1 = bytes[i + 1];
    const b2 = bytes[i + 2];
    out[oi++] = B64_TABLE[b0 >> 2];
    out[oi++] = B64_TABLE[((b0 & 0x03) << 4) | (b1 >> 4)];
    out[oi++] = B64_TABLE[((b1 & 0x0f) << 2) | (b2 >> 6)];
    out[oi++] = B64_TABLE[b2 & 0x3f];
  }
  return out;
}

// Encodes the final 0, 1, or 2 leftover bytes, padded with '=' — the only
// place padding may appear (spec §5.1).
function encodeTail(bytes) {
  if (bytes.length === 0) return new Uint8Array(0);
  const out = new Uint8Array(4);
  if (bytes.length === 1) {
    const b0 = bytes[0];
    out[0] = B64_TABLE[b0 >> 2];
    out[1] = B64_TABLE[(b0 & 0x03) << 4];
    out[2] = B64_PAD;
    out[3] = B64_PAD;
  } else {
    const b0 = bytes[0];
    const b1 = bytes[1];
    out[0] = B64_TABLE[b0 >> 2];
    out[1] = B64_TABLE[((b0 & 0x03) << 4) | (b1 >> 4)];
    out[2] = B64_TABLE[(b1 & 0x0f) << 2];
    out[3] = B64_PAD;
  }
  return out;
}

// Streaming base64 encoder: `Uint8Array` chunks in, ASCII `Uint8Array`
// chunks out. Carries a 0–2 byte remainder between chunks (3-byte
// groups only encode across a `transform` boundary once the carry makes
// up a full group) so '=' padding can only ever appear in the very last
// output chunk, from `flush`.
export function base64Transform() {
  let carry = new Uint8Array(0);

  return new TransformStream({
    transform(chunk, controller) {
      let combined;
      if (carry.length === 0) {
        combined = chunk;
      } else {
        combined = new Uint8Array(carry.length + chunk.length);
        combined.set(carry, 0);
        combined.set(chunk, carry.length);
      }
      const alignedLen = combined.length - (combined.length % 3);
      if (alignedLen > 0) {
        controller.enqueue(encodeAligned(combined.subarray(0, alignedLen)));
      }
      // Copy (never a subarray view) so the carry never keeps a whole
      // large chunk's backing buffer alive for the sake of 0–2 bytes.
      carry = combined.slice(alignedLen);
    },
    flush(controller) {
      if (carry.length > 0) {
        controller.enqueue(encodeTail(carry));
      }
    },
  });
}

const textEncoder = new TextEncoder();

// Builds the frozen `{"recipient","raw"}` envelope as a byte stream —
// prefix, then `raw` piped through `base64Transform()`, then the suffix —
// so the relay never concatenates the encoded message into one string
// and never calls `arrayBuffer()`/`text()` on `message.raw`. `pull` (not
// `start`) drives it, so the stream only reads ahead of `fetch` as fast
// as `fetch` actually consumes it — peak memory is a few chunks, not the
// whole message.
function inboundEmailBody(recipient, rawStream) {
  const prefix = textEncoder.encode(
    `{"recipient":${JSON.stringify(recipient)},"raw":"`,
  );
  const suffix = textEncoder.encode('"}');
  const reader = rawStream.pipeThrough(base64Transform()).getReader();
  let prefixSent = false;
  let suffixSent = false;

  return new ReadableStream({
    async pull(controller) {
      if (!prefixSent) {
        prefixSent = true;
        controller.enqueue(prefix);
        return;
      }
      const { done, value } = await reader.read();
      if (done) {
        if (!suffixSent) {
          suffixSent = true;
          controller.enqueue(suffix);
        }
        controller.close();
        return;
      }
      controller.enqueue(value);
    },
  });
}

export default {
  async email(message, env) {
    if (message.rawSize > MAX_RAW_BYTES) {
      message.setReject('message too large');
      return;
    }

    const response = await fetch(env.CRM_INBOUND_API_URL, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${env.CRM_INBOUND_EMAIL_SECRET}`,
      },
      // The envelope recipient (RCPT TO) — never the To: header.
      body: inboundEmailBody(message.to, message.raw),
      // WHATWG fetch and Node's undici require `duplex` for a stream
      // body; `workerd` ignores init members it does not know
      // (docs/specs/SLICE_017.md §2) — passed unconditionally.
      duplex: 'half',
      // Deterministic temp-fail instead of riding the runtime's
      // execution limit when the tunnel/API hangs; an abort throws →
      // the same safe retry path. 60 s (was 30 s): a 34 MiB body at a
      // conservative 10 Mbit/s takes about 28 s to reach the origin.
      signal: AbortSignal.timeout(60_000),
    });

    // 2xx: done — accepted/duplicate/rejected are all 200 by design;
    // the endpoint is the oracle-free judge.
    if (response.ok) return;

    // 413/400: this mail can never succeed — honest bounce.
    if (response.status === 413 || response.status === 400) {
      message.setReject('message could not be accepted');
      return;
    }

    // 401/403 (misconfigured/rotated-out-of-sync bearer) and 5xx/other:
    // throw so the mail layer temp-fails and the sending MTA retries
    // while a human fixes it. Idempotency at the endpoint absorbs
    // redelivery. (Retry semantics live-verified in the 007g
    // walkthrough — spec §3.)
    throw new Error(`inbound relay failed: HTTP ${response.status}`);
  },
};
