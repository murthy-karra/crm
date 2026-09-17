# Captured correspondence E2E

`correspondence-v1` implements catalog family 04 against the real Web, API,
PostgreSQL and Centrifugo stack. This lane owns the family spec, seed and this
record; the coordinator owns registry/configuration, browser masking and runs.

## Stateful coverage

The seed uses authenticated inquiry commands for three People: two in the primary
Organization and one in a foreign Organization sharing the primary correspondent's
email. It creates no captured history, held messages or business rows through SQL.

The browser journey signs in the agent, another member, an admin and a foreign
admin, then:

1. Reads the agent's capture address and verifies new inquiry work.
2. Delivers synthetic outbound CC mail through the frozen authenticated inbound
   HTTP boundary; proves org-wide timeline metadata, automatic email/sent contact
   attribution and realtime removal from Today.
3. Delivers a synthetic reply-all and proves live `client_replied` work and metadata.
4. Captures the next outbound response using envelope-only BCC delivery, clearing
   reply work. Identical relay redelivery produces no duplicate effects.
5. Forwards an older message, verifies its original date and forwarded rendering,
   and forwards it again with another outer Message-ID but the same original
   References ID. Facts deduplicate and latest contact time does not move backward.
6. Holds two unmatched messages. Other members, admins and the foreign tenant
   cannot list or resolve them; cross-tenant Person linkage fails. No Person is
   silently created.
7. Links an incoming held message via the Web with contact creation, verifies
   realtime history, same-target idempotency and different-target conflict, then
   dismisses another row. Terminal rows null their counterparty address.
8. Rotates the credential through the confirmation UI, rejects old-address mail
   and accepts the same message at the new address. Reload preserves the address.
9. Verifies metadata-only history shape, encrypted/processed raw retention outside
   `raw_payload`, tenant isolation, unchanged Person count and no provider calls.

All mail is generated locally and delivered to the isolated API. No real mailbox,
SMTP provider, DNS relay, external message or extraction model is used. This tests
capture processing after the relay boundary, not end-to-end Internet mail delivery.

## Reuse and verification

Reuses operational-family seeding, observed browser journeys, real realtime
assertions, read-only database checks, traces, videos and cached Docker images.
The inbound secret and token-bearing addresses are registered for redaction.
Browser capture-address and signature elements must be masked before rendering.
No application behavior, dependencies, migrations or contracts change.

Both authored modules passed `node --check`. Run `18502a4cca9c` passed all
9 steps with verified cleanup. Evidence: `.e2e/runs/18502a4cca9c/correspondence-1-a1/`.
Run independently with `./scripts/e2e --family correspondence`.

Additional branches not claimed here: real mail relay integration, attachment and
size limits, future-Date clamp, held-queue flood/cap, simultaneous duplicate races,
member deactivation/reactivation, multi-recipient attribution and raw crash-window
recovery. Existing backend tests cover several of these separately.

Authorities: AGENTS.md; D-015/D-050 core requirements; D-042; SLICE_009 including
its 010c/011c amendments. The operational seed deliberately avoids migration review.
