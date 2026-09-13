# Mobile 001 / 010e1 release smoke

Status: PASS against deployed main `08cb42057013cb8766ae61acb23458b0cf38c416`, after the coordinator's deployed-ready signal on 2026-09-13 at approximately 03:00 UTC. Both helpers passed on their first deployed execution. No failed deployed attempt exists to discard.

- HTTP: 94 checks; 3 temporary admin/member/other-Organization sessions, all revoked and original cookies denied with 401.
- Browser: 20 checks; 2 temporary admin/member sessions, UI logout plus independent original-cookie401; both private profiles removed.
- 110 observed asset responses (29 unique paths) matched the 73-file local manifest; no unexpected HTTP, console, runtime or request errors. Seven actual screenshots retain hashes.
- Report desktop/390px and capture390px screenshots were visually inspected: controls fit. The existing floating Operator launcher overlaps part of the bottom explanatory copy in the report390px captured scroll position; it does not obscure controls. The original screenshot is preserved.
- No source or business mutation, mobile context creation, account change or database access occurred. All 5 created sessions were revoked.

HTTP result: `http-smoke-2026-09-13T03-00-36.103390+00-00/http-results.json`; SHA-256 `909e1c2091341da0637056df357dc1ea390589bb08a6c3acc4a1702ccf879cc1`.
Browser result: `browser-smoke-2026-09-13T03-00-36.016Z/browser-results.json`; SHA-256 `3ac853fa6dd31eb5113947be3a4b417b8987940c53ddb0bdf89847551d1dbf26`.
Each evidence directory contains its own `SHA256SUMS`, request classifications, metadata hashes and exact outcomes.

```sh
python3 http_smoke.py --after-rollout-authorized
node browser_smoke.mjs --after-rollout-authorized
```

Run from this private release directory. Helpers consume coordinator-owned deployed.json, build-sha256.json and web-build-sha256.json. Python uses captured curl with verified TLS; Chromium uses the installed Playwright module and private disposable profiles. The password and cookies are never arguments or evidence fields. Every created session is deleted and the original cookie must receive 401; failed attempts remain in separately timestamped evidence directories.

Only session login/logout, GET reads and browser read delivery are permitted. No FUB connection, capture, import, report creation, activation, mobile context creation or business mutation is performed. The browser request guard blocks other application mutations; existing realtime read-token issuance is allowed. The shared empty migration workspace proves empty states and valid unknown-ID authorization denials, not populated cross-tenant report isolation. Existing implementation evidence owns populated report, task/Operator execution and native recovery behavior. Operator release coverage is its rendered empty panel only because its HTTP turns endpoint can execute commands.

The checks compare published entry and MigrationView assets to the release manifest, record actual public HTML hashes (edge telemetry may alter HTML), verify anonymous/admin/member/other-Organization responses and no-store, read People/Today, and inspect core capture/change-report empty UI at desktop and 390px, including refresh/reload. This lane does not independently prove running binary hashes, database preservation or schema compatibility; those records are coordinator-owned.
