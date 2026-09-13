# Mobile 001 — Independent integration seam review

2026-09-12. Bounded review under D-074 and the frozen `MOBILE_001_CONTRACT.md`; this is not a second broad review of the mobile domain or a native/release completion claim.

## Boundary and outcome

Reviewed the coordinator-owned `crm-api` config, state, startup/router wiring, workspace HTTP guard, Cargo example registrations and `.env.example`. Read the receipt-key parser/redaction and mobile adapter only to verify these seams. **One actionable finding, corrected by the coordinator; no other actionable findings in this boundary.**

**F1 — CONTRACT, P2, fixed:** the original mobile 20-second timer was nested inside the outer workspace middleware. Session extraction and GET workspace admission/read checks could therefore precede the timer, contrary to the frozen total HTTP deadline. The coordinator wrapped `guard_inner` in the 20-second native timeout in `auth/workspace_http.rs`. Targeted reread confirmed the timer encloses session extraction, workspace checks and the downstream route; expiration returns the ordinary 503 `unavailable` envelope, and outer `Cache-Control: no-store` still applies. Ordinary routes retain their existing path. The redundant inner timer was still present during this reread; removing it is harmless cleanup once the outer guard owns the deadline.

## Verified conclusions

- Absent/empty `CRM_MOBILE_RECEIPT_KEYS` parses to `None`; production `AppState::new` preserves that option. Mobile adapters reject missing keys with `mobile_unavailable`, while existing Web/Operator router wiring remains intact.
- A configured key ring is parsed once, shared by `Arc`, and formatted through redacted `ReceiptKeys::Debug`. Invalid configuration returns a fixed error without echoing its value. `.env.example` contains no key material and states rotation retention and separate-key requirements.
- The API starts generation cleanup only with both database and mobile keys, on a 60-second interval. It reports closed error codes. Merely building a test router does not start that worker. Existing startup and router registrations remain additive.
- Mobile route responses and outer workspace rejection/timeout responses carry no-store. Route-local no-store also covers normal extraction/handler errors. The review did not infer cache guarantees for unrelated URLs.
- Synthetic fixture and performance examples require the explicit `test-support` feature; their registrations do not add normal routes or start them during API startup. The production constructor never selects synthetic receipt keys.

## Actual checks and limits

Ran the existing current `crm-api` unit-test binary with these exact filters; both passed:

```text
--exact config::tests::optional_mobile_ring_preserves_existing_configuration
--exact config::tests::configured_mobile_ring_is_validated_and_redacted
```

Ran `git diff --check` over the six assigned seam/configuration files: passed. Re-read the coordinator's exact outer-timeout patch and verified `ApiError::Unavailable` maps to 503 with `error: unavailable`.

No runtime source edited by this reviewer. No new full DB gate, HTTP 20-second delay experiment, cleanup-worker timing experiment, native build, shared-service restart or deployment was run here. Final compilation/integration gates and removal of the redundant inner timer remain with the owning writers; this document records source review plus the focused checks actually performed.
