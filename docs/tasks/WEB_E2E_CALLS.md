# Web E2E — Browser call and required outcome (catalog09)

Status: passed 9/9 steps in isolated run `18502a4cca9c`; cleanup verified empty. Syntax checks passed. No application/shared contracts changed.

Authority: AGENTS.md, D-015/D-023/D-030/D-031/D-032/D-033/D-034/D-050/D-057, SLICE_006/006b/006c, reviewed catalog09a–09c. D-033 supersedes the earlier optional/preselected outcome: the test requires no selected radio, disabled Save until selection, no Skip, and caller-owned Today work until an explicit outcome.

Owned files: `e2e/families/calls.spec.mjs`, `e2e/support/calls-seed.mjs`, this record. Seed calls-v1 reuses operational-family setup and creates one callable synthetic Person via ReceiveInquiry as Blair. Alice calls that Person, deliberately separating caller from assignee. Other-Organization Casey supplies the tenant denial.

Nine steps cover direct browser start/dial/ringing, answer-time automatic contact evidence, caller-only controls, foreign404, one-active-call protection and repeat dial, hangup, required outcome recovery after reload, caller Today responsibility, choosing and correcting a chained immutable outcome, signed duplicate terminal webhooks without duplicate completion facts, busy versus provider/microphone failure contact-credit rules, and Operator proposal dismissal/confirmation through the same lifecycle. Repeated Operator confirmation cannot create a second call. Final checks count calls, completions and contact facts, preserve Person assignment, scope realtime, and identify all controlled provider traffic.

External boundaries: the real Vue application, API, Rust LiveKit HTTP adapter, typed commands, Centrifugo and PostgreSQL execute. Coordinator-owned `e2e/livekit-browser.mjs` replaces only the external browser SDK in a separately built Web test image; real CRM adapter and useCall remain unchanged. Coordinator-owned calls-provider emulates LiveKit room/SIP endpoints and supplies controlled ring/answer/busy/provider failure/microphone denial. Operator inference uses the deterministic provider module.

This is signaling/state coverage, not WebRTC/audio, actual browser microphone permission, SIP/RTP transport, LiveKit deployment, Telnyx/PSTN, or recording coverage. The microphone case simulates SDK permission failure; it does not exercise an OS permission dialog. No outside recipient is contacted. Webhooks use synthetic E2E LiveKit key/secret and recorded HTTP evidence excludes credentials.

Required harness settings: calls-specific Web image with external SDK alias and `/__e2e/call-provider` mock route; synthetic LiveKit configuration; browser environment `E2E_LIVEKIT_KEY`/`E2E_LIVEKIT_SECRET` for signing controlled duplicate terminal webhooks; dummy Operator key and loopback provider proxy. Controls are POST mocks9000 `/call-control` (mode answered/busy/provider_error/microphone_denied; answer action with call room) and `/operator-control` call scenario.

Limits: agent disconnect/reconciliation sweep, ring timeout, media failures, authorization revocation midcall and long-running max-duration are not claimed. Durable state assertions distinguish no-contact failures from attempted calls; no assumptions about real call audio quality are made.

Checks run: node --check for family and seed passed. Coordinator owns Docker execution and final evidence.

Runtime evidence: [report](../../.e2e/runs/18502a4cca9c/calls-1-a1/html/index.html), [step outcomes](../../.e2e/runs/18502a4cca9c/calls-1-a1/steps.json), [run/image/cleanup metadata](../../.e2e/runs/18502a4cca9c/calls-1-a1/run.json). The coordinated batch exercised both families concurrently with other families.

Successful execution proves the real CRM flow against controlled external SDK/signaling boundaries. It does not prove audio, WebRTC, real LiveKit/SIP transport or OS microphone permissions.
