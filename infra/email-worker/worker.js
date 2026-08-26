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

// The endpoint caps the HTTP body at 2 MiB; base64 inflates raw bytes by
// 4/3 and the JSON envelope adds overhead. The largest raw size whose
// encoded body safely fits: floor((2 MiB - 4 KiB overhead) * 3/4),
// rounded down to a clean 1.4 MiB. Over it → an honest bounce beats a
// silent 413 (spec §3).
export const MAX_RAW_BYTES = Math.floor(1.4 * 1024 * 1024);

// Chunked base64: btoa on a megabyte-scale binary string blows the
// argument/stack limits; encode 3-byte-aligned chunks so no padding
// appears mid-stream. Exported pure function — unit-tested with
// node --test (spec criterion 7: corrupt base64 would be silently
// 200-accepted and file garbage, the one failure mode a live
// walkthrough can miss).
export function base64Encode(bytes) {
  const CHUNK = 3 * 32 * 1024; // 3-byte aligned → no mid-stream padding
  let out = '';
  for (let i = 0; i < bytes.length; i += CHUNK) {
    const slice = bytes.subarray(i, i + CHUNK);
    let bin = '';
    for (let j = 0; j < slice.length; j += 8192) {
      bin += String.fromCharCode.apply(null, slice.subarray(j, j + 8192));
    }
    out += btoa(bin);
  }
  return out;
}

export default {
  async email(message, env) {
    if (message.rawSize > MAX_RAW_BYTES) {
      message.setReject('message too large');
      return;
    }

    const raw = new Uint8Array(await new Response(message.raw).arrayBuffer());
    const body = JSON.stringify({
      // The envelope recipient (RCPT TO) — never the To: header.
      recipient: message.to,
      raw: base64Encode(raw),
    });

    const response = await fetch(env.CRM_INBOUND_API_URL, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${env.CRM_INBOUND_EMAIL_SECRET}`,
      },
      body,
      // Deterministic temp-fail instead of riding the runtime's
      // execution limit when the tunnel/API hangs; an abort throws →
      // the same safe retry path.
      signal: AbortSignal.timeout(30_000),
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
