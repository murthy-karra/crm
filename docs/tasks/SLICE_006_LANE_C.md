# Task brief — Slice 006, Lane C (telephony infrastructure)

Parent specification: `docs/specs/SLICE_006.md` §7, §11. Read:
`AGENTS.md` §7, `docs/decisions/DECISION_LOG.md` (D-013, D-016 §4/§5,
D-024, D-025, D-030), `docs/architecture/ARCHITECTURE_BASELINE.md`
(Telephony), README "External connectivity", `infra/development/**`,
`scripts/dev-tunnel`.

## Outcome

A public-IP Linux host running Caddy (TLS, Let's Encrypt) + LiveKit
server + SIP service + Redis from a committed
`infra/telephony/compose.yaml` (+ `Caddyfile`, `livekit.yaml`,
`sip.yaml`, `.env.example` for the host), reached by a plain DNS A
record `livekit.tarams.org` (no tunnel), signaling `wss://` and the
Twirp API `https://` both through Caddy on 443, media on the host's
public UDP, webhook URL `https://api.tarams.org/webhooks/
livekit`, no egress. A README "Telephony" section: provisioning, ports,
DNS, how to verify with the `lk` CLI (`lk room list`, a test SIP
participant), and how to rotate the API key. Telnyx one-time setup
documented for the user (number, credential connection) — the trunk
itself is created by Lane A's `scripts/telephony-trunk`.

## Ownership boundary

`infra/telephony/**` and the README "Telephony" section only. No
secrets committed (D-013): the host's `.env` is gitignored; the
DNS record is dashboard-managed (D-025) — document it, don't script
it.

## Needs from the user

(a) The host (VPS or OVH box, SSH access, public IPv4). (b) A DNS A
record `livekit.tarams.org` → the host IP, **DNS-only** (grey cloud,
not proxied — proxying would put signaling and the API behind
Cloudflare's 100 s cap and break ICE assumptions). (c) Ports 80/443 TCP
reachable so Caddy can obtain the certificate. (d)
Telnyx: one number with caller-ID set, a credential SIP connection with
an outbound voice profile assigned. (e) The same `LIVEKIT_API_KEY` /
`SECRET` pair in the host's `livekit.yaml` (incl. the webhook `api_key`)
and the Mac's `.env` — document the manual sync. Report exact firewall
rules required, and note host log retention (SIP logs carry numbers).

## Required checks

From the Mac: `lk room create`/`list`/`delete` against the host; a
browser joins a room over `wss://livekit.tarams.org` with a token minted
by `lk token`; with the trunk configured, `lk sip participant create` to
the user's phone rings it. Never print credentials.

## Stop and report

Any need to route media or signaling through cloudflared or a proxied
hostname, or to expose the host's LiveKit API without TLS; any Telnyx feature requiring the application to
hold Telnyx credentials.
