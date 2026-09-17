# Stateful Web E2E: unattended routing and unresolved intake

`routing-v1` implements a bounded catalog 03 journey in nine dependent steps.
Authority: AGENTS.md, D-015, D-023, D-035–D-041, D-050, SLICE_007b,
SLICE_007e and SLICE_008. No application code or contract changes.

Shared seed creates two operational Organizations and four memberships through
normal application commands, with no People, raw intake or rotation state.
Admin configures default assignment through the Web. Synthetic RFC822 mail using
the application's authored Cypress Bay pinned template enters the real inbound
HTTP endpoint, with the ephemeral family relay credential. Real parsing creates
People/inquiries and routes to Alice's Today through Centrifugo/refetch.

The family verifies byte-identical delivery deduplication, invalid recipient and
transport rejection, independent same-byte delivery to the second tenant,
unassigned visibility, four round-robin assignments in canonical membership order,
duplicates preserving the rotation cursor, and repeat inquiries retaining existing
ownership without consuming a rotation slot. Browser settings changes persist on
reload; immutable routing facts retain system actor/webhook attribution.

A pinned message missing contact fields deterministically enters Unresolved.
Members see metadata, admins decrypt content on demand, and member/foreign raw
reads, retries and discard are denied. Try again preserves the unresolved reason;
admin discard updates every connected queue. Redelivery and repeat discard retain
the tombstone and original attribution; retry/read fail after discard. Read-only
PostgreSQL assertions confirm exact family data, retained ciphertext without the
plaintext sentinel, and tenant-scoped rotation. Realtime carries no content.

No external email is sent, no Cloudflare Worker is deployed, and no extraction
provider is used. The synthetic relay envelope enters the real private API directly.
Intake addresses and bearer credentials are remembered for artifact sanitization;
rendered intake addresses are masked by the shared browser harness.

Run: `./scripts/e2e --family routing`. Syntax checks passed. Run `293f7a88aca9`
passed all nine steps with cleanup `verified_empty`. Initial authoring failures
(native Fetch status handling and observer subscription readiness) are retained
in earlier run artifacts; no application changes were needed. Final combined-suite
evidence follows below.

Limits: successful unresolved-to-Person extraction/retry, Groq confidence/failure
handling, actual mail-provider delivery/authentication, forwarded-message parsing,
address rotation, explicit assignment override, deactivated/new-member rotation
and lock-contention recovery remain outside this family. The pinned missing-contact
retry verifies deterministic failure and durable discard, not successful extraction.

## Final combined verification

Run `949f3e952fb3` passed all six families (61 steps), with three browser families
active simultaneously, all images reused and every environment removed. See
[expansion verification](WEB_E2E_EXPANSION.md) for authoritative evidence and the
shared recorder fixes discovered during parallel execution.
